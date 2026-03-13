//! Configuration file parser.
#![allow(
    missing_docs,
    clippy::missing_docs_in_private_items,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction,
    reason = "generated code"
)]

use super::lexer::TokenType as Token;
use super::lexer::{Diagnostic, tokenize};
use super::parser2::ContentSource;
use codespan_reporting::diagnostic::Label;

/// Parse a configuration to a CST.
///
/// # Errors
///
/// Returns <code>Vec<[Diagnostic]></code> for parse errors.
pub fn parse_to_cst<S: ContentSource>(
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

/// Context required for [`Parser`]. Not needed by our code.
#[derive(Default)]
pub struct Context<'a> {
    /// [`Parser`] requires this struct have a lifetime; this tracks it.
    marker: std::marker::PhantomData<&'a ()>,
}

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

impl ParserCallbacks for Parser<'_> {
    /// Hook to produce tokens for the generated [`Parser`].
    fn create_tokens(
        source: &str,
        diags: &mut Vec<Diagnostic>,
    ) -> (Vec<Token>, Vec<Span>) {
        let mut tokens = Vec::new();
        let mut spans = Vec::new();

        for (token, span) in tokenize(source, diags) {
            tokens.push(token);
            spans.push(span);
        }
        (tokens, spans)
    }

    /// Hook to allow the generated [`Parser`] to create a diagnostic.
    fn create_diagnostic(&self, span: Span, message: String) -> Diagnostic {
        Diagnostic::error()
            .with_message(message)
            .with_label(Label::primary((), span))
    }

    /// Check the token after `Newline+` isn’t EOF or '}'
    ///
    /// ```lelwel
    /// context: (?1 Newline+ line)* Newline*
    ///   | line (?1 Newline+ line)* Newline*;
    /// ```
    fn predicate_context_1(&self) -> bool {
        let mut i = 1;
        while self.peek(i) == Token::Newline {
            i += 1;
        }
        !matches!(self.peek(i), Token::EOF | Token::RBrace)
    }

    /// Check second token is '='
    fn predicate_line_1(&self) -> bool {
        matches!(self.peek(1), Token::Equal)
    }

    /// Check second token is '('
    fn predicate_line_2(&self) -> bool {
        matches!(self.peek(1), Token::LParen)
    }
}

impl<'src> Cst<'src> {
    /// Iterate all of the descendents of a [`Cst`] node returned by [`Parser`].
    pub fn descendents<'a>(&'a self, id: NodeRef) -> CNodeIter<'a, 'src>
    where
        'src: 'a,
    {
        let end_offset = match self.nodes[id.0] {
            Node::Rule(_, last_child_offset) => usize::from(last_child_offset),
            _ => 0,
        };

        CNodeIter {
            next_index: 0,
            nodes: &self.nodes[id.0 + 1..id.0 + 1 + end_offset],
            spans: &self.spans[..],
            source: self.source,
            // 16 is bigger than required for default configuration:
            stack: Vec::with_capacity(16),
        }
    }
}

/// Iterator for descendents of a [`Cst`] node.
pub struct CNodeIter<'a, 'src> {
    next_index: usize,
    nodes: &'a [Node],
    spans: &'a [Span],
    source: &'src str,

    /// `Vec` of tuples:
    ///
    ///   1. Rule being traversed.
    ///   2. Index after last of its descendents.
    stack: Vec<(Rule, usize)>,
}

impl<'a, 'src> Iterator for CNodeIter<'a, 'src> {
    type Item = CNode<'src>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if the next node is outside the top rule on the stack.
        if let Some(rule) = self.pop_stack_if_finished() {
            return Some(CNode::Rule(rule, RuleSide::Pop));
        }

        match self.nodes.get(self.next_index)? {
            Node::Rule(rule, i) => {
                self.stack
                    .push((*rule, usize::from(*i) + self.next_index + 1));
                self.next_index += 1;
                Some(CNode::Rule(*rule, RuleSide::Push))
            }
            Node::Token(token, i) => {
                self.next_index += 1;
                Some(CNode::Token(
                    *token,
                    &self.source[self.spans[usize::from(*i)].clone()],
                ))
            }
        }
    }
}

impl<'a, 'src> CNodeIter<'a, 'src> {
    /// If we’re at the end of the current rule, pop and return the rule.
    fn pop_stack_if_finished(&mut self) -> Option<Rule> {
        self.stack
            .pop_if(|(_, finished)| {
                assert!(
                    self.next_index <= *finished,
                    "escaped containing rule without noticing"
                );
                self.next_index == *finished
            })
            .map(|(rule, _)| rule)
    }
}

/// A node in the [`Cst`] in a more useful format.
#[derive(Clone, Debug)]
pub enum CNode<'a> {
    Rule(Rule, RuleSide),
    Token(Token, &'a str),
}

/// When iterating over [`CNode`]s we encounter rule nodes twice — pushing them
/// on to the stack, or popping them off.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleSide {
    /// Entering the rule
    Push,
    /// Leaving the rule
    Pop,
}
