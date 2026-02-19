//! ## Returns
//!
//! Types that implement [`Return`] can be passed from action to action for
//! processing then converted into an [`actix_web::HttpResponse`] for return to
//! the client.
//!
//!   * A [`StringReturn`] is a short string that might be part of a path, an
//!     error code, etc.
//!   * A [`RealFileReturn`] represents a path to a real file — it opens the
//!     file and holds its descriptor.
//!   * A [`ContentReturn`] holds actual content, possibly with a path and other
//!     metadata attached to it. It does not hold open a file descriptor.
//!   * [`ActionReturn`] is an enum that can hold any of the other returns.
//!
//! `StringReturn` is an [`OsString`][std::ffi::OsString] internally, so
//! non-Unicode paths are supported on platforms that allow them.
//!
//! ### Very large files
//!
//! A [`RealFileReturn`] can be transformed into a [`actix_files::NamedFile`],
//! which Actix can stream back to the client.
//!
//! However, to do any transformations, e.g. converting Markdown to HTML or
//! rendering HTML into a template, the entire file must be loaded into memory
//! as a [`ContentReturn`].

mod content_return;
mod media_type;
mod real_file_return;
mod string_return;

pub use content_return::*;
pub use media_type::*;
pub use real_file_return::*;
pub use string_return::*;

use super::{Context, Error, RequestContext, Result, VariableMap};
use actix_web::HttpResponse;
use ambassador::{Delegate, delegatable_trait};

/// Return from any action
#[derive(Debug, Delegate, derive_more::From)]
#[delegate(Return)]
pub enum ActionReturn {
    /// A short string.
    StringReturn(StringReturn),

    /// A path and an open file.
    RealFileReturn(RealFileReturn),

    /// Response body content (possibly associated with a path).
    ContentReturn(ContentReturn),
}

impl From<&str> for ActionReturn {
    fn from(string: &str) -> Self {
        StringReturn::from(string).into()
    }
}

impl From<ActionReturn> for Result {
    fn from(ret: ActionReturn) -> Self {
        Ok(ret)
    }
}

/// A return from an action
#[delegatable_trait]
pub trait Return {
    /// Ensure that the return represents a real file.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the path is not a file that can be opened.
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ActionReturn>;

    /// Convert the return to a [`StringReturn`].
    ///
    /// Uses the path from [`RealFileReturn`] and [`ContentReturn`].
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the path cannot be represented as a [`String`].
    fn into_string_return(self) -> Result<StringReturn>;

    /// Convert the return to a [`ContentReturn`].
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the path could not be read into memory.
    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn>;

    /// Generate a response (or an error)
    ///
    /// # Errors
    ///
    /// Returned errors will be converted to appropriate HTTP responses.
    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse>;
}
