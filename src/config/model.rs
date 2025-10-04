//! The types that represent the actual configuration

use super::lexer::TokenType;
use globset::{Glob, GlobBuilder};
use std::slice;

/// Alias for a complete configuration
pub type Configuration<'src> = Vec<ConfigRule<'src>>;

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
