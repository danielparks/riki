//! Errors returned by the crate.

use crate::pages::Source;
use std::io;
use std::result;
use thiserror::Error; // doesn’t conflict with the enum.

/// Error type for the crate.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to bind to socket
    #[error("Failed to bind to socket on {address:?}")]
    BindError {
        /// The original error
        source: io::Error,

        /// The address the socket could not bind on
        address: String,
    },

    /// IO error
    #[error("IO error")]
    Io(#[from] io::Error),

    /// Failed to render page metadata
    #[error("Failed to render page metadata")]
    MetadataRender {
        /// The original error
        source: serde_yaml::Error,
    },

    /// Failed to render page body with [`handlebars`]
    #[error("Failed to render page body")]
    PageRender {
        /// The original error.
        source: handlebars::RenderError,
        /// The source of the page.
        ///
        /// Boxed to prevent the error type from getting too big.
        page_source: Box<Source>,
        /// The template name.
        template: String,
    },

    /// Failed to parse page metadata
    #[error("Failed to parse page metadata")]
    ParsePageMetadata(#[from] serde_yaml::Error),

    /// Failed to read page file
    #[error("Failed to read page file")]
    ReadPageFile {
        /// The original error.
        source: io::Error,
    },

    /// Failed to compile template with [`handlebars`]
    #[error("Failed to compile template at: {0:?}")]
    TemplateCompile(#[from] handlebars::TemplateError),
}

/// `Result` type for this crate.
pub type Result<T, E = Error> = result::Result<T, E>;
