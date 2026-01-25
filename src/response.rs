//! A draft response that can be further processed or returned to the client.
//!
//! These can be passed from action to action for processing, and then converted
//! into an [`HttpResponse`] and returned to the client.
//!
//!   * A [`PathReturn`] represents a path of the firesystem that is guaranteed
//!     to be a file — it opens the file and holds its descriptor.
//!   * A [`ContentReturn`] holds actual content, possibly with a path and other
//!     metadata attached to it. It does not hold open a file descriptor.
//!
//! ### Very large files
//!
//! A [`PathReturn`] can be transformed into a [`NamedFile`], which Actix can
//! stream back to the client.
//!
//! However, to do any transformations, e.g. converting Markdown to HTML or
//! rendering HTML into a template, the entire file must be loaded into memory
//! as a [`ContentReturn`].

use crate::http::{WebError, WebResult, util};
use crate::{Error, Result};
use actix_files::NamedFile;
use actix_web::body::BodySize;
use actix_web::http::header::{
    HeaderValue, InvalidHeaderValue, TryIntoHeaderValue,
};
use actix_web::web::Bytes;
use actix_web::{HttpRequest, HttpResponse};
use jiff::Timestamp;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs::File;
use std::io::{self, Read};
use std::mem;
use std::path::PathBuf;
use std::pin::Pin;
use std::task;
use tendril::StrTendril;

/// A file on the file system.
#[derive(Debug)]
pub struct PathReturn {
    /// Path to the file.
    pub path: PathBuf,

    /// The open file object.
    pub file: File,

    /// Time the file was last modified.
    pub modified: Option<Timestamp>,

    /// Time the file was created.
    pub created: Option<Timestamp>,
}

impl PathReturn {
    /// Create a `PathReturn` from a path.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the path doesn’t exist, isn’t a file, or
    /// otherwise couldn’t be read.
    pub fn new(path: PathBuf) -> io::Result<Self> {
        // FIXME? move out of util?
        let file = util::open_confirmed_file(&path)?;
        let metadata = file.metadata().ok();

        Ok(Self {
            path,
            file,
            modified: metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
            created: metadata
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
        })
    }

    /// Convert to a [`NamedFile`].
    ///
    /// This makes all `text/*` files default to having `charset=utf-8`. If the
    /// media type is not `text`, or if it already has a `charset` parameter,
    /// then the media type will be left as-is.
    ///
    /// # Errors
    ///
    ///   * Mapped `io::Error`s from [`NamedFile::from_file()`].
    ///   * [`WebError::InternalString`] if the constructed content-type cannot
    ///     be parsed (should never happen).
    fn into_named_file(self) -> WebResult<NamedFile> {
        let file = NamedFile::from_file(self.file, &self.path)?;
        let content_type = file.content_type();
        if content_type.type_() != mime::TEXT
            || content_type.params().any(|(name, _)| name == mime::CHARSET)
        {
            return Ok(file);
        }

        let new_content_type = format!("{}; charset=utf-8", &content_type);

        Ok(file.set_content_type(new_content_type.parse().map_err(
            |error| {
                WebError::InternalString(format!(
                    "Parsing constructed content type {new_content_type:?}: \
                        {error}"
                ))
            },
        )?))
    }
}

impl Return for PathReturn {
    fn into_content_return(self) -> WebResult<ContentReturn> {
        let Self { mut file, path, created, modified } = self;

        let mut body = String::with_capacity(
            file.metadata()?
                .len()
                .try_into()
                .map_err(|_| Error::FileTooLarge(path.clone()))?,
        );

        #[expect(
            clippy::verbose_file_reads,
            reason = "required by code that checks if path is a file"
        )]
        file.read_to_string(&mut body)?;

        Ok(ContentReturn {
            body: body.into(),
            content_type: MediaType::APPLICATION_OCTET_STREAM,
            source: Source::File { path, modified, created },
            metadata: Metadata::new(),
        })
    }

    fn into_response(self, req: &HttpRequest) -> WebResult<HttpResponse> {
        Ok(self.into_named_file()?.into_response(req))
    }
}

