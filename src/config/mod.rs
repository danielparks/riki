//! Handle configuration files;
#![expect(dead_code, reason = "wip")]
#![allow(clippy::missing_docs_in_private_items, reason = "wip")]
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod lexer;
pub mod parser;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use lexer::{Diagnostic, Span, Token};
use logos::Logos;
use parser::{CNode, CNodeIter, Cst, Node, NodeRef, Parser, Rule, RuleSide};
use std::fmt;
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
        if !diagnostics.is_empty() {
            for diag in &diagnostics {
                term::emit(&mut err_stream.lock(), &config, &file, diag)
                    .unwrap();
            }
            return Ok(()); // FIXME error?
        }

        println!("{cst}");
        process_cst(&cst);
    }

    Ok(())
}

/// Process the CST from the parse into rules.
fn process_cst(cst: &Cst) {
    let mut iter = cst.descendents(NodeRef::ROOT);
    let mut matcher_stack: Vec<(WordType, Span)> = Vec::with_capacity(5);
    while let Some(node) = iter.next() {
        use RuleSide::{Pop, Push};
        #[expect(clippy::match_same_arms, reason = "clarity")]
        match node {
            CNode::Rule(Rule::Action, Push) => {
                matcher_stack.push(consume_matcher(&mut iter, ""));
                // FIXME emit rule
                expect_rule(&mut iter, Rule::Action, Pop, " after action push");
            }
            CNode::Rule(
                rule @ (Rule::Action
                | Rule::Function
                | Rule::Matcher
                | Rule::Set
                | Rule::Value),
                Pop,
            ) => panic!("{rule:?} pop rule should have already been consumed"),
            CNode::Rule(Rule::Context, Push) => {
                matcher_stack
                    .push(consume_matcher(&mut iter, " in context rule"));
                expect_token(&mut iter, Token::LBrace, " in context rule");
            }
            CNode::Rule(Rule::Context, Pop) => {
                matcher_stack.pop();
            }
            CNode::Rule(
                rule @ (Rule::Block | Rule::Line | Rule::Params),
                _,
            ) => {
                panic!("{rule:?} rule should be elided by grammar")
            }
            CNode::Rule(Rule::Error, _) => {
                panic!("found error rule (should have prevented processing)")
            }
            CNode::Rule(Rule::File, _) => {
                // Either start or end of file
                assert!(
                    matcher_stack.is_empty(),
                    "matcher stack must be empty at start and end of file"
                );
            }
            CNode::Rule(Rule::Function, Push) => {
                // FIXME emit?
                let _ = consume_function_contents(&mut iter);
            }
            CNode::Rule(Rule::Matcher, Push) => {
                panic!("unexpected matcher rule");
            }
            CNode::Rule(Rule::Rule, Push) => {
                matcher_stack.push(consume_matcher(&mut iter, " in rule rule"));
                // Value or function next — let this loop take care of it.
            }
            CNode::Rule(Rule::Rule, Pop) => {
                matcher_stack.pop();
            }
            CNode::Rule(Rule::Set, Push) => {
                let _id_span =
                    consume_bare_word_token(&mut iter, " for set variable");
                expect_token(&mut iter, Token::Equal, " in set rule");
                let _value = consume_value(&mut iter, " in set rule");
                expect_rule(&mut iter, Rule::Set, Pop, " after set push rule");
            }
            CNode::Rule(Rule::Value, Push) => {
                // FIXME emit
                let _ = consume_value_contents(&mut iter);
            }
            CNode::Token(Token::EOF, _) => {
                // FIXME remove?
                panic!("EOF???")
            }
            CNode::Token(
                token @ (Token::BareWord
                | Token::DoubleQuoted
                | Token::SingleQuoted
                | Token::LBrace
                | Token::LParen
                | Token::RParen
                | Token::Comma
                | Token::Equal),
                _,
            ) => panic!("unexpected {token:?} token outside of a rule"),
            CNode::Token(Token::Newlines | Token::RBrace, _) => {}
            CNode::Token(Token::Error, _) => {
                panic!("unexpected error token")
            }
        }
    }
}

