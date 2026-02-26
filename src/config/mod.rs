//! Handle configuration files
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod actions;
pub mod errors;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod parser2;
mod tests;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use errors::SpannedErrors;
use lexer::{Diagnostic, tokenize};
use model::ConfigSettings;
use parser::Parser;
use std::fs;
use std::io;
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
    let source = fs::read_to_string(path)?;

    let mut diagnostics = vec![];
    if just_tokens {
        for (token, span) in tokenize(&source, &mut diagnostics) {
            println!("{token:?}({:?})", BStr::new(&source.as_bytes()[span]));
        }
    } else {
        let cst = Parser::parse(&source, &mut diagnostics);

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

    print_diagnostics(path, &source, err_stream, &diagnostics);
    Ok(()) // FIXME error?
}

/// Print errors found in configuration file.
///
/// # Panics
///
/// Panics if it can’t write to `err_stream` (probably stderr).
pub fn print_errors<'src, P: AsRef<Path>>(
    path: P,
    source: &'src str,
    err_stream: &StandardStream,
    errors: SpannedErrors<'src>,
) {
    print_diagnostics(
        path,
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
pub fn print_diagnostics<P: AsRef<Path>>(
    path: P,
    source: &str,
    err_stream: &StandardStream,
    diagnostics: &[Diagnostic],
) {
    let config = Config::default();
    let path = BString::new(Vec::from_path_lossy(path.as_ref()).to_vec());
    let file = SimpleFile::new(path, source); // BString implements Display

    for diag in diagnostics {
        term::emit(&mut err_stream.lock(), &config, &file, diag).unwrap();
    }
}
