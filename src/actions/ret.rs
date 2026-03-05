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
use std::borrow::Cow;
use std::fmt;

/// Return from any action
#[derive(Delegate, derive_more::From)]
#[delegate(Return)]
pub enum ActionReturn {
    /// A short string.
    StringReturn(StringReturn),

    /// A path and an open file.
    RealFileReturn(RealFileReturn),

    /// Response body content (possibly associated with a path).
    ContentReturn(ContentReturn),
}

impl fmt::Debug for ActionReturn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StringReturn(ret) => ret.fmt(f),
            Self::RealFileReturn(ret) => ret.fmt(f),
            Self::ContentReturn(ret) => ret.fmt(f),
        }
    }
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

/// # A return from an action
///
/// Returns can be evaluated two ways:
///
///   * As content. If the return is a string literal, then it is interpreted to
///     be a path and is opened.
///   * As a string or path. If the return is a string literal, then it is used
///     literally, e.g. as in `error("404")`. If the return is content, then its
///     inner path is used, if available.
///
/// ## Paths
///
/// There are a few different path types relevant to returns:
///
///   * **Inner path:** this is the path built up in the configuration file to
///     identify the resource to return. It might be relative to the working
///     directory, or absolute.
///   * **URL path:** this is the path requested from the web server that will
///     retrieve the resource.
///   * **Real, or file system path:** the absolute path to the resource on the
///     file system.
#[delegatable_trait]
pub trait Return {
    /// Get this return’s inner path built up by configuration.
    ///
    /// This might be absolute, or it might be relative to the working directory
    /// (in which case it will not start with a slash).
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the return was a file without a path.
    fn inner_path(&self) -> Result<&str>;

    /// Ensure that the return represents a real file.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the path is not a file that can be opened.
    fn ensure_file<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<ActionReturn>;

    /// Convert the return to a [`StringReturn`].
    ///
    /// Uses the path from [`RealFileReturn`] and [`ContentReturn`].
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the return was a file without a path.
    fn into_string_return(self) -> Result<StringReturn>;

    /// Convert the return to a [`ContentReturn`].
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the path could not be read into memory.
    fn into_content_return<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<ContentReturn>;

    /// Generate a response (or an error).
    ///
    /// # Errors
    ///
    /// Returned errors will be converted to appropriate HTTP responses.
    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse>;

    /// Get the URL path.
    ///
    /// This is the path that returns this resource when requested from the
    /// server. It must always start with a slash.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the return was a file without a path.
    fn url_path(&self) -> Result<Cow<'_, str>> {
        self.inner_path().map(|path| {
            if path.starts_with('/') {
                path.into()
            } else {
                format!("/{path}").into()
            }
        })
    }

    /// Try to evaluate as a string.
    ///
    /// This interprets the return as a short string, e.g. an error code, or
    /// a literal response (`literal("everything is fine")`).
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the return was a file without a path, or the
    /// short string or path was not UTF-8.
    fn into_string(self) -> Result<String>
    where
        Self: Sized,
    {
        Ok(self.into_string_return()?.into())
    }
}
