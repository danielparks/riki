//! Errors returned by [`riki_render`][crate].

use std::path::PathBuf;
use std::result;
use thiserror::Error;

/// Error type for [`riki_render`][crate].
#[derive(Debug, Error)]
pub enum Error {
    /// An important directory is missing.
    #[error("Missing directory {0:?}")]
    MissingDirectory(PathBuf),

    /// Failed to compile a template with [`handlebars`].
    #[error(transparent)]
    TemplateCompile(#[from] handlebars::TemplateError),
}

/// `Result` type for [`riki_render`][crate].
pub type Result<T, E = Error> = result::Result<T, E>;
