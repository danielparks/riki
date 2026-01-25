//! Test Markdown parsing and rendering.

use super::util::parse_md;
use crate::http::WebError;
use crate::{Content, ContentReturn, Error};
use assert2::{check, let_assert};

/// Get metadata value from ret in format that’s easy to compare.
fn get_metadata<'a, C: Content>(
    ret: &'a ContentReturn<C>,
    key: &'_ str,
) -> Option<&'a str> {
    ret.metadata.get(key).map(String::as_str)
}

#[test]
fn empty_page() {
    let_assert!(Ok(ret) = parse_md(""));
    check!(get_metadata(&ret, "title") == None);
    check!(&ret.body == "");
}

#[test]
fn empty_page_with_just_separator() {
    let_assert!(Ok(ret) = parse_md("---"));
    check!(get_metadata(&ret, "title") == None);
    check!(&ret.body == "");
}

#[test]
fn empty_page_with_just_separator_and_whitespace() {
    let_assert!(Ok(ret) = parse_md("\n---\n   "));
    check!(get_metadata(&ret, "title") == None);
    check!(&ret.body == "");
}

#[test]
fn empty_page_with_bad_metadata() {
    check!(let Err(WebError::Internal(Error::ParsePageMetadata(_)))
        = parse_md("bad_yaml\n---"));
}

#[test]
fn empty_page_with_blank_title() {
    let_assert!(Ok(ret) = parse_md("title: \"\"\n---"));
    check!(get_metadata(&ret, "title") == Some(""));
    check!(&ret.body == "");
}

// FIXME presently it saves null as the string "~"
// #[test]
// fn empty_page_with_null_title() {
//     let_assert!(Ok(ret) = Page::from_string("title:\n---"));
//     assert_page_metadata_eq(&ret, "title", "");
//     assert_page_body_eq(&ret, "");
// }

#[test]
fn empty_page_with_title() {
    let_assert!(Ok(ret) = parse_md("title: TITLE \n\n---"));
    check!(get_metadata(&ret, "title") == Some("TITLE"));
    check!(&ret.body == "");
}

#[test]
fn trivial_page() {
    let_assert!(Ok(ret) = parse_md("title: TITLE\n---\n# header\n"));
    check!(get_metadata(&ret, "title") == Some("TITLE"));
    check!(&ret.body == "<h1>header</h1>\n");
}

#[test]
fn no_title_one_h1() {
    let_assert!(Ok(ret) = parse_md("# header\n"));
    check!(get_metadata(&ret, "title") == Some("header"));
    check!(&ret.body == "<h1>header</h1>\n");
}

#[test]
fn no_title_two_h1() {
    let_assert!(Ok(ret) = parse_md("# one\n\n# two\n"));
    check!(get_metadata(&ret, "title") == Some("one"));
    check!(&ret.body == "<h1>one</h1>\n<h1>two</h1>\n");
}
