//! Utility functions for tests

use crate::actions::{
    ContentReturn, Error, MediaType, Result, StaticContext, markdown_to_html,
};

/// Parse markdown into `ContentReturn`.
///
/// # Errors
///
/// Might return `Error`.
pub fn parse_md(raw: &str) -> Result<ContentReturn> {
    let ret = ContentReturn {
        body: raw.into(),
        content_type: MediaType::TEXT_MARKDOWN_UTF8,
        ..ContentReturn::default()
    };

    match markdown_to_html(&StaticContext::default(), ret) {
        Ok(Some(ret)) => Ok(ret),
        Ok(None) => Err(Error::NotFound),
        Err(error) => Err(error),
    }
}
