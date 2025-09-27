//! Configuration file parser.
#![allow(
    missing_docs,
    clippy::missing_docs_in_private_items,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction,
    reason = "generated code"
)]

use super::lexer::{Diagnostic, Token, tokenize};
use codespan_reporting::diagnostic::Label;

// TODO: add context information to the parser if required
#[derive(Default)]
pub struct Context<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

impl ParserCallbacks for Parser<'_> {
    fn create_tokens(
        source: &str,
        diags: &mut Vec<Diagnostic>,
    ) -> (Vec<Token>, Vec<Span>) {
        tokenize(source, diags)
    }

    fn create_diagnostic(&self, span: Span, message: String) -> Diagnostic {
        Diagnostic::error()
            .with_message(message)
            .with_label(Label::primary((), span))
    }

    /// Check second token isn’t EOF
    ///
    /// ```lelwel
    /// file: (?1 Newlines line)* [Newlines]
    ///   | line (?1 Newlines line)* [Newlines];
    /// ```
    fn predicate_file_1(&self) -> bool {
        self.peek(1) != Token::EOF
    }
}
