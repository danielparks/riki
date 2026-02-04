//! ## Returns
//!
//! Types that implement [`Return`] can be passed from action to action for
//! processing then converted into an [`actix_web::HttpResponse`] for return to
//! the client.
//!
//!   * A [`StringReturn`] is a short string that might be part of a path, an
//!     error code, etc.
//!   * A [`PathReturn`] represents a path of the firesystem that is guaranteed
//!     to be a file — it opens the file and holds its descriptor.
//!   * A [`ContentReturn`] holds actual content, possibly with a path and other
//!     metadata attached to it. It does not hold open a file descriptor.
//!   * [`ActionReturn`] is an enum that can hold any of the other returns.
//!
//! ### Very large files
//!
//! A [`PathReturn`] can be transformed into a [`actix_files::NamedFile`], which
//! Actix can stream back to the client.
//!
//! However, to do any transformations, e.g. converting Markdown to HTML or
//! rendering HTML into a template, the entire file must be loaded into memory
//! as a [`ContentReturn`].

use super::{Context, Error, RequestContext, Result, VariableMap};
use actix_files::NamedFile;
use actix_web::HttpResponse;
use actix_web::body::BodySize;
use actix_web::http::header::{
    HeaderValue, InvalidHeaderValue, TryIntoHeaderValue,
};
use actix_web::web::Bytes;
use jiff::Timestamp;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::task;
use tendril::StrTendril;

/// Return from any action
#[derive(Debug, derive_more::From)]
pub enum ActionReturn {
    /// A short string.
    StringReturn(StringReturn),

    /// A path to an readable file.
    PathReturn(PathReturn),

    /// Response body content (possibly associated with a path).
    ContentReturn(ContentReturn),
}

impl Return for ActionReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        match self {
            Self::StringReturn(ret) => ret.ensure_file(context),
            Self::PathReturn(_) | Self::ContentReturn(_) => Ok(self),
        }
    }

    fn into_string_return(self) -> Result<StringReturn> {
        match self {
            Self::StringReturn(ret) => Ok(ret),
            Self::PathReturn(ret) => ret.into_string_return(),
            Self::ContentReturn(ret) => ret.into_string_return(),
        }
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        match self {
            Self::StringReturn(ret) => ret.into_content_return(context),
            Self::PathReturn(ret) => ret.into_content_return(context),
            Self::ContentReturn(ret) => Ok(ret),
        }
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        match self {
            Self::StringReturn(ret) => ret.into_response(context),
            Self::PathReturn(ret) => ret.into_response(context),
            Self::ContentReturn(ret) => ret.into_response(context),
        }
    }
}

impl From<&str> for ActionReturn {
    fn from(string: &str) -> Self {
        StringReturn::from(string).into()
    }
}

impl From<StringReturn> for Result {
    fn from(ret: StringReturn) -> Self {
        Ok(ret.into())
    }
}

impl From<PathReturn> for Result {
    fn from(ret: PathReturn) -> Self {
        Ok(ret.into())
    }
}

impl From<ContentReturn> for Result {
    fn from(ret: ContentReturn) -> Self {
        Ok(ret.into())
    }
}

/// A short string.
#[derive(Debug)]
pub struct StringReturn(String);

impl StringReturn {
    /// Get the value as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Append `suffix` if this doesn’t already end with it.
    #[must_use]
    pub fn ensure_ends_with<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();
        if !self.0.ends_with(suffix) {
            self.0.push_str(suffix);
        }
        self
    }

    /// Append `suffix` to the value.
    #[must_use]
    pub fn append<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();
        self.0.push_str(suffix);
        self
    }

    /// Strip everything from the last `'/'` on.
    ///
    /// If there are no `'/'`s, this returns `""`.
    #[must_use]
    pub fn dirname(mut self) -> Self {
        self.0.truncate(self.0.rfind('/').unwrap_or_default());
        self
    }

    /// Append `suffix` after a `'/'`.
    ///
    /// Ensures there is only on `'/'`.
    ///
    /// Note that if `suffix` starts with `'/'`, this will still append it.
    #[must_use]
    pub fn join<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();

        if self.0.ends_with('/') {
            if let Some(stripped) = suffix.strip_prefix('/') {
                self.0.push_str(stripped);
            } else {
                self.0.push_str(suffix);
            }
        } else if suffix.starts_with('/') {
            self.0.push_str(suffix);
        } else {
            self.0.push('/');
            self.0.push_str(suffix);
        }
        self
    }
}

impl Return for StringReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        Ok(PathReturn::new(self.into(), context)?.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(self)
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        PathReturn::new(self.into(), context)?.into_content_return(context)
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        PathReturn::new(self.into(), context)?.into_response(context)
    }
}

impl From<&str> for StringReturn {
    fn from(string: &str) -> Self {
        Self(string.to_owned())
    }
}

impl From<String> for StringReturn {
    fn from(string: String) -> Self {
        Self(string)
    }
}

