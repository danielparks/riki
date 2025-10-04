//! Handle configuration files;
#![allow(clippy::too_many_lines, reason = "wip")]

pub mod lexer;
pub mod parser;
mod tests;

use bstr::{BStr, BString, ByteVec};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use globset::{Glob, GlobBuilder};
use lexer::{Diagnostic, TokenType, tokenize};
use parser::{CNode, CNodeIter, Cst, NodeRef, Parser, Rule, RuleSide};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::slice;
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
            println!("{}", rule.canonical());
        }
    }

    Ok(())
}

/// Parse configuration file contents and return rules or errors.
///
/// # Errors
///
/// Returns [`Diagnostic`]s that point out problems in `source`.
pub fn parse(source: &str) -> Result<Vec<ConfigRule<'_>>, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let cst = Parser::parse(source, &mut diagnostics);
    if diagnostics.is_empty() {
        Ok(process_cst(&cst))
    } else {
        Err(diagnostics)
    }
}

/// Process the CST from the parse into rules.
fn process_cst<'src>(cst: &Cst<'src>) -> Vec<ConfigRule<'src>> {
    let mut iter = cst.descendents(NodeRef::ROOT);
    let mut matcher_stack: MatcherStack = MatcherStack::empty();
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
fn consume_set_contents<'src>(iter: &mut CNodeIter<'_, 'src>) -> Setting<'src> {
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
fn consume_function_contents<'src>(
    iter: &mut CNodeIter<'_, 'src>,
) -> Value<'src> {
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
fn consume_value_contents<'src>(iter: &mut CNodeIter<'_, 'src>) -> Value<'src> {
    let word = consume_word_token(iter, " in value");
    expect_rule(iter, Rule::Value, RuleSide::Pop, " after value token");
    word.into()
}

/// Check that the next node is a matcher rule, then get the token it contains.
fn consume_matcher<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Word<'src> {
    expect_rule(iter, Rule::Matcher, RuleSide::Push, &context);
    let word = consume_word_token(iter, format!(" in matcher rule{context}"));
    expect_rule(iter, Rule::Matcher, RuleSide::Pop, " after matcher token");
    word
}

/// Consume a bare word token.
fn consume_identifier<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Identifier<'src> {
    consume_word_token(iter, &context)
        .try_into()
        .unwrap_or_else(|_| panic!("expected identifier token{context}"))
}

/// Check that the next node is a token and return it.
fn consume_word_token<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Word<'src> {
    let next = iter.next();
    let Some(CNode::Token(token, src)) = next else {
        panic!("expected word token{context}, got {next:?}");
    };

    Word { type_: token.try_into().unwrap(), src }
}

/// Check that the next node is a certain rule.
fn expect_rule<D: fmt::Display>(
    iter: &mut CNodeIter<'_, '_>,
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
fn expect_token<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    expected: TokenType,
    context: D,
) -> &'src str {
    let next = iter.next();
    let Some(CNode::Token(token, src)) = next else {
        panic!("expected token{context}, got {next:?}");
    };
    assert_eq!(
        expected, token,
        "expected {expected:?} token{context}, got {token:?}"
    );
    src
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
pub struct Word<'src> {
    /// The type of word
    pub type_: WordType,

    /// The slice of the source representing this word
    pub src: &'src str,
}

impl Word<'_> {
    /// Return the contents of this word
    ///
    /// FIXME variable interpolation
    #[must_use]
    pub fn contents(&self) -> String {
        match self.type_ {
            WordType::Identifier => self.src.to_owned(),
            WordType::Path => path_unescape(self.src),
            WordType::BareGlob => bare_glob_unescape(self.src),
            WordType::QuotedSingle | WordType::QuotedDouble => {
                string_unescape(self.src)
            }
        }
    }

    /// Return the contents of this word
    ///
    /// FIXME variable interpolation
    #[must_use]
    pub fn as_glob_str(&self) -> String {
        match self.type_ {
            WordType::Identifier => globset::escape(self.src),
            WordType::Path => globset::escape(&path_unescape(self.src)),
            WordType::BareGlob => bare_glob_unescape(self.src),
            WordType::QuotedSingle | WordType::QuotedDouble => {
                globset::escape(&string_unescape(self.src))
            }
        }
    }

    /// Return the canonical representation of this word
    #[must_use]
    pub fn canonical(&self) -> String {
        self.src.to_owned()
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
pub struct Identifier<'src>(pub &'src str);

impl Identifier<'_> {
    /// Return the canonical representation of this identifier
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.to_owned()
    }
}

