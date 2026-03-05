//! Media type for a return.

use http::HeaderValue;
use serde::Serialize;

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

impl From<&MediaType> for HeaderValue {
    fn from(media_type: &MediaType) -> Self {
        Self::from_static(media_type.0)
    }
}
