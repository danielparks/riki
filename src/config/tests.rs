//! Test configuration parsing.
#![cfg(test)]
#![allow(clippy::incompatible_msrv, reason = "Expect current stable for tests")]

use super::model::ConfigRule;
use super::parser2;
use assert2::check;

/// Convenience function to easily compare parse results
fn parse(source: &str) -> Result<String, String> {
    parser2::parse(source)
        .map(|rules| {
            rules
                .iter()
                .map(ConfigRule::canonical)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .map_err(|diagnostics| {
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
    check!(parse("") == ok_str(""));
    check!(parse("#") == ok_str(""));
    check!(parse("# comment") == ok_str(""));
    check!(parse("   ") == ok_str(""));
    check!(parse("   # comment") == ok_str(""));
    check!(parse("\n# comment") == ok_str(""));
    check!(parse("# comment\n") == ok_str(""));
}

#[test_log::test]
fn bad_char() {
    // FIXME these errors seem wrong
    check!(
        parse("=")
            == err_str(
                r#"invalid syntax, expected one of: <bare glob>, <end of file>, <identifier>, '\n', <path>, <"string">, <'string'>, '}' 0..1"#
            )
    );
    check!(
        parse("{")
            == err_str(
                r#"invalid syntax, expected one of: <bare glob>, <end of file>, <identifier>, '\n', <path>, <"string">, <'string'>, '}' 0..1"#
            )
    );
    check!(
        parse("}") == err_str("invalid syntax, expected: <end of file> 0..1")
    );
}

#[test_log::test]
fn one_token() {
    check!(parse("foo") == ok_str(r#"/** "foo""#));
}

#[test_log::test]
fn one_rule() {
    check!(parse("/ foo") == ok_str(r#"/** "foo""#));
    check!(parse("/ foo\n") == ok_str(r#"/** "foo""#));
    check!(parse("/ foo#comment") == ok_str(r#"/** "foo""#));
    check!(parse("/ foo#comment\n") == ok_str(r#"/** "foo""#));
    check!(parse("#comment\n/ foo") == ok_str(r#"/** "foo""#));
}

#[test_log::test]
fn one_rule_function() {
    check!(parse(r#"/ literal("ok")"#) == ok_str(r#"/** literal("ok")"#));
}

#[test_log::test]
fn bad_var() {
    check!(parse("/ $bad") == err_str(r#"found unknown variable "bad" 2..6"#));
    check!(
        parse(r#"/ "/abc/$extra_bad1/def""#)
            == err_str(r#"found unknown variable "extra_bad1" 8..19"#)
    );
    check!(
        parse(r#"/ "/abc/$/def""#)
            == err_str("found unescaped '$' without variable in string 8..9")
    );
    check!(
        parse("/ '$bad1$path$bad2'")
            == err_str(
                "found unknown variable \"bad1\" 3..8\n\
                found unknown variable \"bad2\" 13..18"
            )
    );
}

#[test_log::test]
fn good_var() {
    check!(parse("/ $path") == ok_str(r#"/** "${path}""#));
}

#[test_log::test]
fn bad_function() {
    check!(parse("/ bad()") == err_str(r#"found unknown function "bad" 2..5"#));
    check!(
        parse("/ error(bad())")
            == err_str(r#"found unknown function "bad" 8..11"#)
    );
    check!(
        parse("/ error(1, 2)")
            == err_str(
                "found 2 parameters in call to error(); expected 1 2..7"
            )
    );
    check!(
        parse("/ error()")
            == err_str(
                "found 0 parameters in call to error(); expected 1 2..7"
            )
    );
}

#[test_log::test]
fn good_function() {
    check!(parse("/ error(403)") == ok_str(r#"/** error("403")"#));
}

#[test_log::test]
fn bad_setting() {
    check!(
        parse("bad = nope")
            == err_str(r#"found unknown setting name "bad" 0..3"#)
    );
    check!(
        parse("bad = error(403)")
            == err_str(r#"found unknown setting name "bad" 0..3"#)
    );
    check!(
        parse("root = error(403)")
            == err_str(
                "setting \"root\" does not accept a function result as a value \
                0..4"
            )
    );
}

#[test_log::test]
fn good_setting() {
    check!(parse("root = /tmp") == ok_str(r#"/** root = "/tmp""#));
}