/// A draft response that can be further processed or returned to the client.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ContentReturn {
    /// The body of the response
    pub body: Content,

    /// The content type of the response body
    pub content_type: MediaType,

    /// The original source of the response
    pub source: Source,

    /// Other metadata that may be useful to renderers.
    pub metadata: Metadata,
}

impl ContentReturn {
    /// Try to load the title from HTML if it’s not in the metadata.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotUtf8`] if the content is not UTF-8.
    pub fn ensure_metadata_title(&mut self) -> Result<()> {
        if !self.metadata.contains_key("title") {
            let fragment = dom_query::Document::fragment(StrTendril::try_from(
                self.body.clone(),
            )?);
            let h1 = fragment.select_single("h1");
            if h1.length() > 0 {
                self.metadata.insert("title".into(), h1.text().into());
            }
        }
        Ok(())
    }
}

impl Return for ContentReturn {
    fn into_content_return(self) -> WebResult<ContentReturn> {
        Ok(self)
    }

    fn into_response(self, _req: &HttpRequest) -> WebResult<HttpResponse> {
        Ok(HttpResponse::Ok()
            .content_type(&self.content_type)
            .body(self.into_content_return()?.body))
    }
}

/// A return from an action
pub trait Return {
    /// Ensure the return is a [`ContentReturn`].
    ///
    /// # Returns
    ///
    ///   * `Ok(Some(ret))` for a regular response
    ///   * `Ok(None)` to fall through to the next rule
    ///   * <code>Err([WebError])</code> for an error to be converted into an
    ///     appropriate HTTP response.
    #[expect(clippy::missing_errors_doc, reason = "Returns is more useful")]
    fn into_content_return(self) -> WebResult<ContentReturn>;

    /// Generate a response (or an error)
    ///
    /// # Errors
    ///
    /// Returned errors will be converted to appropriate HTTP responses.
    fn into_response(self, req: &HttpRequest) -> WebResult<HttpResponse>;
}

/// Content for the response.
///
/// Can either be binary or a UTF-8 string. Use [`Self::ensure_string()`] to
/// ensure that `Content::Binary` doesn’t hold valid UTF-8.
#[derive(Clone, Debug, Serialize, derive_more::From)]
#[serde(untagged)]
pub enum Content {
    /// UTF-8 content
    String(String),

    /// Binary content
    Bytes(Vec<u8>),
}

impl Content {
    /// Get the length of the content in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::String(string) => string.len(),
            Self::Bytes(bytes) => bytes.len(),
        }
    }

    /// Is this content empty?
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Try to get the content as a `String`.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotUtf8` if the content isn’t valid UTF-8.
    pub fn into_string(self) -> Result<String> {
        String::try_from(self)
    }

    /// Ensure that the content is a `String`.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotUtf8` if the content isn’t valid UTF-8.
    pub fn ensure_string(&mut self) -> Result<&String> {
        match self {
            Self::String(string) => Ok(string),
            Self::Bytes(vec) => {
                *self = Self::String(String::from_utf8(mem::take(vec))?);
                match self {
                    Self::String(string) => Ok(string),
                    Self::Bytes(_) => unreachable!("just set to String"),
                }
            }
        }
    }

    /// Get the content as `&str` or panic.
    ///
    /// # Panics
    ///
    /// Panics if the content isn’t stored as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::String(string) => string.as_str(),
            Self::Bytes(_) => panic!("Content is not UTF-8"),
        }
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::String(String::default())
    }
}

impl actix_http::body::MessageBody for Content {
    type Error = Infallible;

    #[inline]
    fn size(&self) -> BodySize {
        BodySize::Sized(self.len().try_into().expect("usize into u64"))
    }

    #[inline]
    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Result<Bytes, Self::Error>>> {
        if self.is_empty() {
            task::Poll::Ready(None)
        } else {
            task::Poll::Ready(Some(Ok(mem::take(self.get_mut()).into())))
        }
    }

