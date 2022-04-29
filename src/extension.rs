use async_trait::async_trait;
use std::io;
use tokio::fs;

pub trait Url {
    /// Returns the file name.
    #[must_use]
    fn file_name(&self) -> Option<&str>;
}

impl Url for url::Url {
    #[must_use]
    fn file_name(&self) -> Option<&str> {
        if self.cannot_be_a_base() {
            return None;
        }

        std::path::Path::new(self.path())
            .file_name()
            .map(|s| s.to_str().expect("bad url"))
    }
}

#[async_trait]
pub trait Path {
    /// Returns whether or not the path exists.
    async fn async_try_exists(&self) -> Result<bool, io::Error>;
}

#[async_trait]
impl Path for std::path::Path {
    async fn async_try_exists(&self) -> Result<bool, io::Error> {
        match fs::metadata(self).await {
            Ok(_) => Ok(true),
            Err(error) => {
                use io::ErrorKind::NotFound;
                match error.kind() {
                    NotFound => Ok(false),
                    _ => Err(error),
                }
            }
        }
    }
}
