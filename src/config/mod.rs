//! Handle configuration files
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod actions;
pub mod errors;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod parser2;
mod tests;

use crate::config::parser2::{FileSource, Source};
use bstr::BStr;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use errors::SpannedErrors;
use lexer::{Diagnostic, tokenize};
use model::ConfigSettings;
use parser::Parser;
use std::io::{self, Write};
use std::path::Path;
use termcolor::StandardStream;

/// Dump the CST of a configuration file to stdout.
///
/// For debugging and development.
///
/// # Errors
///
/// Returns an `io::Error` if it can’t read the configuration file.
///
/// # Panics
///
/// Panics if it can’t write to `err_stream` (probably stderr).
pub fn dump_config(
    path: &Path,
    err_stream: &StandardStream,
    just_tokens: bool,
) -> io::Result<()> {
    let source = FileSource::read(path)?;

    let mut diagnostics = vec![];
    if just_tokens {
        for (token, span) in tokenize(&source.content, &mut diagnostics) {
            println!(
                "{token:?}({:?})",
                BStr::new(&source.content.as_bytes()[span])
            );
        }
    } else {
        let cst = Parser::parse(&source.content, &mut diagnostics);

        println!("{cst}");

        if diagnostics.is_empty() {
            diagnostics = match parser2::process_cst(&cst) {
                Ok(configuration) => {
                    let mut settings = &ConfigSettings::default();
                    println!();
                    for rule in configuration.rules() {
                        if &rule.settings != settings {
                            settings = &rule.settings;
                            println!(
                                "{}",
                                settings.canonical("/**").join("\n")
                            );
                        }
                        println!("{}", rule.canonical());
                    }

                    return Ok(());
                }
                Err(errors) => errors
                    .into_iter()
                    .map(|error| error.into_diagnostic(&source))
                    .collect(),
            }
        }

        println!();
    }

    print_diagnostics(&source, err_stream, &diagnostics);
    Ok(()) // FIXME error?
}

/// Print errors found in configuration file.
///
/// # Panics
///
/// Panics if it can’t write to `err_stream` (probably stderr).
pub fn print_errors<'src, S: Source>(
    source: &'src S,
    err_stream: &StandardStream,
    errors: SpannedErrors<'src>,
) {
    print_diagnostics(
        source,
        err_stream,
        &errors
            .into_iter()
            .map(|error| error.into_diagnostic(source))
            .collect::<Vec<_>>(),
    );
}

/// Print diagnostics found in configuration file.
///
/// # Panics
///
/// Panics if it can’t write to `err_stream` (probably stderr).
pub fn print_diagnostics<S: Source>(
    source: &S,
    err_stream: &StandardStream,
    diagnostics: &[Diagnostic],
) {
    let out = &mut err_stream.lock();
    let config = Config::default();
    let file = SimpleFile::new(
        source.name(),
        if let Some(content) = source.source() {
            content
        } else {
            writeln!(out, "Found errors in {}:", source.name()).unwrap();
            ""
        },
    );

    for diag in diagnostics {
        term::emit(out, &config, &file, diag).unwrap();
    }
}
