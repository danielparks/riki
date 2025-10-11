//! Test page rendering.

use assert2::{check, let_assert};
use crate::{Error, Page};

/// Get metadata value from page in format that’s easy to compare.
fn get_metadata<'a>(page: &'a Page, key: &'_ str) -> Option<&'a str> {
    page.metadata.get(key).map(String::as_str)
}

#[test]
fn empty_page() {
    let_assert!(Ok(page) = Page::from_memory(""));
    check!(get_metadata(&page, "title") == None);
    check!(&page.body == "");
}

#[test]
fn empty_page_with_just_separator() {
    let_assert!(Ok(page) = Page::from_memory("---"));
    check!(get_metadata(&page, "title") == None);
    check!(&page.body == "");
}

#[test]
fn empty_page_with_just_separator_and_whitespace() {
    let_assert!(Ok(page) = Page::from_memory("\n---\n   "));
    check!(get_metadata(&page, "title") == None);
    check!(&page.body == "");
}

#[test]
fn empty_page_with_bad_metadata() {
    check!(let Err(Error::ParsePageMetadata(_))
        = Page::from_memory("bad_yaml\n---"));
}

#[test]
fn empty_page_with_blank_title() {
    let_assert!(Ok(page) = Page::from_memory("title: \"\"\n---"));
    check!(get_metadata(&page, "title") == Some(""));
    check!(&page.body == "");
}

// FIXME presently it saves null as the string "~"
// #[test]
// fn empty_page_with_null_title() {
//     let_assert!(Ok(page) = Page::from_string("title:\n---"));
//     assert_page_metadata_eq(&page, "title", "");
//     assert_page_body_eq(&page, "");
// }

#[test]
fn empty_page_with_title() {
    let_assert!(Ok(page) = Page::from_memory("title: TITLE \n\n---"));
    check!(get_metadata(&page, "title") == Some("TITLE"));
    check!(&page.body == "");
}

#[test]
fn trivial_page() {
    let_assert!(Ok(page) = Page::from_memory("title: TITLE\n---\n# header\n"));
    check!(get_metadata(&page, "title") == Some("TITLE"));
    check!(&page.body == "<h1>header</h1>\n");
}

#[test]
fn no_title_one_h1() {
    let_assert!(Ok(page) = Page::from_memory("# header\n"));
    check!(get_metadata(&page, "title") == Some("header"));
    check!(&page.body == "<h1>header</h1>\n");
}

#[test]
fn no_title_two_h1() {
    let_assert!(Ok(page) = Page::from_memory("# one\n\n# two\n"));
    check!(get_metadata(&page, "title") == Some("one"));
    check!(&page.body == "<h1>one</h1>\n<h1>two</h1>\n");
}
