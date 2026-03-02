//! Handle configuration files
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod actions;
pub mod errors;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod parser2;
mod tests;

use bstr::BStr;
use lexer::{Diagnostic, tokenize};
use model::{ConfigSettings, Configuration};
use parser::Cst;
use parser::Parser;
use parser2::ContentSource;

/// Dump canonical version of `configuration` to stdout.
pub fn dump_canonical(configuration: &Configuration<'_>) {
    let mut settings = &ConfigSettings::default();
    for rule in configuration.rules() {
        if &rule.settings != settings {
            settings = &rule.settings;
            println!("{}", settings.canonical("/**").join("\n"));
        }
        println!("{}", rule.canonical());
    }
}

/// Dump the tokens from a configuration file to stdout.
///
/// For debugging and development.
///
/// # Errors
///
/// Returns <code>Vec<[Diagnostic]></code> for parse errors.
pub fn dump_config_tokens<S: ContentSource>(
    source: &S,
) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    for (token, span) in tokenize(source.content(), &mut diagnostics) {
        println!(
            "{token:?}({:?})",
            BStr::new(&source.content().as_bytes()[span])
        );
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Parse a configuration
///
/// # Errors
///
/// Returns <code>Vec<[Diagnostic]></code> for parse errors.
pub fn parse<S: ContentSource>(
    source: &S,
) -> Result<Configuration<'_>, Vec<Diagnostic>> {
    parser2::process_cst(&parse_cst(source)?)
        .map_err(|errors| errors::errors_to_diagnostics(errors, source))
}

/// Parse a configuration to a CST.
///
/// # Errors
///
/// Returns <code>Vec<[Diagnostic]></code> for parse errors.
pub fn parse_cst<S: ContentSource>(
    source: &S,
) -> Result<Cst<'_>, Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    let cst = Parser::parse(source.content(), &mut diagnostics);
    if diagnostics.is_empty() {
        Ok(cst)
    } else {
        Err(diagnostics)
    }
}
