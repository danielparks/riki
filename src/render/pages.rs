//! Handle rendering a Markdown “page”.

use super::elements::handle_a_email_source;
use crate::actions::Metadata;
use dom_query::Document;
use pulldown_cmark::{Options, Parser};
use regex::Regex;
use serde_yaml::Error as YamlError;
use std::collections::HashMap;
use std::result;
use std::sync::LazyLock;

/// Render a page source to a string.
///
/// This is used to redact private data, e.g. `a-email` elements, from a page
/// source file.
#[must_use]
#[expect(clippy::needless_pass_by_value, reason = "ToString accepts borrows")]
pub fn render_source_to_string<S: ToString>(source: S) -> String {
    let document = Document::from(source.to_string());

    for node in document.select("a-email").nodes() {
        handle_a_email_source(node);
    }

    if let Some(body) = document.body() {
        body.inner_html().to_string()
    } else {
        tracing::warn!("Should be unreachable: no document body element.");
        String::new()
    }
}

/// Split the raw page string into metadata and body.
///
/// Pages are just Markdown files with YAML headers separated by a `---` line.
///
/// ```yaml
/// title: Example page
/// other_metadata: foobar
///
/// ---
///
/// # Markdown page here
/// ```
pub fn split_raw_page(raw: &str) -> (&str, &str) {
    static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:^|\s*[\r\n])---(?:$|[\r\n]\s*)").unwrap()
    });
    let mut iter = SPLIT_RE.splitn(raw, 2);
    match (iter.next(), iter.next()) {
        (Some(yaml), Some(body)) => (yaml, body),
        (Some(body), None) => ("", body),
        (None, _) => unreachable!(),
    }
}

/// Load metadata from string.
///
/// This expects the header of a page without the trailing `---`. Metadata is
/// limited to string values (arrays and other structured data is not allowed).
///
/// # Errors
///
/// Returns [`YamlError`] if the YAML is invalid.
pub fn metadata_from_string(raw: &str) -> result::Result<Metadata, YamlError> {
    if raw.trim().is_empty() {
        Ok(HashMap::new())
    } else {
        serde_yaml::from_str(raw)
    }
}

/// Render Markdown as HTML.
#[must_use]
pub fn render_markdown(markdown: &str) -> String {
    let mut buffer = String::new();
    let parser = Parser::new_ext(markdown, Options::all());
    pulldown_cmark::html::push_html(&mut buffer, parser);

    buffer
}
