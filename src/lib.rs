//! Render and serve files.

// Lint configuration in Cargo.toml isn't supported by cargo-geiger.
#![forbid(unsafe_code)]

mod errors;
pub use errors::*;

mod tests;

pub mod http;
pub mod rules;

pub use riki_actions as actions;
pub use riki_config as config;
pub use riki_misc as misc;
pub use riki_render as render;
