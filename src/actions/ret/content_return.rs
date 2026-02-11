//! A return with actual content.

use super::{
    ActionReturn, Context, Error, MediaType, RequestContext, Result, Return,
    StringReturn, VariableMap,
};
use actix_web::HttpResponse;
use actix_web::body::BodySize;
use actix_web::web::Bytes;
use jiff::Timestamp;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::mem;
use std::path::PathBuf;
use std::pin::Pin;
use std::task;
use tendril::StrTendril;

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
            StringReturn::try_from(path)
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
