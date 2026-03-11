//! Test Markdown parsing and rendering.
#![cfg(test)]

use crate::actions::{
    ContentReturn, Error, MediaType, Result, StaticContext, markdown_to_html,
};
use assert2::{check, let_assert};

/// Parse markdown into `ContentReturn`.
///
/// # Errors
///
/// Might return `Error`.
pub fn parse_md(raw: &str) -> Result<ContentReturn> {
    markdown_to_html(
        &StaticContext::default(),
        ContentReturn::from(raw)
            .with_content_type(MediaType::TEXT_MARKDOWN_UTF8),
    )
}

/// Get metadata value from ret in format that’s easy to compare.
fn get_metadata<'a>(ret: &'a ContentReturn, key: &'_ str) -> Option<&'a str> {
    ret.metadata.get(key).map(String::as_str)
}

#[test_log::test]
fn empty_page() {
    let_assert!(Ok(ret) = parse_md(""));
    check!(get_metadata(&ret, "title") == None);
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn non_utf8_page() {
    let_assert!(
        Err(Error::Internal(error)) = markdown_to_html(
            &StaticContext::default(),
            ContentReturn {
                body: b"title: foo\xff\n----".into(),
                ..ContentReturn::default()
            }
        )
    );
    check!(error.to_string() == "Found non-UTF-8 data");
}

#[test_log::test]
fn empty_page_with_just_separator() {
    let_assert!(Ok(ret) = parse_md("---"));
    check!(get_metadata(&ret, "title") == None);
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn empty_page_with_just_separator_and_whitespace() {
    let_assert!(Ok(ret) = parse_md("\n---\n   "));
    check!(get_metadata(&ret, "title") == None);
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn empty_page_with_bad_metadata() {
    let_assert!(Err(Error::Internal(error)) = parse_md("bad_yaml\n---"));
    check!(let Ok(crate::Error::ParsePageMetadata(_)) = error.downcast());
}

#[test_log::test]
fn empty_page_with_empty_string_title() {
    let_assert!(Ok(ret) = parse_md("title: \"\"\n---"));
    check!(get_metadata(&ret, "title") == Some(""));
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn empty_page_with_blank_title() {
    let_assert!(Ok(ret) = parse_md("title:\n---"));
    check!(get_metadata(&ret, "title") == Some(""));
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn empty_page_with_tilde_title() {
    let_assert!(Ok(ret) = parse_md("title: ~\n---"));
    check!(get_metadata(&ret, "title") == Some("~"));
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn empty_page_with_title() {
    let_assert!(Ok(ret) = parse_md("title: TITLE \n\n---"));
    check!(get_metadata(&ret, "title") == Some("TITLE"));
    check!(ret.body.as_str() == "");
}

#[test_log::test]
fn trivial_page() {
    let_assert!(Ok(ret) = parse_md("title: TITLE\n---\n# header\n"));
    check!(get_metadata(&ret, "title") == Some("TITLE"));
    check!(ret.body.as_str() == "<h1>header</h1>\n");
}

#[test_log::test]
fn no_title_one_h1() {
    let_assert!(Ok(ret) = parse_md("# header\n"));
    check!(get_metadata(&ret, "title") == Some("header"));
    check!(ret.body.as_str() == "<h1>header</h1>\n");
}

#[test_log::test]
fn no_title_two_h1() {
    let_assert!(Ok(ret) = parse_md("# one\n\n# two\n"));
    check!(get_metadata(&ret, "title") == Some("one"));
    check!(ret.body.as_str() == "<h1>one</h1>\n<h1>two</h1>\n");
}
