use crate::{
    channel::Manifest,
    digest::Sha256,
    download::{self, Downloader},
};
use futures::{stream, StreamExt, TryStreamExt};
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tokio::fs;
use tracing::info;
use url::Url;

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
pub enum RefreshError {
    BadChecksum(Url),
    Download(download::Error),
    FileSystem(io::Error),
}

impl Display for RefreshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChecksum(url) => write!(f, "bad checksum for '{}'", url),
            Self::Download(error) => error.fmt(f),
            Self::FileSystem(error) => error.fmt(f),
        }
    }
}

impl Error for RefreshError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BadChecksum(_) => None,
            Self::Download(error) => error.source(),
            Self::FileSystem(error) => error.source(),
        }
    }
}

impl From<download::Error> for RefreshError {
    fn from(error: download::Error) -> Self {
        Self::Download(error)
    }
}

impl From<io::Error> for RefreshError {
    fn from(error: io::Error) -> Self {
        Self::FileSystem(error)
    }
}

pub struct Cache {
    path: PathBuf,
    manifest: Manifest,
}

impl Cache {
    /// Creates a cache from `path`.
    #[inline]
    #[must_use]
    pub fn new(path: PathBuf, manifest: Manifest) -> Self {
        Self { path, manifest }
    }

    /// Locates an artefact URL in the cache.
    #[inline]
    #[must_use]
    fn locate(&self, url: &Url) -> PathBuf {
        let path = self.path.join(Path::new(url.path()).as_relative());
        assert!(!path.starts_with("/"));
        path
    }

    /// Refreshes the cache.
    pub async fn refresh(
        &self,
        downloader: &Downloader,
        jobs: NonZeroUsize,
    ) -> Result<(), RefreshError> {
        info!("found {} packages", self.manifest.npackages());

        stream::iter(self.manifest.packages().flat_map(|(package, data)| {
            info!(package = package.as_str(), "found package");
            data.artefacts().flat_map(|(_, artefact)| {
                artefact
                    .url
                    .iter()
                    .map(|url| (url, artefact.hash))
                    .chain(artefact.xz_url.iter().map(|url| (url, artefact.xz_hash)))
            })
        }))
        .filter_map(|(url, hash)| async move {
            match hash {
                Some(expect) => match Sha256::from_file(&self.locate(url)).await {
                    Ok(actual) => {
                        if expect == actual {
                            info!(url = url.as_str(), "already downloaded");
                            None
                        } else {
                            Some(Ok((url, hash)))
                        }
                    }

                    Err(error) => Some(Err(error.into())),
                },

                None => Some(Ok((url, hash))),
            }
        })
        .try_for_each_concurrent(jobs.get(), |(url, hash)| async move {
            let bytes = downloader.download(url.clone()).await?;
            if let Some(expect) = hash {
                if Sha256::from_slice(&bytes) != expect {
                    return Err(RefreshError::BadChecksum(url.clone()));
                }
            }

            info!(url = url.as_str(), "downloaded");

            let destination = self.locate(url);
            fs::create_dir_all(&destination).await?;
            fs::write(destination, &bytes).await?;

            Ok::<_, RefreshError>(())
        })
        .await
    }
}
