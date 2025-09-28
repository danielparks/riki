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

    /// Check second token isn’t EOF or '}'
    ///
    /// ```lelwel
    /// context: (?1 Newlines line)* [Newlines]
    ///   | line (?1 Newlines line)* [Newlines];
    /// ```
    fn predicate_context_1(&self) -> bool {
        // FIXME Token::EOF? Does this work?
        !matches!(self.peek(1), Token::EOF | Token::RBrace)
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

impl<'a> Cst<'a> {
    pub fn descendents(&'a self, id: NodeRef) -> CNodeIter<'a> {
        let end_offset = match self.nodes[id.0] {
            Node::Rule(_, i) => usize::from(i),
            _ => 0,
        };

        CNodeIter {
            next_index: 0,
            nodes: &self.nodes[id.0 + 1..id.0 + 1 + end_offset],
            spans: &self.spans[..],
            stack: Vec::new(), // FIXME capacity from end_offset
        }
    }
}

pub struct CNodeIter<'a> {
    next_index: usize,
    nodes: &'a [Node],
    spans: &'a [Span],

    /// `Vec` of tuples:
    ///
    ///   1. Index of rule in question in `self.nodes`.
    ///   2. Index after last of its descendents.
    stack: Vec<(usize, usize)>,
}

impl<'a> Iterator for CNodeIter<'a> {
    type Item = CNode;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if the next node is outside the top rule on the stack.
        if let Some((rule_index, _)) = self.stack.pop_if(|(_, finished)| {
            assert!(
                self.next_index <= *finished,
                "escaped containing rule without noticing"
            );
            self.next_index == *finished
        }) {
            match self.nodes[rule_index] {
                Node::Rule(rule, _) => {
                    return Some(CNode::Rule(rule, RuleSide::Pop));
                }
                Node::Token(..) => {
                    panic!("CNodeIter stack pointed to token instead of rule");
                }
            }
        }

        match self.nodes.get(self.next_index)? {
            Node::Rule(rule, i) => {
                self.stack.push((
                    self.next_index,
                    usize::from(*i) + self.next_index + 1,
                ));
                self.next_index += 1;
                Some(CNode::Rule(*rule, RuleSide::Push))
            }
            Node::Token(token, i) => {
                self.next_index += 1;
                Some(CNode::Token(*token, self.spans[usize::from(*i)].clone()))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleSide {
    /// Entering the rule
    Push,
    /// Leaving the rule
    Pop,
}

#[derive(Clone, Debug)]
pub enum CNode {
    Rule(Rule, RuleSide),
    Token(Token, Span),
}

impl Node {
    /// Is this node a Rule?
    #[inline]
    pub fn is_rule(&self) -> bool {
        matches!(self, Self::Rule(..))
    }

    /// Is this node a Token?
    #[inline]
    pub fn is_token(&self) -> bool {
        matches!(self, Self::Token(..))
    }
}
