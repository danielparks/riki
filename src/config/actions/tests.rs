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
) -> Context<StaticVariables<'a>> {
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
fn canonical_clean_path() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let action = Action::from(canonical(parsed("$clean_path")));
    check!(eval_action(&action, root, "/a.txt") == "OK /a.txt");
    check!(eval_action(&action, root, "/a.txt/") == "-> /a.txt");
    check!(eval_action(&action, root, "/a") == "OK /a");
    check!(eval_action(&action, root, "/") == "OK /");
}

#[test_log::test]
fn canonical_clean_path_if_file() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), "AAA").unwrap();

    let action = Action::from(canonical(if_file(parsed("$clean_path"))));
    check!(eval_action(&action, root, "/a.txt") == "OK /a.txt");
    check!(eval_action(&action, root, "/a.txt/") == "-> /a.txt");
    check!(eval_action(&action, root, "/a") == "error Skip");
    check!(eval_action(&action, root, "/") == "error Skip");
}

#[test_log::test]
fn canonical_clean_path_index_dir() {
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
    check!(eval_action(&action, root, "/d/index.html") == "error Skip");
    check!(eval_action(&action, root, "/a") == "error Skip");
    check!(eval_action(&action, root, "/") == "error Skip");
}

#[test_log::test]
fn canonical_clean_path_index_to_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let action =
        Action::from(canonical(as_dir(dirname(parsed("$clean_path")))));
    check!(eval_action(&action, root, "/d/index.html") == "-> /d/");
    check!(eval_action(&action, root, "/d/abc") == "-> /d/");
    check!(eval_action(&action, root, "/abc") == "-> /");

    // FIXME? / isn’t a file, so this won’t actually succeed.
    check!(eval_action(&action, root, "/") == "OK /");
}

#[test_log::test]
fn canonical_absolute_clean_path() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // FIXME better variable interpolation in ParsedString
    let action = Action::from(canonical(parsed("/$clean_path")));
    check!(eval_action(&action, root, "/a.txt") == "-> //a.txt");
    check!(eval_action(&action, root, "/a.txt/") == "-> //a.txt");
    check!(eval_action(&action, root, "/a") == "-> //a");
    check!(eval_action(&action, root, "/") == "-> //");
}

#[test_log::test]
fn canonical_absolute_clean_path_if_file() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("context_root");

    // Put files in separate root.
    let prefix_path = dir.path().join("real_root");
    fs::create_dir(&prefix_path).unwrap();
    fs::write(prefix_path.join("a.txt"), "AAA").unwrap();
    let prefix = prefix_path
        .into_os_string()
        .into_string()
        .expect("TempDir produced a non-UTF-8 path");

    // Absolute paths are pretty much useless with `canonical()`.
    let config_str = format!("{prefix}$clean_path");
    let action = Action::from(canonical(if_file(parsed(&config_str))));
    check!(
        eval_action(&action, &root, "/a.txt") == format!("-> {prefix}/a.txt")
    );
    check!(
        eval_action(&action, &root, "/a.txt/") == format!("-> {prefix}/a.txt")
    );
    check!(eval_action(&action, &root, "/a") == "error Skip");
    check!(eval_action(&action, &root, "/") == "error Skip");
}

#[test_log::test]
fn error_400() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    check!(
        eval_action(&Action::from(error("400")), root, "/")
            == "error BadRequest(\"unknown\")"
    );
    check!(
        eval_action(&Action::from(error("400 a b c")), root, "/")
            == "error BadRequest(\"a b c\")"
    );
}

#[test_log::test]
fn error_403() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    check!(
        eval_action(&Action::from(error("403")), root, "/")
            == "error Forbidden"
    );
    check!(
        eval_action(&Action::from(error("403 invalid")), root, "/")
            == "error Internal(could not evaluate error(\"403 invalid\"))"
    );
}

#[test_log::test]
fn error_404() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    check!(
        eval_action(&Action::from(error("404")), root, "/") == "error NotFound"
    );
    check!(
        eval_action(&Action::from(error("404 invalid")), root, "/")
            == "error Internal(could not evaluate error(\"404 invalid\"))"
    );
}

#[test_log::test]
fn error_500() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    check!(
        eval_action(&Action::from(error("500")), root, "/")
            == "error Internal(unknown)"
    );
    check!(
        eval_action(&Action::from(error("500 a b c")), root, "/")
            == "error Internal(a b c)"
    );
}

#[test_log::test]
fn error_other() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Error codes that are already tested.
    let exclude = [400, 403, 404, 500];

    for i in 0..1 {
        if exclude.contains(&i) {
            continue;
        }

        check!(
            eval_action(&Action::from(error(format!("{i:03}"))), root, "/")
                == format!(
                    "error Internal(could not evaluate error(\"{i:03}\"))"
                )
        );
        check!(
            eval_action(
                &Action::from(error(format!("{i:03} a b c"))),
                root,
                "/"
            ) == format!(
                "error Internal(could not evaluate error(\"{i:03} a b c\"))"
            )
        );
    }
}
