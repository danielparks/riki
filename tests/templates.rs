//! Test template manager.

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

#[test]
fn empty_template() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "");

    let mut tpls = TemplateManager::new(temp.as_ref()).unwrap();
    let page = Page::from_string("title: page\n---\n# Page").unwrap();

    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "");
}

#[test]
fn basic_template() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{ metadata.title }} {{& body }}");

    let mut tpls = TemplateManager::new(temp.as_ref()).unwrap();

    let page = Page::from_string("title: title<test>\n---\n# heading").unwrap();
    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "title&lt;test&gt; <h1>heading</h1>\n");

    let page = Page::from_string("title: t2\n---\n").unwrap();
    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "t2 ");
}

#[test]
fn basic_template_twice() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{& body }}");

    let mut tpls = TemplateManager::new(temp.as_ref()).unwrap();

    let page = Page::from_string("# 1").unwrap();
    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "<h1>1</h1>\n");

    let page = Page::from_string("# 2").unwrap();
    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "<h1>2</h1>\n");
}

#[test]
fn multiple_templates() {
    let temp = TempDir::default();
    create_file(&temp, "default.tmpl", "{{& body }}");
    create_file(&temp, "weird.tmpl", "strange {{& body }}");

    let mut tpls = TemplateManager::new(temp.as_ref()).unwrap();

    let page = Page::from_string("# 1").unwrap();
    let actual = tpls.default().unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "<h1>1</h1>\n");

    let page = Page::from_string("# 2").unwrap();
    let actual = tpls.get(&"weird").unwrap().render_to_string(&page).unwrap();
    assert_eq!(actual, "strange <h1>2</h1>\n");
}
