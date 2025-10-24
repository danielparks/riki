//! Render and serve files.

// Lint configuration in Cargo.toml isn’t supported by cargo-geiger.
#![forbid(unsafe_code)]

mod embeds;
mod errors;

pub mod actions;
pub mod config;
pub mod http;
pub mod misc;
pub mod render;
pub mod rules;

pub use errors::*;
