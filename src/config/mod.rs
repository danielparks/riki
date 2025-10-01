//! Handle configuration files;
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod lexer;
pub mod parser;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use lexer::{Diagnostic, Span, TokenType};
use logos::Logos;
use parser::{CNode, CNodeIter, Cst, NodeRef, Parser, Rule, RuleSide};
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
    let mut matcher_stack: Vec<Word> = Vec::with_capacity(5);
    while let Some(node) = iter.next() {
        use RuleSide::{Pop, Push};
        #[expect(clippy::match_same_arms, reason = "clarity")]
        match node {
            CNode::Rule(Rule::Action, Push) => {
                matcher_stack.push(consume_matcher(&mut iter, ""));
                // FIXME emit rule
                println!("RULE {matcher_stack:?}");
                assert!(matcher_stack.pop().is_some());
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
                expect_token(&mut iter, TokenType::LBrace, " in context rule");
            }
            CNode::Rule(Rule::Context, Pop) => {
                assert!(matcher_stack.pop().is_some());
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
                let function = consume_function_contents(&mut iter);
                // FIXME emit rule
                println!("RULE {matcher_stack:?} {function:?}");
            }
            CNode::Rule(Rule::Matcher, Push) => {
                panic!("unexpected matcher rule");
            }
            CNode::Rule(Rule::Rule, Push) => {
                matcher_stack.push(consume_matcher(&mut iter, " in rule rule"));
                // Value or function next — let this loop take care of it.
            }
            CNode::Rule(Rule::Rule, Pop) => {
                assert!(matcher_stack.pop().is_some());
            }
            CNode::Rule(Rule::Set, Push) => {
                let setting = consume_set_contents(&mut iter);

                // FIXME emit set
                println!("SET {matcher_stack:?} {setting:?}");
            }
            CNode::Rule(Rule::Value, Push) => {
                let value = consume_value_contents(&mut iter);
                // FIXME emit rule
                println!("RULE {matcher_stack:?} {value:?}");
            }
            CNode::Token(
                token @ (TokenType::BareWord
                | TokenType::DoubleQuoted
                | TokenType::SingleQuoted
                | TokenType::LBrace
                | TokenType::LParen
                | TokenType::RParen
                | TokenType::Comma
                | TokenType::Equal),
                _,
            ) => panic!("unexpected {token:?} token outside of a rule"),
            CNode::Token(token @ (TokenType::Error | TokenType::EOF), _) => {
                panic!("unexpected {token:?} token")
            }
            CNode::Token(TokenType::Newlines | TokenType::RBrace, _) => {
                // Ignore
            }
        }
    }
}

/// Consume contents of a set rule.
fn consume_set_contents(iter: &mut CNodeIter) -> Setting {
    let variable = consume_bare_word_token(iter, " for set variable");
    expect_token(iter, TokenType::Equal, " in set rule");

    let value = match iter.next() {
        Some(CNode::Rule(Rule::Function, RuleSide::Push)) => {
            consume_function_contents(iter)
        }
        Some(CNode::Rule(Rule::Value, RuleSide::Push)) => {
            consume_value_contents(iter)
        }
        Some(other) => {
            panic!("expected function or value push rule; got {other:?}");
        }
        None => panic!("expected function or value push rule; got end of file"),
    };

    expect_rule(iter, Rule::Set, RuleSide::Pop, " after set push rule");
    Setting { variable, value }
}

/// Consume contents of a function rule.
fn consume_function_contents(iter: &mut CNodeIter) -> Value {
    // Get identifier (name of function)
    let identifier = consume_bare_word_token(iter, " for function identifier");
    let mut parameters = Parameters::new();
    // Get '('
    expect_token(iter, TokenType::LParen, " in function rule");

    // Process all the nodes inside the parentheses.
    while let Some(node) = iter.next() {
        match node {
            CNode::Rule(Rule::Value, RuleSide::Push) => {
                parameters.push(consume_value_contents(iter));
            }
            CNode::Rule(Rule::Value, RuleSide::Pop) => {
                panic!("unexpected value rule pop; should have been consumed")
            }
            CNode::Rule(Rule::Function, RuleSide::Push) => {
                parameters.push(consume_function_contents(iter));
            }
            // Ignore tokens — we rely on the parser grammar to make sure these
            // are in the correct places.
            CNode::Token(TokenType::Comma | TokenType::RParen, _) => {}
            // Find the end of the function. We always consume both in a pair
            // so we can never get one that corresponds to a different function.
            CNode::Rule(Rule::Function, RuleSide::Pop) => {
                return Value::Function(identifier, parameters);
            }
            other => panic!("expected value rule, ',', or ')', got {other:?}"),
        }
    }
    panic!("expected value rule, ',', or ')', but the file ended")
}

