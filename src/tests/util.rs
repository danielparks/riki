//! Utility functions for tests

use crate::http::{Context, WebError, WebResult, markdown_to_html};
use crate::{ContentReturn, MediaType};

/// Parse markdown into `ContentReturn`.
///
/// # Errors
///
/// Might return `WebError`.
pub fn parse_md(raw: &str) -> WebResult<ContentReturn<String>> {
    let ret = ContentReturn {
        body: raw.to_owned(),
        content_type: MediaType::TEXT_MARKDOWN_UTF8,
        ..ContentReturn::default()
    };

    match markdown_to_html(&Context::default(), ret) {
        Ok(Some(ret)) => Ok(ret),
        Ok(None) => Err(WebError::NotFound),
        Err(error) => Err(error),
    }
}
