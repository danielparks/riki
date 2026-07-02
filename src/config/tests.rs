//! Test configuration parsing.
#![cfg(test)]

use super::actions::Action;
use super::model::{ConfigSettings, Configuration};
use super::parser2;
use assert2::{assert, check};

/// Parse configuration file and return errors in easy to compare format.
fn parse(source: &str) -> Result<Configuration<'_>, String> {
    parser2::parse(source).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{} {:?}",
                    diagnostic.message, diagnostic.labels[0].range
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Convenience function to easily compare parse results.
fn canonicalize(source: &str) -> Result<String, String> {
    parse(source).map(|config| {
        let mut settings = &ConfigSettings::default();
        let mut out = Vec::new();
        for rule in config.rules() {
            if &rule.settings != settings {
                settings = &rule.settings;
                out.extend(settings.canonical("/**"));
            }
            out.push(rule.canonical());
        }
        out.join("\n")
    })
}

/// Convenience function to return `Result<String, String>` for comparison.
#[expect(clippy::unnecessary_wraps, reason = "helper function")]
fn ok_str(value: &str) -> Result<String, String> {
    Ok(value.to_owned())
}

/// Convenience function to return `Result<String, String>` for comparison.
fn err_str(value: &str) -> Result<String, String> {
    Err(value.to_owned())
}

#[test_log::test]
fn empty() {
    check!(canonicalize("") == ok_str(""));
    check!(canonicalize("#") == ok_str(""));
    check!(canonicalize("# comment") == ok_str(""));
    check!(canonicalize("   ") == ok_str(""));
    check!(canonicalize("   # comment") == ok_str(""));
    check!(canonicalize("\n# comment") == ok_str(""));
    check!(canonicalize("# comment\n") == ok_str(""));
}

#[test_log::test]
fn bad_char() {
    // FIXME these errors seem wrong
    check!(
        canonicalize("=")
            == err_str(
                r#"invalid syntax, expected one of: <bare glob>, <end of file>, <identifier>, '\n', <path>, <"string">, <'string'>, '}' 0..1"#
            )
    );
    check!(
        canonicalize("{")
            == err_str(
                r#"invalid syntax, expected one of: <bare glob>, <end of file>, <identifier>, '\n', <path>, <"string">, <'string'>, '}' 0..1"#
            )
    );
    check!(
        canonicalize("}")
            == err_str("invalid syntax, expected: <end of file> 0..1")
    );
}

#[test_log::test]
fn one_token() {
    check!(canonicalize("foo") == ok_str(r#"/** "foo""#));
}

#[test_log::test]
fn one_rule() {
    check!(canonicalize("/ foo") == ok_str(r#"/** "foo""#));
    check!(canonicalize("/ foo\n") == ok_str(r#"/** "foo""#));
    check!(canonicalize("/ foo#comment") == ok_str(r#"/** "foo""#));
    check!(canonicalize("/ foo#comment\n") == ok_str(r#"/** "foo""#));
    check!(canonicalize("#comment\n/ foo") == ok_str(r#"/** "foo""#));
}

#[test_log::test]
fn one_rule_function() {
    check!(
        canonicalize(r#"/ literal("ok")"#) == ok_str(r#"/** literal("ok")"#)
    );
}

#[test_log::test]
fn bad_var() {
    check!(
        canonicalize("/ $bad")
            == err_str(r#"found unknown variable "bad" 2..6"#)
    );
    check!(
        canonicalize(r#"/ "/abc/$extra_bad1/def""#)
            == err_str(r#"found unknown variable "extra_bad1" 8..19"#)
    );
    check!(
        canonicalize(r#"/ "/abc/$/def""#)
            == err_str("found unescaped '$' without variable in string 8..9")
    );
    check!(
        canonicalize("/ '$bad1$clean_path$bad2'")
            == err_str(
                "found unknown variable \"bad1\" 3..8\n\
                found unknown variable \"bad2\" 19..24"
            )
    );
}

#[test_log::test]
fn good_var() {
    check!(canonicalize("/ $clean_path") == ok_str(r#"/** "${clean_path}""#));
}

#[test_log::test]
fn bad_function() {
    check!(
        canonicalize("/ bad()")
            == err_str(r#"found unknown function "bad" 2..5"#)
    );
    check!(
        canonicalize("/ error(bad())")
            == err_str(r#"found unknown function "bad" 8..11"#)
    );
    check!(
        canonicalize("/ error(1, 2)")
            == err_str(
                "found 2 parameters in call to error(); expected 1 2..13"
            )
    );
    check!(
        canonicalize("/ error()")
            == err_str(
                "found 0 parameters in call to error(); expected 1 2..9"
            )
    );
}

#[test_log::test]
fn good_function() {
    check!(canonicalize("/ error(403)") == ok_str(r#"/** error("403")"#));
}

#[test_log::test]
fn bad_setting() {
    check!(
        canonicalize("bad = nope")
            == err_str(r#"found unknown setting name "bad" 0..3"#)
    );
    check!(
        canonicalize("bad = error(403)")
            == err_str(r#"found unknown setting name "bad" 0..3"#)
    );
    check!(
        canonicalize("root = error(403)")
            == err_str(
                "setting \"root\" does not accept a function result as a value \
                0..17"
            )
    );
}

#[test_log::test]
fn good_setting() {
    check!(
        canonicalize(
            "root = /tmp
            templates = /templates
            / $clean_path"
        ) == ok_str(
            "/** root = \"/tmp\"\n\
            /** templates = \"/templates\"\n\
            /** \"${clean_path}\""
        )
    );
    check!(
        canonicalize(
            "root = /foo
            root = $clean_path
            templates = templates
            / $clean_path"
        ) == ok_str(
            "/** root = \"/foo/${clean_path}\"\n\
            /** templates = \"/foo/${clean_path}/templates\"\n\
            /** \"${clean_path}\""
        )
    );
    check!(
        canonicalize(
            "root = /foo
            root = ./$clean_path
            templates = templates
            / $clean_path"
        ) == ok_str(
            "/** root = \"/foo/./${clean_path}\"\n\
            /** templates = \"/foo/./${clean_path}/templates\"\n\
            /** \"${clean_path}\""
        )
    );
    check!(
        canonicalize(
            "root = /$clean_path/
            templates = templates
            / $clean_path"
        ) == ok_str(
            "/** root = \"/${clean_path}/\"\n\
            /** templates = \"/${clean_path}/templates\"\n\
            /** \"${clean_path}\""
        )
    );
}

#[test_log::test]
fn config_matches_simple() {
    let config = parse(
        "root = /srv\n\
        templates = templates\n\
        / $clean_path",
    )
    .unwrap();
    assert!(let [rule] = &config.matches("/foo/bar")[..]);
    check!(rule.settings.root == "/srv".into());
    check!(rule.settings.templates == "/srv/templates".into());
    check!(rule.matcher.canonical() == "/**");
    check!(let Action::Literal(_) = rule.action);
}

#[test_log::test]
fn relative_settings() {
    let config = parse(
        "root = /srv
        templates = tmpl
        root = web
        / $clean_path",
    )
    .unwrap();
    assert!(let [rule] = &config.matches("/")[..]);
    check!(rule.settings.root == "/srv/web".into());
    check!(rule.settings.templates == "/srv/tmpl".into());
}

#[test_log::test]
fn absolute_settings() {
    let config = parse(
        "root = /srv
        templates = /tpl
        root = /www
        / $clean_path",
    )
    .unwrap();
    assert!(let [rule] = &config.matches("/")[..]);
    check!(rule.settings.root == "/www".into());
    check!(rule.settings.templates == "/tpl".into());
}
