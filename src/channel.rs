use crate::digest::Sha256;
use ahash::AHashMap;
use serde::Deserialize;
use url::Url;

/// Represents an artefact.
///
/// # Limitations
///
/// Ideally, this type should be broken up into two with the `available` field acting as a tag but
/// serde doesn't support non-string tags (<https://github.com/serde-rs/serde/issues/745>).
#[derive(Deserialize)]
pub struct Artefact {
    pub available: bool,
    pub url: Option<Url>,
    pub hash: Option<Sha256>,
    pub xz_url: Option<Url>,
    pub xz_hash: Option<Sha256>,
}

/// Represents data belonging to a package.
#[derive(Deserialize)]
pub struct PackageData {
    #[serde(rename = "target")]
    artefacts: AHashMap<String, Artefact>,
}

impl PackageData {
    /// Returns the artefacts in the package data.
    pub fn artefacts(&self) -> impl Iterator<Item = (&String, &Artefact)> {
        self.artefacts.iter()
    }
}

/// Represents a channel manifest.
#[derive(Deserialize)]
pub struct Manifest {
    #[serde(rename = "pkg")]
    pub packages: AHashMap<String, PackageData>,
}

impl Manifest {
    /// Returns the number of packages in the manifest.
    pub fn npackages(&self) -> usize {
        self.packages.len()
    }

    /// Returns the packages in the manifest.
    pub fn packages(&self) -> impl Iterator<Item = (&String, &PackageData)> {
        self.packages.iter()
    }

    /// Deserialises a manifest from a slice.
    pub fn from_slice(slice: &[u8]) -> Result<Self, toml::de::Error> {
        toml::from_slice(slice)
    }
}
