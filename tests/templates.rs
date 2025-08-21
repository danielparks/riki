//! Test template manager.

use assert2::{check, let_assert};
use riki::{Page, TemplateManager};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use temp_testdir::TempDir;

fn create_file<P, S, C>(dir: P, name: S, contents: C)
where
    P: AsRef<Path>,
    S: AsRef<str>,
    C: AsRef<[u8]>,
{
    let path = dir.as_ref().to_path_buf().join(name.as_ref());
    let mut f = File::create(path).unwrap();
    f.write_all(contents.as_ref()).unwrap();
}

/// Makes error output more legible.
const AS_STR: for<'a> fn(&'a String) -> &'a str = String::as_str;

#[test]
fn empty_template() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "");

    let tpls = TemplateManager::new(temp.as_ref()).unwrap();
    let_assert!(Ok(tpl) = tpls.get("default"));

    let_assert!(Ok(page) = Page::from_string("title: page\n---\n# Page"));
    check!(let Ok("") = tpl.render_to_string(&page).as_ref().map(AS_STR));
}

#[test]
fn basic_template() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{ metadata.title }} {{& body }}");

    let tpls = TemplateManager::new(temp.as_ref()).unwrap();
    let_assert!(Ok(tpl) = tpls.get("default"));

    let_assert!(
        Ok(page) = Page::from_string("title: title<test>\n---\n# heading")
    );
    check!(
        let Ok("title&lt;test&gt; <h1>heading</h1>\n")
            = tpl.render_to_string(&page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(page) = Page::from_string("title: t2\n---\n"));
    check!(let Ok("t2 ") = tpl.render_to_string(&page).as_ref().map(AS_STR));
}

#[test]
fn basic_template_twice() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{& body }}");

    let tpls = TemplateManager::new(temp.as_ref()).unwrap();

    let_assert!(Ok(tpl) = tpls.get("default"));
    let_assert!(Ok(page) = Page::from_string("# 1"));
    check!(
        let Ok("<h1>1</h1>\n")
            = tpl.render_to_string(&page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(tpl) = tpls.get("default"));
    let_assert!(Ok(page) = Page::from_string("# 2"));
    check!(
        let Ok("<h1>2</h1>\n")
            = tpl.render_to_string(&page).as_ref().map(AS_STR)
    );
}

#[test]
fn multiple_templates() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{& body }}");
    create_file(&temp, "weird.tmpl", "strange {{& body }}");

    let tpls = TemplateManager::new(temp.as_ref()).unwrap();

    let_assert!(Ok(tpl) = tpls.get("default"));
    let_assert!(Ok(page) = Page::from_string("# 1"));
    check!(
        let Ok("<h1>1</h1>\n")
            = tpl.render_to_string(&page).as_ref().map(AS_STR)
    );

    let_assert!(Ok(tpl) = tpls.get("weird"));
    let_assert!(Ok(page) = Page::from_string("# 2"));
    check!(
        let Ok("strange <h1>2</h1>\n")
            = tpl.render_to_string(&page).as_ref().map(AS_STR)
    );
}
