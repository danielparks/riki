//! # Actions to generate and process data within the web server.
#![doc = read_doc::module!("context.rs", "errors.rs", "functions.rs", "ret.rs")]
// Lint configuration in Cargo.toml isn't supported by cargo-geiger.
#![forbid(unsafe_code)]

mod context;
pub use context::*;

mod errors;
pub use errors::*;

pub mod elements;
pub mod pages;

mod functions;
pub use functions::*;

mod ret;
pub use ret::*;

mod tests;
