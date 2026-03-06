//! Template management for riki.

// Lint configuration in Cargo.toml isn't supported by cargo-geiger.
#![forbid(unsafe_code)]

mod embeds;
mod error;
mod templates;

pub use error::*;
pub use templates::*;
