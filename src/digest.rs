use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    fmt::{self, Display, Formatter},
    io,
    path::Path,
};
use tokio::{fs::File, io::AsyncReadExt};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct Sha256(#[serde(with = "hex")] pub [u8; 32]);

impl Sha256 {
    #[inline]
    #[must_use]
    pub fn from_slice(s: &[u8]) -> Self {
        Self(sha2::Sha256::digest(s).into())
    }

    pub async fn from_file(path: &Path) -> Result<Self, io::Error> {
        let mut bytes = Vec::new();
        let mut file = File::open(path).await?;
        file.read_to_end(&mut bytes).await?;
        Ok(Self::from_slice(&bytes))
    }
}

impl Display for Sha256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