/// Consume contents of a function rule.
fn consume_function_contents(iter: &mut CNodeIter) -> Span {
    // FIXME
    // Get identifier (name of function)
    let id_span = consume_bare_word_token(iter, " for function identifier");
    // Get '('
    expect_token(iter, Token::LParen, " in function rule");

    // Process all the nodes inside the parentheses.
    while let Some(node) = iter.next() {
        match node {
            CNode::Rule(Rule::Value, RuleSide::Push) => {
                let _ = consume_value_contents(iter);
            }
            CNode::Rule(Rule::Value, RuleSide::Pop) => {
                panic!("unexpected value rule pop; should have been consumed")
            }
            CNode::Rule(Rule::Function, RuleSide::Push) => {
                let _ = consume_function_contents(iter);
            }
            // Ignore tokens — we rely on the parser grammar to make sure these
            // are in the correct places.
            CNode::Token(Token::Comma | Token::RParen, _) => {}
            // Find the end of the function. We always consume both in a pair
            // so we can never get one that corresponds to a different function.
            CNode::Rule(Rule::Function, RuleSide::Pop) => {
                return id_span; // FIXME
            }
            other => panic!("expected value rule, ',', or ')', got {other:?}"),
        }
    }
    panic!("expected value rule, ',', or ')', but the file ended")
}

/// Consume a value rule.
fn consume_value<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> (WordType, Span) {
    expect_rule(iter, Rule::Value, RuleSide::Push, context);
    consume_value_contents(iter)
}

/// Consume contents of a value rule.
fn consume_value_contents(iter: &mut CNodeIter) -> (WordType, Span) {
    let word_span = consume_word_token(iter, " in value");
    expect_rule(iter, Rule::Value, RuleSide::Pop, " after value token");
    word_span
}

/// Check that the next node is a matcher rule, then get the token it contains.
fn consume_matcher<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> (WordType, Span) {
    expect_rule(iter, Rule::Matcher, RuleSide::Push, &context);
    let word_span =
        consume_word_token(iter, format!(" in matcher rule{context}"));
    expect_rule(iter, Rule::Matcher, RuleSide::Pop, " after matcher token");
    word_span
}

/// Consume a bare word token.
fn consume_bare_word_token<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> Span {
    let (word_type, span) = consume_word_token(iter, &context);
    assert_eq!(
        word_type,
        WordType::Bare,
        "expected bare word token{context}"
    );
    span
}

/// Check that the next node is a token and return it.
fn consume_word_token<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> (WordType, Span) {
    let next = iter.next();
    let Some(CNode::Token(token, span)) = next else {
        panic!("expected word token{context}, got {next:?}");
    };

    (token.try_into().unwrap(), span)
}

/// Check that the next node is a certain rule.
fn expect_rule<D: fmt::Display>(
    iter: &mut CNodeIter,
    expected: Rule,
    expected_side: RuleSide,
    context: D,
) {
    let next = iter.next();
    let Some(CNode::Rule(rule, side)) = next else {
        panic!(
            "expected {expected:?} {expected_side:?} rule{context}, got {next:?}"
        );
    };
    assert_eq!(
        (expected, expected_side),
        (rule, side),
        "expected {expected:?} {expected_side:?} rule{context}, got {rule:?} {side:?} rule)"
    );
}

/// Check that the next node is a specific token and return it.
fn expect_token<D: fmt::Display>(
    iter: &mut CNodeIter,
    expected: Token,
    context: D,
) -> Span {
    let next = iter.next();
    let Some(CNode::Token(token, span)) = next else {
        panic!("expected token{context}, got {next:?}");
    };
    assert_eq!(
        expected, token,
        "expected {expected:?} token{context}, got {token:?}"
    );
    span
}

fn print_node(prefix: &str, cst: &Cst, node_ref: NodeRef) {
    match cst.get(node_ref) {
        Node::Rule(rule, _) => {
            println!("{prefix}{rule:?}");
        }
        Node::Token(token, _) => {
            // FIXME this seems like a wonky way to access this
            let (s, span) = cst.match_token(node_ref, token).unwrap();
            println!("{prefix}{token:?} {s:?} [{span:?}]");
        }
    }
}

/// The type of a word
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordType {
    /// Bare word
    Bare,
    /// Single quoted
    ///
    /// Evaluates to a literal path segment as a matcher.
    SingleQuoted,
    /// Double quoted
    ///
    /// Evaluated as a glob when used as a matcher.
    DoubleQuoted,
}

impl TryFrom<Token> for WordType {
    type Error = ParseError;

    fn try_from(value: Token) -> Result<Self, Self::Error> {
        match value {
            Token::BareWord => Ok(Self::Bare),
            Token::SingleQuoted => Ok(Self::SingleQuoted),
            Token::DoubleQuoted => Ok(Self::DoubleQuoted),
            other => Err(ParseError::ExpectedWordToken(other)),
        }
    }
}

/// Errors that could be produced from parsing code.
///
/// Does not include lexer errors or errors sent to diagnostics.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ParseError {
    /// Found something other than a word token.
    #[error("expected a word token, got {0:?}")]
    ExpectedWordToken(Token),
}

/// A rule found in the configuration file.
#[derive(Debug, Clone)]
pub struct ConfigRule<'a> {
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