impl From<StringReturn> for String {
    fn from(ret: StringReturn) -> Self {
        ret.0
    }
}

impl From<StringReturn> for PathBuf {
    fn from(ret: StringReturn) -> Self {
        ret.0.into()
    }
}

impl AsRef<str> for StringReturn {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

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
    pub fn new<'a, V: VariableMap<'a>>(
        path: PathBuf,
        context: &'a Context<'a, V>,
    ) -> io::Result<Self> {
        let file = open_confirmed_file(context.real_path(&path))?;
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
    ///   * [`Error::InternalString`] if the constructed content-type cannot be
    ///     parsed (should never happen).
    fn into_named_file(self) -> Result<NamedFile> {
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
                Error::InternalString(format!(
                    "Parsing constructed content type {new_content_type:?}: \
                        {error}"
                ))
            },
        )?))
    }
}

impl Return for PathReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        _context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        Ok(self.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        // FIXME should this be OsStringReturn?
        Ok(self
            .path
            .to_str()
            .ok_or_else(|| {
                Error::InternalString(
                    "Could not convert non UTF-8 path to String".to_owned(),
                )
            })?
            .to_owned()
            .into())
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        let Self { mut file, path, created, modified } = self;

        let mut body =
            String::with_capacity(file.metadata()?.len().try_into().map_err(
                |_| crate::Error::FileTooLarge(context.real_path(&path)),
            )?);

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

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        Ok(self
            .into_named_file()?
            .into_response(context.variables.request))
    }
}

/// Response body content (possibly associated with a path).
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
    /// Plain text content from memory.
    pub fn plain_text<S: Into<String>>(body: S) -> Self {
        Self {
            body: Content::String(body.into()),
            content_type: MediaType::TEXT_PLAIN_UTF8,
            source: Source::Memory,
            metadata: Metadata::new(),
        }
    }

    /// HTML content from memory.
    pub fn html<S: Into<String>>(body: S) -> Self {
        Self::plain_text(body).with_content_type(MediaType::TEXT_HTML_UTF8)
    }

    /// Try to load the title from HTML if it’s not in the metadata.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::NotUtf8`] if the content is not UTF-8.
    pub fn ensure_metadata_title(&mut self) -> crate::Result<&Self> {
        if !self.metadata.contains_key("title") {
            let fragment = dom_query::Document::fragment(StrTendril::try_from(
                self.body.clone(),
            )?);
            let h1 = fragment.select_single("h1");
            if h1.length() > 0 {
                self.metadata.insert("title".into(), h1.text().into());
            }
        }
        Ok(self)
    }

    /// Set a metadata value.
    #[must_use]
    pub fn with_metadata<S1: Into<String>, S2: Into<String>>(
        mut self,
        key: S1,
        value: S2,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set content type.
    #[must_use]
    pub fn with_content_type<T: Into<MediaType>>(
        mut self,
        media_type: T,
    ) -> Self {
        self.content_type = media_type.into();
        self
    }
}

impl Return for ContentReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        _context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        Ok(self.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        if let Source::File { path, .. } = self.source {
            // FIXME should this be OsStringReturn?
            Ok(path
                .to_str()
                .ok_or_else(|| {
                    Error::InternalString(
                        "Could not convert non UTF-8 path to String".to_owned(),
                    )
                })?
                .to_owned()
                .into())
        } else {
            // FIXME make this error clearer; maybe track span?
            Err(Error::InternalString(
                "Could not get path from response".to_owned(),
            ))
        }
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        _context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        Ok(self)
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        Ok(HttpResponse::Ok()
            .content_type(&self.content_type)
            .body(self.into_content_return(context)?.body))
    }
}

impl From<&str> for ContentReturn {
    fn from(string: &str) -> Self {
        Self::plain_text(string)
    }
}

/// A return from an action
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
    /// Uses the path from [`PathReturn`] and [`ContentReturn`].
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
    /// Returns `crate::Error::NotUtf8` if the content isn’t valid UTF-8.
    pub fn into_string(self) -> crate::Result<String> {
        String::try_from(self)
    }

    /// Ensure that the content is a `String`.
    ///
    /// # Errors
    ///
    /// Returns `crate::Error::NotUtf8` if the content isn’t valid UTF-8.
    pub fn ensure_string(&mut self) -> crate::Result<&String> {
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
    type Error = crate::Error;

    fn try_from(content: Content) -> Result<Self, Self::Error> {
        String::try_from(content).map(Into::into)
    }
}

impl TryFrom<Content> for String {
    type Error = crate::Error;

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

/// Open a file and confirm that it is a file.
///
/// This reads one byte to check if the file is a directory (using `is_dir()`
/// would create a race condition.)
///
/// Returns the opened file (rewound).
///
/// # Errors
///
///   * [`io::Error`] resulting from opening the file, reading a byte, or
///     seeking to the start of the file.
pub fn open_confirmed_file<P: AsRef<Path>>(path: P) -> io::Result<fs::File> {
    let mut file = fs::File::open(path)?;
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;
    Ok(file)
}
