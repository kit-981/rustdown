use chrono::NaiveDate;
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    str::FromStr,
};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ParseVersionError {
    InvalidComponent(String),
    MissingMajor,
    MissingMinor,
    MissingSubminor,
    TrailingCharacters,
}

impl Display for ParseVersionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComponent(s) => write!(f, "invalid component '{}'", s),
            Self::MissingMajor => write!(f, "missing major"),
            Self::MissingMinor => write!(f, "missing minor"),
            Self::MissingSubminor => write!(f, "missing subminor"),
            Self::TrailingCharacters => write!(f, "trailing characters"),
        }
    }
}

impl Error for ParseVersionError {}

#[derive(Debug)]
pub struct Version {
    pub major: usize,
    pub minor: usize,
    pub subminor: usize,
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.subminor)
    }
}

impl FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split('.');
        let mut components = split.by_ref().take(3).map(|s| {
            s.parse::<usize>()
                .map_err(|_| ParseVersionError::InvalidComponent(s.to_string()))
        });

        let major = components.next().ok_or(ParseVersionError::MissingMajor)??;
        let minor = components.next().ok_or(ParseVersionError::MissingMinor)??;
        let subminor = components
            .next()
            .ok_or(ParseVersionError::MissingSubminor)??;

        if split.next().is_some() {
            return Err(ParseVersionError::TrailingCharacters);
        }

        Ok(Self {
            major,
            minor,
            subminor,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseChannelError {
    InvalidDate(chrono::format::ParseError),
    InvalidVersion(ParseVersionError),
    MissingName,
    MissingVersion,
    MissingDate,
}

impl Display for ParseChannelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDate(_) => write!(f, "invalid date"),
            Self::InvalidVersion(_) => write!(f, "invalid version"),
            Self::MissingName => write!(f, "missing name"),
            Self::MissingVersion => write!(f, "missing version"),
            Self::MissingDate => write!(f, "missing date"),
        }
    }
}

impl Error for ParseChannelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDate(error) => Some(error),
            Self::InvalidVersion(error) => Some(error),
            _ => None,
        }
    }
}

impl From<chrono::format::ParseError> for ParseChannelError {
    fn from(error: chrono::format::ParseError) -> Self {
        Self::InvalidDate(error)
    }
}

impl From<ParseVersionError> for ParseChannelError {
    fn from(error: ParseVersionError) -> Self {
        Self::InvalidVersion(error)
    }
}

#[derive(Debug)]
pub enum Channel {
    Stable(Version),
    DateBased { name: String, date: NaiveDate },
}

impl Channel {
    pub fn name(&self) -> &str {
        match self {
            Self::Stable(_) => "stable",
            Self::DateBased { name, date: _ } => name,
        }
    }
}

impl FromStr for Channel {
    type Err = ParseChannelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut components = s.split(':');
        let name = components.next().ok_or(ParseChannelError::MissingName)?;

        match name {
            "stable" => Ok(Self::Stable(Version::from_str(
                components.next().ok_or(ParseChannelError::MissingVersion)?,
            )?)),
            _ => Ok(Channel::DateBased {
                name: name.into(),
                date: NaiveDate::parse_from_str(
                    components.next().ok_or(ParseChannelError::MissingDate)?,
                    "%Y-%m-%d",
                )?,
            }),
        }
    }
}

pub mod manifest {
    use crate::digest::Sha256;
    use ahash::AHashMap;
    use chrono::NaiveDate;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;
    use url::Url;

    /// Represents an artefact.
    ///
    /// # Limitations
    ///
    /// Ideally, this type should be broken up into two with the `available` field acting as a tag but
    /// serde doesn't support non-string tags (<https://github.com/serde-rs/serde/issues/745>).
    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
    pub struct Artefact {
        pub available: bool,
        pub url: Option<Url>,
        pub hash: Option<Sha256>,
        pub xz_url: Option<Url>,
        pub xz_hash: Option<Sha256>,
    }

    /// Represents data belonging to a package.
    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
    pub struct PackageData {
        #[serde(rename = "target")]
        pub artefacts: BTreeMap<String, Artefact>,
    }

    /// Represents a channel manifest.
    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    pub struct Manifest {
        pub date: NaiveDate,
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

        /// Serialises a manifest into a vector of bytes.
        pub fn to_vec(&self) -> Vec<u8> {
            toml::to_vec(self).expect("failed to serialise manifest")
        }
    }
}
