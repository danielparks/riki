//! Render and serve files.

// Lint configuration in Cargo.toml isn’t supported by cargo-geiger.
#![forbid(unsafe_code)]

mod embeds;
mod errors;

pub mod actions;
pub mod http;
pub mod render;
pub mod rules;

pub use errors::*;
