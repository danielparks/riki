//! Test custom elements.
#![allow(clippy::incompatible_msrv, reason = "Expect current stable for tests")]

use crate::http::{Context, WebError, WebResult};
use crate::{ContentReturn, MediaType, Source, http, render_source_to_string};
use assert2::check;
use jiff::Timestamp;
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

fn render(html: &str) -> WebResult<String> {
    static CONTEXT: LazyLock<Context> = LazyLock::new(|| {
        let mut tpls = crate::templates();
        tpls.register_template_string("default", "{{{body}}}")
            .unwrap();
        Context { tpls, ..Context::default() }
    });

    let ref_time: Timestamp = "2001-09-08 18:46:40-0700".parse().unwrap();
    let ret = ContentReturn {
        body: html.into(),
        source: Source::File {
            path: PathBuf::from("memory"),
            modified: Some(ref_time),
            created: Some(ref_time),
        },
        content_type: MediaType::TEXT_HTML_UTF8,
        ..ContentReturn::default()
    };

    match http::render(&CONTEXT, None, ret) {
        Ok(Some(ret)) => Ok(ret.body.into_string()?),
        Ok(None) => Err(WebError::NotFound),
        Err(error) => Err(error),
    }
}

#[test]
fn last_modified_tz() {
    check!(let Ok(_) = render("<last-modified></last-modified>"));

    check!(
        render(r#"<last-modified tz="America/Los_Angeles"></last-modified>"#)
            .unwrap()
            == "<html><head></head><body><span>2001-09-08T18:46:40-07:00[America/Los_Angeles]</span></body></html>"
    );

    check!(
        render(r#"<last-modified tz="America/Chicago"></last-modified>"#)
            .unwrap()
            == "<html><head></head><body><span>2001-09-08T20:46:40-05:00[America/Chicago]</span></body></html>"
    );

    check!(
        render(r#"<last-modified tz="broken"></last-modified>"#).unwrap()
            == "<html><head></head><body><b>last-modified element: failed to find time zone `broken` in time zone database</b></body></html>"
    );
}

#[test]
fn last_modified_format() {
    check!(
        render(r#"<last-modified format="%Y"></last-modified>"#).unwrap()
            == "<html><head></head><body><span>2001</span></body></html>"
    );
}

fn regex_assert(re: &str, input: &str) {
    check!(
        Regex::new(re).unwrap().is_match(input),
        "Regular expression:\n  {re:?}\ndoes not match:\n  {input:?}",
    );
}

#[test]
fn a_email() {
    regex_assert(
        "^<html><head></head><body><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\">abc<span class=\"hidden\">-[a-zA-Z0-9_-]+</span>@example.com</a></body></html>$",
        &render(r#"<a-email href="mailto:abc@example.com"></a-email>"#)
            .unwrap(),
    );

    regex_assert(
        "^<html><head></head><body><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\\?subject=foo\">abc<span class=\"hidden\">-[a-zA-Z0-9_-]+</span>@example.com</a></body></html>$",
        &render(
            r#"<a-email href="mailto:abc@example.com?subject=foo"></a-email>"#,
        )
        .unwrap(),
    );

    regex_assert(
        "^<html><head></head><body><a href=\"mailto:abc-[a-zA-Z0-9_-]+@example.com\">text</a></body></html>$",
        &render(r#"<a-email href="mailto:abc@example.com">text</a-email>"#)
            .unwrap(),
    );

    check!(
        render(r#"<a-email href="abc@example.com"></a-email>"#).unwrap()
            == "<html><head></head><body><b>Invalid URL in href attribute on &lt;a-email&gt;</b></body></html>"
    );

    check!(
        render("<a-email></a-email>").unwrap()
            == "<html><head></head><body><b>No href attribute on &lt;a-email&gt;</b></body></html>"
    );

    check!(
        render(r#"<a-email href=""></a-email>"#).unwrap()
            == "<html><head></head><body><b>Invalid URL in href attribute on &lt;a-email&gt;</b></body></html>"
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
