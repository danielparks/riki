//! Test template manager.
#![allow(clippy::incompatible_msrv, reason = "Expect current stable for tests")]

use crate::actions::{ContentReturn, MediaType, Source};
use crate::render::{base_templates, templates_from_directory};
use assert2::{check, let_assert};
use jiff::Timestamp;
use std::fs;
use std::path::{Path, PathBuf};
use temp_dir::TempDir;

/// Create a file in the temporary directory.
fn create_file<P, C>(dir: &TempDir, name: P, contents: C)
where
    P: AsRef<Path>,
    C: AsRef<[u8]>,
{
    fs::write(dir.path().join(name.as_ref()), contents.as_ref()).unwrap();
}

/// Helper to produce an HTML `ContentReturn`.
fn html_title(body: &str, title: &str) -> ContentReturn {
    ContentReturn::html(body).with_metadata("title", title)
}

/// Makes error output more legible.
const AS_STR: for<'a> fn(&'a String) -> &'a str = String::as_str;

#[test_log::test]
fn empty_template() {
    let mut tpls = base_templates();
    tpls.register_template_string("empty", "").unwrap();

    check!(
        let Ok("")
            = tpls.render("empty", &html_title("<h1>Page</h1>", "page"))
            .as_ref().map(AS_STR)
    );
}

#[test_log::test]
fn override_template() {
    let mut tpls = base_templates();
    // Override embedded default template
    tpls.register_template_string("default", "").unwrap();

    check!(let Ok("")
        = tpls.render("default", &html_title("<h1>Page</h1>", "page"))
        .as_ref().map(AS_STR));
}

#[test_log::test]
fn embedded_template() {
    let tpls = base_templates();
    let mut embedded_names: Vec<_> = tpls.get_templates().keys().collect();
    embedded_names.sort();
    check!(
        embedded_names
            == vec![
                "default",
                "error403",
                "error404",
                "error500",
                "redirect301"
            ]
    );

    check!(let Ok("<!DOCTYPE html>
<html>
\t<head>
\t\t<title>page</title>
\t</head>
\t<body>
\t\t<h1>Page</h1>
\t</body>
</html>
") = tpls.render("default", &html_title("<h1>Page</h1>", "page"))
    .as_ref().map(AS_STR));
}

#[test_log::test]
fn basic_template() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{ metadata.title }} {{{ body }}}");

    let tpls = templates_from_directory(temp.path()).unwrap();

    check!(
        let Ok("title&lt;test&gt; <h1>h1</h1>")
            = tpls.render("default", &html_title("<h1>h1</h1>", "title<test>"))
            .as_ref().map(AS_STR)
    );

    check!(
        let Ok("t2 ")
            = tpls.render("default", &html_title("", "t2"))
            .as_ref().map(AS_STR)
    );
}

#[test_log::test]
fn basic_template_twice() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{{ body }}}");

    let tpls = templates_from_directory(temp.path()).unwrap();

    check!(
        let Ok("<h1>1</h1>")
            = tpls.render("default", &html_title("<h1>1</h1>", ""))
            .as_ref().map(AS_STR)
    );

    check!(
        let Ok("<h1>2</h1>")
            = tpls.render("default", &html_title("<h1>2</h1>", ""))
            .as_ref().map(AS_STR)
    );
}

#[test_log::test]
fn multiple_templates() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{{ body }}}");
    create_file(&temp, "weird.hbs", "strange {{{ body }}}");

    let tpls = templates_from_directory(temp.path()).unwrap();

    check!(
        let Ok("<h1>1</h1>")
            = tpls.render("default", &html_title("<h1>1</h1>", ""))
            .as_ref().map(AS_STR)
    );

    check!(
        let Ok("strange <h1>2</h1>")
            = tpls.render("weird", &html_title("<h1>2</h1>", ""))
            .as_ref().map(AS_STR)
    );
}

#[test_log::test]
fn strftime_helper() {
    let ref_time: Timestamp = "2001-09-08 18:46:40-0700".parse().unwrap();
    let ret = ContentReturn {
        body: "".into(),
        source: Source::File {
            path: PathBuf::from("memory"),
            modified: Some(ref_time),
            created: Some(ref_time),
        },
        content_type: MediaType::TEXT_HTML_UTF8,
        ..ContentReturn::default()
    };

    let mut tpls = base_templates();

    tpls.register_template_string(
        "la_tz",
        r#"{{ strftime source.File.modified "%Y-%m-%d %H:%M:%S %z"
            tz="America/Los_Angeles" }}"#,
    )
    .unwrap();
    check!(let Ok("2001-09-08 18:46:40 -0700")
        = tpls.render("la_tz", &ret).as_ref().map(AS_STR));

    tpls.register_template_string(
        "chicago_tz",
        r#"{{ strftime source.File.modified "%Y-%m-%d %H:%M:%S %z"
            tz="America/Chicago" }}"#,
    )
    .unwrap();
    check!(let Ok("2001-09-08 20:46:40 -0500")
        = tpls.render("chicago_tz", &ret).as_ref().map(AS_STR));

    tpls.register_template_string(
        "local",
        r#"{{ strftime source.File.modified "%Y" }}"#,
    )
    .unwrap();
    check!(let Ok("2001") = tpls.render("local", &ret).as_ref().map(AS_STR));

    tpls.register_template_string(
        "broken",
        r#"{{ strftime source.File.modified "" tz="broken" }}"#,
    )
    .unwrap();
    let result = tpls.render("broken", &ret);
    let_assert!(Err(error) = result.as_ref().map(AS_STR));
    check!(error.to_string().contains(" strftime helper: "));
}
