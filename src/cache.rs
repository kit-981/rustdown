use crate::{
    channel::{
        manifest::{Artefact, Manifest, PackageData},
        Channel,
    },
    digest::Sha256,
    download::{self, Downloader},
    extension::{Path as PathExtension, Url as UrlExtension},
};
use ahash::{AHashMap, AHashSet};
use chrono::NaiveDate;
use futures::{stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tokio::{fs, task};
use tracing::{info, info_span};
use tracing_futures::Instrument;
use url::Url;
use walkdir::WalkDir;

trait PathExt {
    /// Returns a relative version of the path. The return value is the same if the path is already
    /// relative.
    fn as_relative(&self) -> &Path;
}

impl PathExt for Path {
    #[inline]
    #[must_use]
    fn as_relative(&self) -> &Path {
        if !self.starts_with("/") {
            return self;
        }

        self.strip_prefix("/").expect("path is not absolute")
    }
}

#[derive(Debug)]
pub enum BuildError {
    BadChecksum(Url),
    BadOverlap,
    Download(download::Error),
    FileSystem(io::Error),
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChecksum(url) => write!(f, "bad checksum for '{}'", url),
            Self::BadOverlap => write!(f, "channels have different overlapping files"),
            Self::Download(error) => error.fmt(f),
            Self::FileSystem(error) => error.fmt(f),
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BadChecksum(_) | Self::BadOverlap => None,
            Self::Download(error) => error.source(),
            Self::FileSystem(error) => error.source(),
        }
    }
}

impl From<download::Error> for BuildError {
    fn from(error: download::Error) -> Self {
        Self::Download(error)
    }
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> Self {
        Self::FileSystem(error)
    }
}

pub struct Cache {
    path: PathBuf,
    host: Url,
}

impl Cache {
    /// Creates a cache from `path`.
    #[inline]
    #[must_use]
    pub fn new(path: PathBuf, host: Url) -> Self {
        Self { path, host }
    }

    #[inline]
    #[must_use]
    fn date(channel: &Channel, manifest: &Manifest) -> NaiveDate {
        match channel {
            Channel::Stable(_) => manifest.date,
            Channel::DateBased { name: _, date } => *date,
        }
    }

    /// Returns the relative path of an archive.
    #[inline]
    #[must_use]
    fn relative_archive_path(channel: &Channel, manifest: &Manifest, name: &str) -> String {
        format!(
            "dist/{}/{}",
            Self::date(channel, manifest).format("%Y-%m-%d"),
            name
        )
    }

    /// Returns the relative manifest path for the channel.
    ///
    /// The official distribution server hosts a large number of stable manifest copies in unusual
    /// places. These are not replicated here because it is not clear how they are used.
    #[inline]
    #[must_use]
    fn relative_manifest_path(channel: &Channel, manifest: &Manifest) -> String {
        let date = Self::date(channel, manifest).format("%Y-%m-%d");
        match channel {
            Channel::Stable(version) => {
                format!("dist/channel-rust-{}.toml", version)
            }

            Channel::DateBased { name, date: _ } => {
                format!("dist/{}/channel-rust-{}.toml", date, name)
            }
        }
    }

