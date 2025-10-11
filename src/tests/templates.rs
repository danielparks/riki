//! Test template manager.
#![allow(clippy::incompatible_msrv, reason = "Expect current stable for tests")]

use assert2::{check, let_assert};
use jiff::Timestamp;
use crate::{Page, Source};
use std::fs;
use std::path::{Path, PathBuf};
use temp_dir::TempDir;

fn create_file<P, C>(dir: &TempDir, name: P, contents: C)
where
    P: AsRef<Path>,
    C: AsRef<[u8]>,
{
    fs::write(dir.path().join(name.as_ref()), contents.as_ref()).unwrap();
}

/// Makes error output more legible.
const AS_STR: for<'a> fn(&'a String) -> &'a str = String::as_str;

#[test]
fn empty_template() {
    let mut tpls = crate::templates();
    tpls.register_template_string("empty", "").unwrap();

    let_assert!(Ok(page) = Page::from_memory("title: page\n---\n# Page"));
    check!(let Ok("") = tpls.render("empty", &page).as_ref().map(AS_STR));
}

#[test]
fn override_template() {
    let mut tpls = crate::templates();
    // Override embedded default template
    tpls.register_template_string("default", "").unwrap();

    let_assert!(Ok(page) = Page::from_memory("title: page\n---\n# Page"));
    check!(let Ok("") = tpls.render("default", &page).as_ref().map(AS_STR));
}

#[test]
fn embedded_template() {
    let tpls = crate::templates();
    let mut embeded_names: Vec<_> = tpls.get_templates().keys().collect();
    embeded_names.sort();
    check!(
        embeded_names
            == vec![
                "default",
                "error403",
                "error404",
                "error500",
                "redirect301"
            ]
    );

    let_assert!(Ok(page) = Page::from_memory("title: page\n---\n# Page"));
    check!(let Ok("<!DOCTYPE html>
<html>
\t<head>
\t\t<title>page</title>
\t</head>
\t<body>
\t\t<h1>Page</h1>

\t</body>
</html>
") = crate::templates().render("default", &page).as_ref().map(AS_STR));
}

#[test]
fn basic_template() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{ metadata.title }} {{{ body }}}");

    let tpls = crate::templates_from_directory(temp.path()).unwrap();

    let_assert!(
        Ok(page) = Page::from_memory("title: title<test>\n---\n# heading")
    );
    check!(
        let Ok("title&lt;test&gt; <h1>heading</h1>\n")
            = tpls.render("default", &page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = Page::from_memory("title: t2\n---\n"));
    check!(let Ok("t2 ") = tpls.render("default", &page).as_ref().map(AS_STR));
}

#[test]
fn basic_template_twice() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{{ body }}}");

    let tpls = crate::templates_from_directory(temp.path()).unwrap();

    let_assert!(Ok(page) = Page::from_memory("# 1"));
    check!(
        let Ok("<h1>1</h1>\n")
            = tpls.render("default", &page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = Page::from_memory("# 2"));
    check!(
        let Ok("<h1>2</h1>\n")
            = tpls.render("default", &page).as_ref().map(AS_STR)
    );
}

#[test]
fn multiple_templates() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "default.hbs", "{{{ body }}}");
    create_file(&temp, "weird.hbs", "strange {{{ body }}}");

    let tpls = crate::templates_from_directory(temp.path()).unwrap();

    let_assert!(Ok(page) = Page::from_memory("# 1"));
    check!(
        let Ok("<h1>1</h1>\n")
            = tpls.render("default", &page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = Page::from_memory("# 2"));
    check!(
        let Ok("strange <h1>2</h1>\n")
            = tpls.render("weird", &page).as_ref().map(AS_STR)
    );
}

#[test]
fn strftime_helper() {
    let ref_time: Timestamp = "2001-09-08 18:46:40-0700".parse().unwrap();
    let source = Source::File {
        path: PathBuf::from("memory"),
        modified: Some(ref_time),
        created: Some(ref_time),
    };
    let_assert!(Ok(page) = Page::from_source(source, ""));

    let mut tpls = crate::templates();

    tpls.register_template_string(
        "la_tz",
        r#"{{ strftime source.File.modified "%Y-%m-%d %H:%M:%S %z"
            tz="America/Los_Angeles" }}"#,
    )
    .unwrap();
    check!(let Ok("2001-09-08 18:46:40 -0700")
        = tpls.render("la_tz", &page).as_ref().map(AS_STR));

    tpls.register_template_string(
        "chicago_tz",
        r#"{{ strftime source.File.modified "%Y-%m-%d %H:%M:%S %z"
            tz="America/Chicago" }}"#,
    )
    .unwrap();
    check!(let Ok("2001-09-08 20:46:40 -0500")
        = tpls.render("chicago_tz", &page).as_ref().map(AS_STR));

    tpls.register_template_string(
        "local",
        r#"{{ strftime source.File.modified "%Y" }}"#,
    )
    .unwrap();
    check!(let Ok("2001") = tpls.render("local", &page).as_ref().map(AS_STR));

    tpls.register_template_string(
        "broken",
        r#"{{ strftime source.File.modified "" tz="broken" }}"#,
    )
    .unwrap();
    let result = tpls.render("broken", &page);
    let_assert!(Err(error) = result.as_ref().map(AS_STR));
    check!(error.to_string().contains(" strftime helper: "));
}
