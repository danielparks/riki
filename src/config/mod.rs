//! Handle configuration files;
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod lexer;
pub mod model;
pub mod parser;
pub mod parser2;
pub mod string;
mod tests;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use lexer::tokenize;
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
    let path = BString::new(Vec::from_path_lossy(path).to_vec()); // Display
    let config = Config::default();
    let file = SimpleFile::new(path, &source);

    let mut diagnostics = vec![];
    if just_tokens {
        for (token, span) in tokenize(&source, &mut diagnostics) {
            println!("{token:?}({:?})", BStr::new(&source.as_bytes()[span]));
        }

        for diag in &diagnostics {
            term::emit(&mut err_stream.lock(), &config, &file, diag).unwrap();
        }
    } else {
        let cst = Parser::parse(&source, &mut diagnostics);

        println!("{cst}");

        let diagnostics = if diagnostics.is_empty() {
            match parser2::process_cst(&cst) {
                Ok(rules) => {
                    println!();
                    for rule in rules {
                        println!("{}", rule.canonical());
                    }

                    return Ok(());
                }
                Err(errors) => errors
                    .into_iter()
                    .map(|error| error.into_diagnostic(&source))
                    .collect(),
            }
        } else {
            diagnostics
        };

        println!();
        for diag in &diagnostics {
            term::emit(&mut err_stream.lock(), &config, &file, diag).unwrap();
        }
        return Ok(()); // FIXME error?
    }

    Ok(())
}
