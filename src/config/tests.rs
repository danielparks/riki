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
    check!(parse("foo") == ok_str("/ foo"));
}

#[test_log::test]
fn one_rule() {
    check!(parse("/ foo") == ok_str("/ foo"));
    check!(parse("/ foo\n") == ok_str("/ foo"));
    check!(parse("/ foo#comment") == ok_str("/ foo"));
    check!(parse("/ foo#comment\n") == ok_str("/ foo"));
    check!(parse("#comment\n/ foo") == ok_str("/ foo"));
}

#[test_log::test]
fn one_rule_function() {
    check!(parse(r#"/ literal("ok")"#) == ok_str(r#"/ literal("ok")"#));
}