    /// Normalises a manifest.
    ///
    /// This transformation sanitises a manifest by ensuring that every artefact resides at a
    /// consistent location relative to `root`.
    #[must_use]
    fn normalise_manifest(channel: &Channel, manifest: &Manifest, host: &Url) -> Manifest {
        Manifest {
            date: Self::date(channel, manifest),
            packages: manifest
                .packages
                .iter()
                .map(|(package, data)| {
                    (
                        package.clone(),
                        PackageData {
                            artefacts: data
                                .artefacts
                                .iter()
                                .map(|(name, artefact)| {
                                    (name.clone(), {
                                        let transform = |url: &Url| {
                                            host.join(&Self::relative_archive_path(
                                                channel,
                                                manifest,
                                                url.file_name().expect("url has no file name"),
                                            ))
                                            .expect("url cannot be joined")
                                        };

                                        Artefact {
                                            available: artefact.available,
                                            url: artefact.url.as_ref().map(transform),
                                            hash: artefact.hash,
                                            xz_url: artefact.xz_url.as_ref().map(transform),
                                            xz_hash: artefact.xz_hash,
                                        }
                                    })
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// Deletes files that should be preserved. Empty directories are removed.
    async fn prune(&self, preserve: AHashSet<PathBuf>) -> Result<(), io::Error> {
        // There are no obvious ways to prune the cache in parallel without traversing twice. For
        // instance, the decision to remove a directory is determined by previous decisions.
        //
        // Despite this, it's probably faster to first remove all undesired files in parallel before
        // synchronously deleting empty directories using a depth-first traversal.
        let root = self.path.clone();
        task::spawn_blocking(move || {
            WalkDir::new(root)
                // The contents are yielded first so that empty directories can be pruned.
                .contents_first(true)
                .into_iter()
                .try_for_each(|entry| match entry {
                    Ok(entry) => {
                        use std::fs;

                        let path = entry.path();
                        match entry.file_type() {
                            t if t.is_dir() => match fs::read_dir(path)?.next() {
                                Some(_) => Ok(()),
                                None => fs::remove_dir(path),
                            },

                            t if t.is_file() => {
                                if preserve.contains(path) {
                                    Ok(())
                                } else {
                                    fs::remove_file(path)
                                }
                            }

                            t if t.is_symlink() => fs::remove_file(path),

                            _ => unreachable!(),
                        }
                    }
                    Err(error) => Err(error.into()),
                })
        })
        .await
        .expect("panicked while pruning cache")
    }

    /// Builds a cache.
    #[allow(clippy::too_many_lines)]
    pub async fn build(
        &self,
        channels: &AHashMap<Channel, Manifest>,
        downloader: &Downloader,
        jobs: NonZeroUsize,
    ) -> Result<(), BuildError> {
        // Verify that there are no overlapping files with different checksums.
        let archives = channels
            .iter()
            .flat_map(|(channel, manifest)| {
                manifest.archives().map(|(archive, checksum)| {
                    (
                        Self::relative_archive_path(
                            channel,
                            manifest,
                            archive.file_name().expect("unnamed archive"),
                        ),
                        checksum,
                    )
                })
            })
            .try_fold(AHashMap::new(), |mut paths, (path, checksum)| {
                if let Some(found) = paths.insert(path, checksum) {
                    if checksum != found {
                        return Err(BuildError::BadOverlap);
                    }
                }

                Ok(paths)
            })?;

        info!("found {} artefacts", archives.len());

        if self.path.async_try_exists().await? {
            let preserve = archives
                .keys()
                .map(|archive| self.path.join(archive))
                .collect();

            self.prune(preserve).await?;
            info!("pruned cache");
        }

        stream::iter(channels.iter())
            .flat_map(|(channel, manifest)| {
                // TODO: We might download duplicate files more than once?
                stream::iter(manifest.archives()).map(move |(archive, hash)| {
                    async move {
                        let destination = self.path.join(Self::relative_archive_path(
                            channel,
                            manifest,
                            archive.file_name().expect("unnamed archive"),
                        ));

                        // If the file already exists then the download can be skipped.
                        if let Some(hash) = hash {
                            match Sha256::from_file(&destination).await {
                                Ok(actual) => {
                                    if *hash == actual {
                                        info!(
                                            file = archive.file_name().expect("unnamed archive"),
                                            "skipped download"
                                        );
                                        return Ok(());
                                    }
                                }
                                Err(error) => {
                                    use std::io::ErrorKind::NotFound;

                                    // Continue executing if not found.
                                    if error.kind() != NotFound {
                                        return Err(error.into());
                                    }
                                }
                            }
                        }

                        fs::create_dir_all(&destination.parent().expect("file has no parent"))
                            .await?;
                        let bytes = downloader.download(archive.clone()).await?;
                        if let Some(hash) = hash {
                            if Sha256::from_slice(&bytes) != *hash {
                                return Err(BuildError::BadChecksum(archive.clone()));
                            }
                        }

                        fs::write(destination, &bytes).await?;
                        info!(
                            file = archive.file_name().expect("unnamed archive"),
                            "downloaded",
                        );

                        Ok(())
                    }
                    .instrument(info_span!(
                        "download",
                        channel = channel.to_string().as_str()
                    ))
                })
            })
            .map(Ok)
            .try_buffer_unordered(jobs.get())
            .try_collect::<()>()
            .await?;

        let normalised: AHashMap<Channel, Manifest> = channels
            .iter()
            .map(|(channel, manifest)| {
                (
                    channel.clone(),
                    Self::normalise_manifest(channel, manifest, &self.host),
                )
            })
            .collect();

        // Install normalised channel manifests.
        stream::iter(normalised.clone())
            .map(|(channel, manifest)| async move {
                let destination = self
                    .path
                    .join(Self::relative_manifest_path(&channel, &manifest));

                fs::create_dir_all(destination.parent().expect("file has no parent")).await?;
                fs::write(destination, manifest.to_vec()).await?;

                Ok::<_, BuildError>(())
            })
            .map(Ok)
            .try_buffer_unordered(jobs.get())
            .try_collect::<()>()
            .await?;

        // Install normalised channel aliases.
        stream::iter(
            normalised
                .iter()
                .group_by(|(channel, _)| channel.name())
                .into_iter()
                .map(|(_, group)| {
                    group
                        .max_by_key(|(channel, _)| *channel)
                        .expect("missing associated channel")
                }),
        )
        .map(|(channel, manifest)| async {
            let destination = self
                .path
                .join(format!("dist/channel-rust-{}.toml", channel.name()));

            fs::create_dir_all(destination.parent().expect("file has no parent")).await?;
            fs::write(destination, manifest.to_vec()).await?;

            Ok::<_, BuildError>(())
        })
        .map(Ok)
        .try_buffer_unordered(jobs.get())
        .try_collect::<()>()
        .await?;

        Ok(())
    }
}
