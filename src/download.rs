use bytes::Bytes;
use std::{
    convert::Into,
    fmt::{self, Display, Formatter},
};
use url::Url;

#[derive(Clone, Debug, Default)]
pub struct HttpDownloader {
    client: reqwest::Client,
}

impl HttpDownloader {
    #[inline]
    pub async fn download(&self, source: Url) -> Result<Bytes, reqwest::Error> {
        self.client.get(source).send().await?.bytes().await
    }
}

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    UnsupportedUrlScheme(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reqwest(error) => error.fmt(f),
            Self::UnsupportedUrlScheme(scheme) => write!(f, "unsupported url scheme '{}'", scheme),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reqwest(error) => error.source(),
            Self::UnsupportedUrlScheme(_) => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Reqwest(error)
    }
}

/// A downloader can be used to download files.
#[derive(Debug, Default)]
pub struct Downloader {
    http: HttpDownloader,
}

impl Downloader {
    pub async fn download(&self, source: Url) -> Result<Bytes, Error> {
        match source.scheme() {
            "http" | "https" => self.http.download(source).await.map_err(Into::into),
            scheme => Err(Error::UnsupportedUrlScheme(scheme.to_string())),
        }
    }
}
