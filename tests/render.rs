#![allow(clippy::missing_docs_in_private_items, missing_docs)]

use riki::Error;
use riki::Page;

fn assert_page_no_metadata_key(page: &Page, key: &str) {
    assert!(
        !page.metadata.contains_key(key),
        "page.metadata[{:?}] incorrect\n  expected: <not present>\n  actual:   {:?}\n",
        key,
        page.metadata[key]
    );
}

fn assert_page_metadata_eq(page: &Page, key: &str, expected: &str) {
    if !page.metadata.contains_key(key) {
        panic!(
            "page.metadata[{key:?}] incorrect\n  expected: {expected:?}\n  actual:   <not present>\n"
        )
    } else if page.metadata[key] != expected {
        panic!(
            "page.metadata[{:?}] incorrect\n  expected: {:?}\n  actual:   {:?}\n",
            key, expected, page.metadata[key]
        )
    }
}

fn assert_page_body_eq(page: &Page, expected: &str) {
    assert!(
        page.body == expected,
        "page.body incorrect\n  expected: {:?}\n  actual:   {:?}\n",
        expected,
        page.body
    );
}

#[test]
fn empty_page() {
    let page = Page::from_string("").unwrap();
    assert_page_no_metadata_key(&page, "title");
    assert_page_body_eq(&page, "");
}

#[test]
fn empty_page_with_just_separator() {
    let page = Page::from_string("---").unwrap();
    assert_page_no_metadata_key(&page, "title");
    assert_page_body_eq(&page, "");
}

#[test]
fn empty_page_with_just_separator_and_whitespace() {
    let page = Page::from_string("\n---\n   ").unwrap();
    assert_page_no_metadata_key(&page, "title");
    assert_page_body_eq(&page, "");
}

#[test]
fn empty_page_with_bad_metadata() {
    match Page::from_string("bad_yaml\n---") {
        Err(Error::ParsePageMetadata(_)) => {}
        other => panic!("expected Error::ParsePageMetadata; got {other:?}"),
    }
}

#[test]
fn empty_page_with_blank_title() {
    let page = Page::from_string("title: \"\"\n---").unwrap();
    assert_page_metadata_eq(&page, "title", "");
    assert_page_body_eq(&page, "");
}

// FIXME presently it saves null as the string "~"
// #[test]
// fn empty_page_with_null_title() {
//     let page = Page::from_string("title:\n---").unwrap();
//     assert_page_metadata_eq(&page, "title", "");
//     assert_page_body_eq(&page, "");
// }

#[test]
fn empty_page_with_title() {
    let page = Page::from_string("title: TITLE \n\n---").unwrap();
    assert_page_metadata_eq(&page, "title", "TITLE");
    assert_page_body_eq(&page, "");
}

#[test]
fn trivial_page() {
    let page = Page::from_string("title: TITLE\n---\n# header\n").unwrap();
    assert_page_metadata_eq(&page, "title", "TITLE");
    assert_page_body_eq(&page, "<h1>header</h1>\n");
}

#[test]
fn no_title_one_h1() {
    let page = Page::from_string("# header\n").unwrap();
    assert_page_metadata_eq(&page, "title", "header");
    assert_page_body_eq(&page, "<h1>header</h1>\n");
}

#[test]
fn no_title_two_h1() {
    let page = Page::from_string("# one\n\n# two\n").unwrap();
    assert_page_metadata_eq(&page, "title", "one");
    assert_page_body_eq(&page, "<h1>one</h1>\n<h1>two</h1>\n");
}
