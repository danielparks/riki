use std::io;
use std::result;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("Failed to read page file")]
    ReadPageFile { source: io::Error },

    #[error("Failed to render page body")]
    PageRender(#[from] mustache::Error),

    #[error("Failed to render page metadata")]
    MetadataRender { source: serde_yaml::Error },

    #[error("Failed to parse page metadata")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    #[error("Failed to bind to socket on {address:?}")]
    BindError { source: io::Error, address: String },
}

impl MyError {
    #[allow(non_snake_case)]
    pub fn BindErrorMap(address: &str) -> Box<dyn FnOnce(io::Error) -> MyError> {
        let address = String::from(address);
        Box::new(|source: io::Error| MyError::BindError { source, address })
    }
}

pub type Result<T, E = MyError> = result::Result<T, E>;
