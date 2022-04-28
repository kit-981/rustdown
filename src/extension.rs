use std::path::Path;

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

        Path::new(self.path())
            .file_name()
            .map(|s| s.to_str().expect("bad url"))
    }
}