impl<'src> TryFrom<Word<'src>> for Identifier<'src> {
    type Error = ParseError;

    fn try_from(word: Word<'src>) -> Result<Self, Self::Error> {
        match word.type_ {
            WordType::Identifier => Ok(Self(word.src)),
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
#[derive(Clone, Debug)]
pub struct ConfigRule<'src> {
    /// Match a request
    pub matcher: MatcherStack<'src>,
    /// Action to take in response to a request
    pub action: Action<'src>,
}

impl ConfigRule<'_> {
    /// Return the canonical representation of this rule
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} {}", self.matcher.canonical(), self.action.canonical())
    }
}

/// A stack of matchers for a request.
#[derive(Clone, Debug, Default)]
pub struct MatcherStack<'src>(pub Vec<Matcher<'src>>);

impl<'src> MatcherStack<'src> {
    /// Get a new, empty matcher stack
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Add a matcher to the stack
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Add a matcher to the stack
    pub fn push(&mut self, matcher: Matcher<'src>) {
        self.0.push(matcher);
    }

    /// Remove a matcher from the top of the stack
    pub fn pop(&mut self) -> Option<Matcher<'src>> {
        self.0.pop()
    }

    /// Get the matcher stack as a glob
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    ///
    /// # Errors
    ///
    /// May pass through errors from [`GlobBuilder::build()`].
    pub fn as_glob(&self) -> Result<Glob, globset::Error> {
        GlobBuilder::new(&self.as_glob_str())
            .literal_separator(true)
            .backslash_escape(true)
            .empty_alternates(true)
            .build()
    }

    /// Get the matcher stack as a glob string
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    #[must_use]
    #[inline]
    pub fn as_glob_str(&self) -> String {
        //    / foobar /hey -> /**/foobar/hey{,/**}
        //    / / /hey -> /hey{,/**}
        //    / abc / /hey -> /**/abc/hey{,/**}
        //    / abc def /hey -> /**/abc/**/def/hey{,/**}
        let mut full_glob = "/".to_owned(); // FIXME capacity?
        for matcher in &self.0 {
            let glob_str = matcher.as_glob_str();
            #[expect(clippy::manual_strip, reason = "clarity, simplicity")]
            if glob_str.starts_with('/') {
                if full_glob.ends_with('/') {
                    full_glob.push_str(&glob_str[1..]);
                } else {
                    full_glob.push_str(&glob_str);
                }
            } else {
                // Doesn’t start with /
                full_glob.push_str(if full_glob.ends_with('/') {
                    "**/"
                } else {
                    "/**/"
                });
                full_glob.push_str(&glob_str);
            }
        }
        full_glob
    }

    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self) -> String {
        self.as_glob_str()
    }

    /// Return an iterator over the matchers
    pub fn iter(&self) -> slice::Iter<'_, Matcher<'src>> {
        self.0.iter()
    }
}

impl<'a, 'src> IntoIterator for &'a MatcherStack<'src>
where
    'src: 'a,
{
    type Item = &'a Matcher<'src>;
    type IntoIter = slice::Iter<'a, Matcher<'src>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A matcher for a request.
#[derive(Clone, Debug)]
pub struct Matcher<'src>(pub Word<'src>);

impl Matcher<'_> {
    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.canonical()
    }

    /// Get the matcher stack as a glob
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    #[must_use]
    pub fn as_glob_str(&self) -> String {
        self.0.canonical()
    }
}

/// The action corresponding to a rule.
#[derive(Clone, Debug)]
pub enum Action<'src> {
    /// Set an option for matching requests.
    Setting(Setting<'src>),

    /// Value to return for matching requests.
    Value(Value<'src>),
}

impl Action<'_> {
    /// Return the canonical representation of this action
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Setting(setting) => setting.canonical(),
            Self::Value(value) => value.canonical(),
        }
    }
}

/// A configuration setting
#[derive(Clone, Debug)]
pub struct Setting<'src> {
    /// The variable being set
    pub variable: Identifier<'src>,

    /// The value
    pub value: Value<'src>,
}

impl Setting<'_> {
    /// Return the canonical representation of this setting
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} = {}", self.variable.canonical(), self.value.canonical())
    }
}

/// A value for a configuration setting or to return as a response.
#[derive(Clone, Debug)]
pub enum Value<'src> {
    /// Call a function.
    Function(Identifier<'src>, Parameters<'src>),

    /// A string of some kind.
    Literal(Word<'src>),
}

impl Value<'_> {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(identifier, parameters) => {
                format!(
                    "{}({})",
                    identifier.canonical(),
                    parameters
                        .iter()
                        .map(Value::canonical)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::Literal(word) => word.canonical(),
        }
    }
}

impl<'src> From<Word<'src>> for Value<'src> {
    fn from(word: Word<'src>) -> Self {
        Self::Literal(word)
    }
}

/// Parameters to a function call.
pub type Parameters<'src> = Vec<Value<'src>>;
