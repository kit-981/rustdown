use crate::{
    channel::Manifest,
    download::{self, Download},
};
use futures::{stream, TryStreamExt};
use reqwest::Client;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tracing::{info, info_span};
use tracing_futures::Instrument;
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
        client: &Client,
        options: download::Options,
        jobs: NonZeroUsize,
    ) -> Result<(), download::Error> {
        info!("found {} packages", self.manifest.npackages());
        stream::iter(self.manifest.packages().flat_map(|(package, data)| {
            info!(
                package = package.as_str(),
                "found {} artefacts",
                data.nartefacts()
            );

            data.artefacts()
                .filter(|(_, artefact)| artefact.available)
                .flat_map(move |(target, artefact)| {
                    [
                        artefact.url.as_ref().map(|url| (url, artefact.hash)),
                        artefact.xz_url.as_ref().map(|url| (url, artefact.xz_hash)),
                    ]
                    .into_iter()
                    .flatten()
                    .map(move |(url, hash)| {
                        (
                            target,
                            artefact,
                            Download {
                                url: url.clone(),
                                destination: self.locate(url),
                                checksum: hash,
                            },
                        )
                    })
                })
                .map(move |(target, _, download)| async move {
                    download
                        .run(client, options)
                        .instrument(info_span!(
                            "download",
                            package = package.as_str(),
                            target = target.as_str(),
                        ))
                        .await
                })
                .map(Ok)
        }))
        .try_for_each_concurrent(jobs.get(), |download| download)
        .await
    }
}
