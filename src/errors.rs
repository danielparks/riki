//! Errors returned by the crate.

use std::io;
use std::path::PathBuf;
use std::result;
use thiserror::Error; // doesn’t conflict with the enum.

/// Error type for the crate.
#[derive(Debug, Error)]
pub enum Error {
    /// IO error
    #[error("IO error")]
    Io(#[from] io::Error),

    /// Failed to read page file
    #[error("Failed to read page file")]
    ReadPageFile {
        /// The original error.
        source: io::Error,
    },

    /// Failed to render page body with [`mustache`]
    #[error("Failed to render page body")]
    PageRender {
        /// The original error.
        source: mustache::Error,
        /// The page file.
        page: PathBuf,
        /// The template name.
        template: String,
    },

    /// Failed to compile template with [`mustache`]
    #[error("Failed to compile template at {path:?}: {source:?}")]
    TemplateCompile {
        /// The original error.
        source: mustache::Error,
        /// The template file.
        path: PathBuf,
    },

    /// Found template with name that can’t be represented in UTF-8.
    #[error("Template name is not unicode: {path:?}")]
    TemplateName {
        /// The path to the template file.
        path: PathBuf,
    },

    /// Couldn’t find named template
    #[error("Template {name:?} not found")]
    TemplateNotFound {
        /// The requested name.
        name: String,
    },

    /// Failed load static file
    #[error("Failed load static file")]
    StaticFile, // actix_web::Error doesn’t have Send, which breaks anyhow.

    /// Failed to render page metadata
    #[error("Failed to render page metadata")]
    MetadataRender {
        /// The original error
        source: serde_yaml::Error,
    },

    /// Failed to parse page metadata
    #[error("Failed to parse page metadata")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    /// Failed to bind to socket
    #[error("Failed to bind to socket on {address:?}")]
    BindError {
        /// The original error
        source: io::Error,

        /// The address the socket could not bind on
        address: String,
    },
}

/// `Result` type for this crate.
pub type Result<T, E = Error> = result::Result<T, E>;
