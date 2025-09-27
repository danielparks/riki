//! Handle configuration files;
#![expect(dead_code, reason = "wip")]
#![allow(clippy::missing_docs_in_private_items, reason = "wip")]

pub mod lexer;
pub mod parser;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use lexer::{Diagnostic, Span, Token};
use logos::Logos;
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

    let mut diagnostics = vec![];
    if just_tokens {
        for (token, span) in tokenize(&source, &mut diagnostics) {
            println!("{token:?}({:?})", BStr::new(&source.as_bytes()[span]));
        }
    } else {
        println!("{}", Parser::parse(&source, &mut diagnostics));
    }

    let config = Config::default();
    let file = SimpleFile::new(path, &source);
    for diag in &diagnostics {
        term::emit(&mut err_stream.lock(), &config, &file, diag).unwrap();
    }

    Ok(())
}

/// A rule found in the configuration file.
#[derive(Debug, Clone)]
pub struct Rule<'a> {
    matcher: MatcherSequence<'a>,
    action: Action<'a>,
}

/// A full sequence of matchers.
#[derive(Debug, Clone)]
pub struct MatcherSequence<'a>(pub Vec<Matcher<'a>>);

/// A matcher for a request.
#[derive(Debug, Clone)]
pub struct Matcher<'a>(&'a str);

/// The action corresponding to a rule.
#[derive(Debug, Clone)]
pub enum Action<'a> {
    /// Configure an option for matching requests.
    Configure(&'a str, Value<'a>),

    /// Value to return for matching requests.
    Value(Value<'a>),
}

/// A value for a configuration option or to return as a response.
#[derive(Debug, Clone)]
pub enum Value<'a> {
    /// Call a function.
    Function(&'a str, Vec<Self>),

    /// A string of some kind.
    Literal(&'a str),
}

/// Tokenize
pub fn tokenize(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(Token, Span)> {
    let lexer = Token::lexer(source);
    let mut output = vec![];

    for (token, span) in lexer.spanned() {
        let token = token.unwrap_or_else(|err| {
            diagnostics.push(err.into_diagnostic(span.clone()));
            Token::Error
        });

        output.push((token, span));
    }
    output
}
