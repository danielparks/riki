//! # Miscellaneous utility functions.

use crate::actions::is_not_found;
use crate::{Error, Result};
use std::fs;
use std::io::{self, Read, Seek};
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

/// Open a file and confirm that it is a file.
///
/// This reads one byte to check if the file is a directory (using `is_dir()`
/// would create a race condition.)
///
/// Returns the opened file (rewound).
///
/// # Errors
///
///   * [`io::Error`] resulting from opening the file, reading a byte, or
///     seeking to the start of the file.
pub fn open_confirmed_file<P: AsRef<Path>>(path: P) -> io::Result<fs::File> {
    let mut file = fs::File::open(path)?;
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;
    Ok(file)
}
