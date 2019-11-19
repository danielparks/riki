use std::io;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("failed rendering page body")]
    PageRender(#[from] mustache::Error),

    #[error("failed rendering page metadata")]
    MetadataRender(#[from] serde_yaml::Error),

    #[error("failed reading page {path:?}")]
    ReadPageFile { source: io::Error, path: PathBuf },

    #[error("failed parsing page metadata in {path:?}")]
    ParsePageMetadata { source: serde_yaml::Error, path: PathBuf },
}

impl MyError {
    #[allow(non_snake_case)]
    pub fn ReadPageFileMap(path: &Path) -> Box<dyn FnOnce(io::Error) -> MyError> {
        let path = PathBuf::from(path);
        Box::new(|source: io::Error| MyError::ReadPageFile { source, path })
    }
}

pub type Result<T, E = MyError> = std::result::Result<T, E>;
