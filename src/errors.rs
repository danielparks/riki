use std::io;
use std::result;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("failed rendering page body")]
    PageRender(#[from] mustache::Error),

    #[error("failed rendering page metadata")]
    MetadataRender { source: serde_yaml::Error },

    #[error("failed parsing page metadata")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    #[error("failed reading page file")]
    ReadPageFile { source: io::Error },

    #[error("failed to bind to socket on {address:?}")]
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