/// Consume contents of a value rule.
fn consume_value_contents(iter: &mut CNodeIter) -> Value {
    let word = consume_word_token(iter, " in value");
    expect_rule(iter, Rule::Value, RuleSide::Pop, " after value token");
    word.into()
}

/// Check that the next node is a matcher rule, then get the token it contains.
fn consume_matcher<D: fmt::Display>(iter: &mut CNodeIter, context: D) -> Word {
    expect_rule(iter, Rule::Matcher, RuleSide::Push, &context);
    let word = consume_word_token(iter, format!(" in matcher rule{context}"));
    expect_rule(iter, Rule::Matcher, RuleSide::Pop, " after matcher token");
    word
}

/// Consume a bare word token.
fn consume_bare_word_token<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> BareWord {
    consume_word_token(iter, &context)
        .try_into()
        .unwrap_or_else(|_| panic!("expected bare word token{context}"))
}

/// Check that the next node is a token and return it.
fn consume_word_token<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> Word {
    let next = iter.next();
    let Some(CNode::Token(token, span)) = next else {
        panic!("expected word token{context}, got {next:?}");
    };

    Word { type_: token.try_into().unwrap(), span }
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
    expected: TokenType,
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

impl TryFrom<TokenType> for WordType {
    type Error = ParseError;

    fn try_from(value: TokenType) -> Result<Self, Self::Error> {
        match value {
            TokenType::BareWord => Ok(Self::Bare),
            TokenType::SingleQuoted => Ok(Self::SingleQuoted),
            TokenType::DoubleQuoted => Ok(Self::DoubleQuoted),
            other => Err(ParseError::ExpectedWordToken(other)),
        }
    }
}

impl From<WordType> for TokenType {
    fn from(type_: WordType) -> Self {
        match type_ {
            WordType::Bare => Self::BareWord,
            WordType::SingleQuoted => Self::SingleQuoted,
            WordType::DoubleQuoted => Self::DoubleQuoted,
        }
    }
}

/// A reference to a word in the config file
#[derive(Clone, Debug)]
pub struct Word {
    /// The type of word
    pub type_: WordType,

    /// The location in the source
    pub span: Span,
}

/// A reference to a word in the config file
#[derive(Clone, Debug)]
pub struct BareWord(pub Span);

impl TryFrom<Word> for BareWord {
    type Error = ParseError;

    fn try_from(word: Word) -> Result<Self, Self::Error> {
        match word.type_ {
            WordType::Bare => Ok(Self(word.span)),
            other => Err(ParseError::ExpectedWordToken(other.into())),
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
    ExpectedWordToken(TokenType),

    /// Found something other than a bare word token.
    #[error("expected a bare word token, got {0:?}")]
    ExpectedBareWordToken(TokenType),
}

/// A rule found in the configuration file.
#[derive(Debug, Clone)]
pub struct ConfigRule {
    /// Match a request
    pub matcher: MatcherSequence,
    /// Action to take in response to a request
    pub action: Action,
}

/// A full sequence of matchers.
#[derive(Debug, Clone)]
pub struct MatcherSequence(pub Vec<Matcher>);

/// A matcher for a request.
#[derive(Debug, Clone)]
pub struct Matcher(pub Word);

/// The action corresponding to a rule.
#[derive(Debug, Clone)]
pub enum Action {
    /// Set an option for matching requests.
    Setting(Setting),

    /// Value to return for matching requests.
    Value(Value),
}

/// A configuration setting
#[derive(Debug, Clone)]
pub struct Setting {
    /// The variable being set
    pub variable: BareWord,

    /// The value
    pub value: Value,
}

/// A value for a configuration setting or to return as a response.
#[derive(Debug, Clone)]
pub enum Value {
    /// Call a function.
    Function(BareWord, Parameters),

    /// A string of some kind.
    Literal(Word),
}

impl From<Word> for Value {
    fn from(word: Word) -> Self {
        Self::Literal(word)
    }
}

/// Parameters to a function call.
pub type Parameters = Vec<Value>;

/// Tokenize
pub fn tokenize(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(TokenType, Span)> {
    let lexer = TokenType::lexer(source);
    let mut output = vec![];

    for (token, span) in lexer.spanned() {
        let token = token.unwrap_or_else(|err| {
            diagnostics.push(err.into_diagnostic(span.clone()));
            TokenType::Error
        });

        output.push((token, span));
    }
    output
}
