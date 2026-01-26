//! # Miscellaneous utility functions.

use crate::actions::is_not_found;
use crate::{Error, Result};
use std::path::Path;

/// Check that path is a directory or a symlink that resolves to a directory.
///
/// # Errors
///
///   * [`Error::MissingDirectory`] not a directory or doesn’t exist.
///   * [`Error::Io`] some other problem getting info about `path`.
pub fn check_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    match path.metadata().map(|m| m.is_dir()) {
        Ok(true) => Ok(()),
        Err(error) if !is_not_found(&error) => Err(Error::Io(error)),
        _ => Err(Error::MissingDirectory(path.to_path_buf())),
    }
}
