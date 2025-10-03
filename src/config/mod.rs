//! Handle configuration files;
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod lexer;
pub mod parser;
mod tests;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use lexer::{Diagnostic, Span, TokenType, tokenize};
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

        println!("{cst}");

        if !diagnostics.is_empty() {
            println!();
            for diag in &diagnostics {
                term::emit(&mut err_stream.lock(), &config, &file, diag)
                    .unwrap();
            }
            return Ok(()); // FIXME error?
        }

        println!();
        for rule in process_cst(&cst) {
            println!("{}", rule.canonical(&source));
        }
    }

    Ok(())
}

/// Parse configuration file contents and return rules or errors.
///
/// # Errors
///
/// Returns [`Diagnostic`]s that point out problems in `source`.
pub fn parse(source: &str) -> Result<Vec<ConfigRule>, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let cst = Parser::parse(source, &mut diagnostics);
    if diagnostics.is_empty() {
        Ok(process_cst(&cst))
    } else {
        Err(diagnostics)
    }
}

/// Process the CST from the parse into rules.
fn process_cst(cst: &Cst) -> Vec<ConfigRule> {
    let mut iter = cst.descendents(NodeRef::ROOT);
    let mut matcher_stack: Vec<Matcher> = Vec::with_capacity(5);
    let mut rules: Vec<ConfigRule> = Vec::new();
    while let Some(node) = iter.next() {
        use RuleSide::{Pop, Push};
        #[expect(clippy::match_same_arms, reason = "clarity")]
        match node {
            CNode::Rule(Rule::Action, Push) => {
                let action =
                    Action::Value(consume_matcher(&mut iter, "").into());
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
                expect_rule(&mut iter, Rule::Action, Pop, " after action push");
            }
            CNode::Rule(Rule::Context, Push) => {
                matcher_stack.push(Matcher(consume_matcher(
                    &mut iter,
                    " in context rule",
                )));
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
                let action =
                    Action::Value(consume_function_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
            }
            CNode::Rule(Rule::Matcher, Push) => {
                panic!("unexpected matcher rule");
            }
            CNode::Rule(
                rule @ (Rule::Action
                | Rule::Function
                | Rule::Matcher
                | Rule::Set
                | Rule::Value),
                Pop,
            ) => panic!("{rule:?} pop rule should have already been consumed"),
            CNode::Rule(Rule::Rule, Push) => {
                matcher_stack
                    .push(Matcher(consume_matcher(&mut iter, " in rule rule")));
                // Value or function next — let this loop take care of it.
            }
            CNode::Rule(Rule::Rule, Pop) => {
                assert!(matcher_stack.pop().is_some());
            }
            CNode::Rule(Rule::Set, Push) => {
                let action = Action::Setting(consume_set_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
            }
            CNode::Rule(Rule::Value, Push) => {
                let action = Action::Value(consume_value_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
            }
            CNode::Token(
                token @ (TokenType::Identifier
                | TokenType::Path
                | TokenType::BareGlob
                | TokenType::QuotedDouble
                | TokenType::QuotedSingle
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
            CNode::Token(TokenType::Newline | TokenType::RBrace, _) => {
                // Ignore
            }
        }
    }

    rules
}

/// Consume contents of a set rule.
fn consume_set_contents(iter: &mut CNodeIter) -> Setting {
    let variable = consume_identifier(iter, " for set variable");
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
    let identifier = consume_identifier(iter, " for function identifier");
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
fn consume_identifier<D: fmt::Display>(
    iter: &mut CNodeIter,
    context: D,
) -> Identifier {
    consume_word_token(iter, &context)
        .try_into()
        .unwrap_or_else(|_| panic!("expected identifier token{context}"))
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
    /// Identifier
    Identifier,
    /// Path,
    Path,
    /// Bare glob
    BareGlob,
    /// Single quoted string
    QuotedSingle,
    /// Double quoted string
    QuotedDouble,
}

impl TryFrom<TokenType> for WordType {
    type Error = ParseError;

    fn try_from(value: TokenType) -> Result<Self, Self::Error> {
        match value {
            TokenType::Identifier => Ok(Self::Identifier),
            TokenType::Path => Ok(Self::Path),
            TokenType::BareGlob => Ok(Self::BareGlob),
            TokenType::QuotedSingle => Ok(Self::QuotedSingle),
            TokenType::QuotedDouble => Ok(Self::QuotedDouble),
            other => Err(ParseError::ExpectedWordToken(other)),
        }
    }
}

impl From<WordType> for TokenType {
    fn from(type_: WordType) -> Self {
        match type_ {
            WordType::Identifier => Self::Identifier,
            WordType::Path => Self::Path,
            WordType::BareGlob => Self::BareGlob,
            WordType::QuotedSingle => Self::QuotedSingle,
            WordType::QuotedDouble => Self::QuotedDouble,
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

impl Word {
    /// The string this token represents in the source.
    #[must_use]
    #[inline]
    pub fn source_str<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.clone()]
    }

    /// Return the contents of this word
    ///
    /// FIXME variable interpolation
    #[must_use]
    pub fn contents(&self, source: &str) -> String {
        let src = self.source_str(source);
        match self.type_ {
            WordType::Identifier => src.to_owned(),
            WordType::Path => path_unescape(src),
            WordType::BareGlob => bare_glob_unescape(src),
            WordType::QuotedSingle | WordType::QuotedDouble => {
                string_unescape(src)
            }
        }
    }

    /// Return the contents of this word
    ///
    /// FIXME variable interpolation
    #[must_use]
    pub fn as_glob_str(&self, source: &str) -> String {
        let src = self.source_str(source);
        match self.type_ {
            WordType::Identifier => globset::escape(src),
            WordType::Path => globset::escape(&path_unescape(src)),
            WordType::BareGlob => bare_glob_unescape(src),
            WordType::QuotedSingle | WordType::QuotedDouble => {
                globset::escape(&string_unescape(src))
            }
        }
    }

    /// Return the canonical representation of this word
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        self.source_str(source).to_owned()
    }
}

/// Get the contents of a bare glob, e.g. `contents`.
///
/// FIXME [`globset`] will take care of the unescaping.
///
/// FIXME string interpolation
fn bare_glob_unescape(src: &str) -> String {
    src.to_owned()
}

/// Get the contents of a path, e.g. `contents`.
///
/// FIXME string interpolation
/// FIXME other escape sequences
/// FIXME non-unicode?
fn path_unescape(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut iter = src.chars();
    while let Some(c) = iter.next() {
        out.push(if c == '\\' {
            match iter.next().expect("character expected after backslash") {
                'n' => '\n',
                't' => '\t',
                c => c,
            }
        } else {
            c
        });
    }
    out
}

/// Get the contents of a string, e.g. `"contents"`.
///
/// Assumes the string has a single byte quote at the start and end.
///
/// FIXME string interpolation
/// FIXME other escape sequences
/// FIXME non-unicode?
///
/// # Panics
///
/// Panics if the string isn’t at least 2 bytes long (for the quotes), or of the
/// last character before the last quote is an unescaped backslash.
#[expect(clippy::arithmetic_side_effects, reason = "src.len() should be >= 2")]
fn string_unescape(src: &str) -> String {
    debug_assert!(src.len() > 2, "expected src to be wrapped in quotes");
    let mut out = String::with_capacity(src.len() - 2);
    // Should always have single byte quotes on either end.
    let mut iter = src[1..src.len() - 1].chars();
    while let Some(c) = iter.next() {
        out.push(if c == '\\' {
            match iter.next().expect("character expected after backslash") {
                'n' => '\n',
                't' => '\t',
                c => c,
            }
        } else {
            c
        });
    }
    out
}

/// A reference to an identifier in the config file
#[derive(Clone, Debug)]
pub struct Identifier(pub Span);

impl Identifier {
    /// The string this token represents in the source.
    #[must_use]
    #[inline]
    pub fn source_str<'a>(&self, source: &'a str) -> &'a str {
        &source[self.0.clone()]
    }

    /// Return the canonical representation of this identifier
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        self.source_str(source).to_owned()
    }
}

impl TryFrom<Word> for Identifier {
    type Error = ParseError;

    fn try_from(word: Word) -> Result<Self, Self::Error> {
        match word.type_ {
            WordType::Identifier => Ok(Self(word.span)),
            other => Err(ParseError::ExpectedIdentifierToken(other.into())),
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

    /// Found something other than an identifier token.
    #[error("expected an identifier token, got {0:?}")]
    ExpectedIdentifierToken(TokenType),
}

/// A rule found in the configuration file.
#[derive(Debug, Clone)]
pub struct ConfigRule {
    /// Match a request
    pub matcher: Vec<Matcher>,
    /// Action to take in response to a request
    pub action: Action,
}

impl ConfigRule {
    /// Return the canonical representation of this rule
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        format!(
            "{} {}",
            self.matcher
                .iter()
                .map(|matcher| matcher.canonical(source))
                .collect::<Vec<_>>()
                .join(" "),
            self.action.canonical(source),
        )
    }
}

/// A matcher for a request.
#[derive(Debug, Clone)]
pub struct Matcher(pub Word);

impl Matcher {
    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        self.0.canonical(source)
    }

    /// Get the matcher stack as a glob
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    #[must_use]
    pub fn as_glob_str(&self, source: &str) -> String {
        self.0.canonical(source)
    }
}

/// The action corresponding to a rule.
#[derive(Debug, Clone)]
pub enum Action {
    /// Set an option for matching requests.
    Setting(Setting),

    /// Value to return for matching requests.
    Value(Value),
}

impl Action {
    /// Return the canonical representation of this action
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        match self {
            Self::Setting(setting) => setting.canonical(source),
            Self::Value(value) => value.canonical(source),
        }
    }
}

/// A configuration setting
#[derive(Debug, Clone)]
pub struct Setting {
    /// The variable being set
    pub variable: Identifier,

    /// The value
    pub value: Value,
}

impl Setting {
    /// Return the canonical representation of this setting
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        format!(
            "{} = {}",
            self.variable.canonical(source),
            self.value.canonical(source)
        )
    }
}

/// A value for a configuration setting or to return as a response.
#[derive(Debug, Clone)]
pub enum Value {
    /// Call a function.
    Function(Identifier, Parameters),

    /// A string of some kind.
    Literal(Word),
}

impl Value {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self, source: &str) -> String {
        match self {
            Self::Function(identifier, parameters) => {
                format!(
                    "{}({})",
                    identifier.canonical(source),
                    parameters
                        .iter()
                        .map(|value| value.canonical(source))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::Literal(word) => word.canonical(source),
        }
    }
}

impl From<Word> for Value {
    fn from(word: Word) -> Self {
        Self::Literal(word)
    }
}

/// Parameters to a function call.
pub type Parameters = Vec<Value>;