    #[inline]
    fn try_into_bytes(self) -> Result<Bytes, Self> {
        match self {
            Self::String(string) => Ok(Bytes::from(string)),
            Self::Bytes(vec) => Ok(Bytes::from(vec)),
        }
    }
}

impl From<Content> for actix_web::web::Bytes {
    fn from(content: Content) -> Self {
        match content {
            Content::String(string) => string.into(),
            Content::Bytes(vec) => vec.into(),
        }
    }
}

impl TryFrom<Content> for StrTendril {
    type Error = Error;

    fn try_from(content: Content) -> Result<Self, Self::Error> {
        String::try_from(content).map(Into::into)
    }
}

impl TryFrom<Content> for String {
    type Error = Error;

    fn try_from(content: Content) -> Result<Self, Self::Error> {
        match content {
            Content::String(string) => Ok(string),
            Content::Bytes(vec) => Ok(Self::from_utf8(vec)?),
        }
    }
}

impl From<StrTendril> for Content {
    fn from(input: StrTendril) -> Self {
        Self::String(input.to_string())
    }
}

impl From<&str> for Content {
    fn from(input: &str) -> Self {
        Self::String(input.to_owned())
    }
}

impl From<&[u8]> for Content {
    fn from(input: &[u8]) -> Self {
        Self::Bytes(input.to_owned())
    }
}

impl<const N: usize> From<&[u8; N]> for Content {
    fn from(input: &[u8; N]) -> Self {
        Self::Bytes(input.to_vec())
    }
}

/// A MIME/Media type
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MediaType(pub &'static str);

impl MediaType {
    /// `text/html; charset=utf-8`
    pub const TEXT_HTML_UTF8: Self = Self("text/html; charset=utf-8");

    /// `text/markdown; charset=utf-8`
    pub const TEXT_MARKDOWN_UTF8: Self = Self("text/markdown; charset=utf-8");

    /// `text/plain; charset=utf-8`
    pub const TEXT_PLAIN_UTF8: Self = Self("text/plain; charset=utf-8");

    /// `application/octet-stream`
    pub const APPLICATION_OCTET_STREAM: Self = Self("application/octet-stream");
}

impl Default for MediaType {
    fn default() -> Self {
        Self::APPLICATION_OCTET_STREAM
    }
}

impl TryIntoHeaderValue for &MediaType {
    type Error = InvalidHeaderValue;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        HeaderValue::from_str(self.0)
    }
}

/// The source of a [`ContentReturn`].
///
/// ### Use in templates
///
/// This will be available in the template as `{{source}}`. To access variant
/// fields, use <code>source.<i>variant</i>.<i>field</i></code>.
///
/// For example:
///
/// ```hbs
/// {{#if source.File.modified}}
///     <p>Last updated {{ source.File.modified }}</p>
/// {{/if}}
/// ```
#[derive(Clone, Debug, Default, Serialize)]
pub enum Source {
    /// From memory.
    #[default]
    Memory,

    /// From stdin.
    Stdin,

    /// From a file.
    File {
        /// Path to the file.
        ///
        /// Access in templates with `{{source.File.path}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        path: PathBuf,

        /// Time the file was last modified.
        ///
        /// Access in templates with `{{source.File.modified}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        modified: Option<Timestamp>,

        /// Time the file was created.
        ///
        /// Access in templates with `{{source.File.created}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        created: Option<Timestamp>,
    },
}

impl Source {
    /// Get the last modified time, if available.
    #[must_use]
    pub const fn modified(&self) -> Option<Timestamp> {
        match self {
            Self::File { modified, .. } => *modified,
            _ => None,
        }
    }

    /// Get the creation time, if available.
    #[must_use]
    pub const fn created(&self) -> Option<Timestamp> {
        match self {
            Self::File { created, .. } => *created,
            _ => None,
        }
    }
}

/// Page metadata.
///
/// This is the YAML data in the page header, for example:
///
/// ```text
/// class: blog-post
///
/// ---
///
/// # How to use riki to host your web site
///
/// ...
/// ```
///
/// If not present, the `title` will be set to the contents of the first `<h1>`
/// on the page.
pub type Metadata = HashMap<String, String>;
