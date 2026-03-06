//! Errors returned by the crate.

use std::io;
use std::path::PathBuf;
use std::result;
use thiserror::Error; // doesn't conflict with the enum.

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

    /// An important directory is missing
    #[error("Missing directory {0:?}")]
    MissingDirectory(PathBuf),
}

/// `Result` type for this crate.
pub type Result<T, E = Error> = result::Result<T, E>;
