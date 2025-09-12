//! Test custom elements.

use assert2::{check, let_assert};
use jiff::Timestamp;
use regex::Regex;
use riki::{Page, Result, Source, render_source_to_string};
use std::path::PathBuf;

/// Makes error output more legible.
const AS_STR: for<'a> fn(&'a String) -> &'a str = String::as_str;

fn ref_page(content: &str) -> Result<Page> {
    let ref_time: Timestamp = "2001-09-08 18:46:40-0700".parse().unwrap();
    let source = Source::File {
        path: PathBuf::from("memory"),
        modified: Some(ref_time),
        created: Some(ref_time),
    };
    Page::from_source(source, content)
}

#[test]
fn last_modified_tz() {
    let mut tpls = riki::templates();
    tpls.register_template_string("default", "{{{body}}}")
        .unwrap();

    let_assert!(Ok(page) = ref_page("<last-modified></last-modified>"));
    check!(let Ok(_) = page.render_to_string(&tpls, None).as_ref().map(AS_STR));

    let_assert!(
        Ok(page) = ref_page(
            r#"<last-modified tz="America/Los_Angeles"></last-modified>"#
        )
    );
    check!(
        let Ok("<html><head></head><body><p><span>2001-09-08T18:46:40-07:00[America/Los_Angeles]</span></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );

    let_assert!(
        Ok(page) =
            ref_page(r#"<last-modified tz="America/Chicago"></last-modified>"#)
    );
    check!(
        let Ok("<html><head></head><body><p><span>2001-09-08T20:46:40-05:00[America/Chicago]</span></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );

    let_assert!(
        Ok(page) = ref_page(r#"<last-modified tz="broken"></last-modified>"#)
    );
    check!(
        let Ok("<html><head></head><body><p><b>last-modified element: failed to find time zone `broken` in time zone database</b></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );
}

#[test]
fn last_modified_format() {
    let mut tpls = riki::templates();
    tpls.register_template_string("default", "{{{body}}}")
        .unwrap();

    let_assert!(
        Ok(page) = ref_page(r#"<last-modified format="%Y"></last-modified>"#)
    );
    check!(
        let Ok("<html><head></head><body><p><span>2001</span></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );
}

fn regex_assert(re: &str, input: &str) {
    check!(
        Regex::new(re).unwrap().is_match(input),
        "Regular expresion:\n  {re:?}\ndoes not match:\n  {input:?}",
    );
}

#[test]
fn a_email() {
    let mut tpls = riki::templates();
    tpls.register_template_string("default", "{{{body}}}")
        .unwrap();

    let_assert!(
        Ok(page) =
            ref_page(r#"<a-email href="mailto:abc@example.com"></a-email>"#)
    );
    let_assert!(Ok(html) = page.render_to_string(&tpls, None));
    regex_assert(
        "^<html><head></head><body><p><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\">abc<span class=\"hidden\">-[a-zA-Z0-9_-]+</span>@example.com</a></p>\n</body></html>$",
        &html,
    );

    let_assert!(
        Ok(page) = ref_page(
            r#"<a-email href="mailto:abc@example.com?subject=foo"></a-email>"#
        )
    );
    let_assert!(Ok(html) = page.render_to_string(&tpls, None));
    regex_assert(
        "^<html><head></head><body><p><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\\?subject=foo\">abc<span class=\"hidden\">-[a-zA-Z0-9_-]+</span>@example.com</a></p>\n</body></html>$",
        &html,
    );

    let_assert!(
        Ok(page) = ref_page(
            r#"<a-email href="mailto:abc@example.com">text</a-email>"#
        )
    );
    let_assert!(Ok(html) = page.render_to_string(&tpls, None));
    regex_assert(
        "^<html><head></head><body><p><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\">text</a></p>\n</body></html>$",
        &html,
    );

    let_assert!(
        Ok(page) = ref_page(r#"<a-email href="abc@example.com"></a-email>"#)
    );
    check!(
        let Ok("<html><head></head><body><p><b>Invalid URL in href attribute on &lt;a-email&gt;</b></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = ref_page("<a-email></a-email>"));
    check!(
        let Ok("<html><head></head><body><p><b>No href attribute on &lt;a-email&gt;</b></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = ref_page(r#"<a-email href=""></a-email>"#));
    check!(
        let Ok("<html><head></head><body><p><b>Invalid URL in href attribute on &lt;a-email&gt;</b></p>\n</body></html>")
            = page.render_to_string(&tpls, None).as_ref().map(AS_STR)
    );
}

#[test]
fn a_email_source() {
    check!(
        "title: test\n\n---\n\n<b>test</b>\n".to_owned()
            == render_source_to_string("title: test\n\n---\n\n<b>test</b>\n")
    );

    check!(
        r#"<a-email href="mailto:***@*****"></a-email>"#.to_owned()
            == render_source_to_string(
                r#"<a-email href="mailto:abc@example.com"></a-email>"#
            )
    );

    check!(
        r#"<a-email href="mailto:***@*****?subject=test"></a-email>"#
            .to_owned()
            == render_source_to_string(
                r#"<a-email href="mailto:abc@example.com?subject=test"></a-email>"#
            )
    );

    check!(
        r#"<a-email href="mailto:***@*****">text</a-email>"#.to_owned()
            == render_source_to_string(
                r#"<a-email href="mailto:abc@example.com">text</a-email>"#
            )
    );

    check!(
        r#"<a-email href="*****"></a-email>"#.to_owned()
            == render_source_to_string(
                r#"<a-email href="abc@example.com"></a-email>"#
            )
    );

    check!(
        r#"<a-email href="*****"></a-email>"#.to_owned()
            == render_source_to_string(r#"<a-email href=""></a-email>"#)
    );

    check!(
        r"<a-email></a-email>".to_owned()
            == render_source_to_string(r"<a-email></a-email>")
    );
}
