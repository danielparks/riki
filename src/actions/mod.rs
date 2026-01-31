//! # Actions to generate and process data within the web server.
#![doc = read_doc::module!("context.rs", "errors.rs", "functions.rs", "ret.rs")]

mod context;
pub use context::*;

mod errors;
pub use errors::*;

mod functions;
pub use functions::*;

mod ret;
pub use ret::*;
