//! Miscellaneous support code.

// Lint configuration in Cargo.toml isn't supported by cargo-geiger.
#![forbid(unsafe_code)]

pub mod bitfilter;
mod header_map_helper;

pub use header_map_helper::HeaderMapHelper;
