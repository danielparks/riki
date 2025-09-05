//! Errors returned by the crate.

use crate::pages::Source;
use std::io;
use std::path::PathBuf;
use std::result;
use thiserror::Error; // doesn’t conflict with the enum.

/// Error type for the crate.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to bind to socket
    #[error("Error binding to socket on {address:?}: {source}")]
    BindError {
        /// The original error
        source: io::Error,

        /// The address the socket could not bind on
        address: String,
    },

    /// IO error
    #[error("Error in IO: {0}")]
    Io(#[from] io::Error),

    /// Failed to render page metadata
    #[error("Error rendering page metadata: {0}")]
    MetadataRender(serde_yaml::Error),

    /// An important directory is missing
    #[error("Missing directory {0:?}")]
    MissingDirectory(PathBuf),

    /// Failed to render page body in a template with [`handlebars`]
    #[error("{source}")]
    TemplateRender {
        /// The original error.
        source: handlebars::RenderError,

        /// The source of the page.
        ///
        /// Boxed to prevent the error type from getting too big.
        page_source: Box<Source>,
    },

    /// Failed to parse page metadata
    #[error("Error parsing page metadata: {0}")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    /// Failed to read page file
    #[error("Error reading page file {path:?}: {source}")]
    ReadPageFile {
        /// The original error
        source: io::Error,

        /// The page file
        path: PathBuf,
    },

    /// Failed to compile template with [`handlebars`]
    #[error(transparent)]
    TemplateCompile(#[from] handlebars::TemplateError),
}

/// `Result` type for this crate.
pub type Result<T, E = Error> = result::Result<T, E>;
