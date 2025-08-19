use std::io;
use std::result;
use thiserror::Error; // doesn’t conflict with the enum.

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("Failed to read page file")]
    ReadPageFile { source: io::Error },

    #[error("Failed to render page body")]
    PageRender(#[from] mustache::Error),

    #[error("Failed load static file")]
    StaticFile, // actix_web::Error doesn’t have Send, which breaks anyhow.

    #[error("Failed to render page metadata")]
    MetadataRender { source: serde_yaml::Error },

    #[error("Failed to parse page metadata")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    #[error("Failed to bind to socket on {address:?}")]
    BindError { source: io::Error, address: String },
}

impl Error {
    #[allow(non_snake_case)]
    #[must_use]
    pub fn BindErrorMap(address: &str) -> Box<dyn FnOnce(io::Error) -> Self> {
        let address = String::from(address);
        Box::new(|source: io::Error| Self::BindError { source, address })
    }
}

pub type Result<T, E = Error> = result::Result<T, E>;
