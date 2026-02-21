//! Test configuration actions.
#![cfg(test)]

use crate::actions::{self, Context, Return, StaticVariables};
use crate::config::actions::Action;
use crate::config::actions::functions::*;
use crate::config::model::ParsedString;
use crate::config::parser2::StringType;
use assert2::check;
use std::fs;
use std::path::Path;
use temp_dir::TempDir;

/// Create a [`ParsedString`] with variable interpolation.
fn parsed(s: &str) -> ParsedString<'_> {
    ParsedString::from_string_content(s, StringType::QuotedDouble).unwrap()
}

/// Create a context with the given `root` and `request_path`.
fn make_context<'a>(
    root: &'a std::path::Path,
    request_path: &'a str,
) -> Context<'a, StaticVariables<'a>> {
    Context {
        working_path: root.to_path_buf(),
        variables: StaticVariables {
            request_path,
            ..StaticVariables::default()
        },
        ..Context::default()
    }
}

/// Evaluate an action and return a string representing the path returned.
fn eval_action(action: &Action<'_>, root: &Path, request_path: &str) -> String {
    match action.evaluate(&make_context(root, request_path)) {
        Ok(ret) => format!("OK {}", ret.url_path().unwrap()),
        Err(actions::Error::RedirectCanonical(target)) => {
            format!("-> {target}")
        }
        Err(error) => format!("error {error:?}"),
    }
}

#[test_log::test]
fn canonical_if_file() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), "AAA").unwrap();

    let action = Action::from(canonical(if_file(parsed("$clean_path"))));
    check!(eval_action(&action, root, "/a.txt") == "OK /a.txt");
    check!(eval_action(&action, root, "/a.txt/") == "-> /a.txt");
}

#[test_log::test]
fn canonical_index_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/index.html"), "DDD").unwrap();

    let action = Action::from(canonical(condition(
        if_file(parsed("${clean_path}/index.html")),
        as_dir(parsed("$clean_path")),
    )));
    check!(eval_action(&action, root, "/d/") == "OK /d/");
    check!(eval_action(&action, root, "/d") == "-> /d/");
    check!(eval_action(&action, root, "/d/index.html") == "error NotFound");
}

#[test_log::test]
fn canonical_index_to_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/index.html"), "DDD").unwrap();

    let action =
        Action::from(canonical(as_dir(dirname(parsed("$clean_path")))));
    check!(eval_action(&action, root, "/d/index.html") == "-> /d/");
    check!(eval_action(&action, root, "/d/abc") == "-> /d/");
    check!(eval_action(&action, root, "/abc") == "-> /");

    // FIXME? / isn’t a file, so this won’t actually succeed.
    check!(eval_action(&action, root, "/") == "OK /");
}
