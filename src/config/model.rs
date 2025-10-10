//! The types that represent the actual configuration

use super::glob::GlobString;
use super::lexer::{Diagnostic, Span, TokenType};
use super::string::ParsedString;
use codespan_reporting::diagnostic::Label;
use globset::{Glob, GlobBuilder};
use std::fmt;
use std::slice;

/// Alias for a complete configuration
pub type Configuration<'src> = Vec<ConfigRule<'src>>;

/// The type of a word
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordType {
    /// Identifier
    Identifier,
    /// Bare path
    Path,
    /// Bare glob
    BareGlob,
    /// Single quoted string
    QuotedSingle,
    /// Double quoted string
    QuotedDouble,
}

impl fmt::Display for WordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identifier => "identifier",
            Self::Path => "path",
            Self::BareGlob => "glob",
            Self::QuotedSingle | Self::QuotedDouble => "string",
        })
    }
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

/// A reference to a word (string, identifier, path, or glob) in the config file
#[derive(Clone, Debug)]
pub struct Word<'src> {
    /// The type of word
    pub type_: WordType,

    /// The slice of the source representing this word
    pub src: &'src str,
}

impl Word<'_> {
    /// Return the canonical representation of this word
    #[must_use]
    pub fn canonical(&self) -> String {
        self.src.to_owned()
    }
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
    type Error = SpannedErrors<'src>;

    fn try_from(word: Word<'src>) -> Result<Self, Self::Error> {
        match word.type_ {
            WordType::Identifier => Ok(Self(word.src)),
            other => Err(vec![SpannedError {
                src: word.src,
                error: ParseError::ExpectedIdentifierToken(other.into()),
            }]),
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

    /// Create a matcher stack from a slice of bare glob strings
    #[must_use]
    pub fn from_glob_strs<I: IntoIterator<Item = &'src str>>(globs: I) -> Self {
        Self(
            globs
                .into_iter()
                .map(GlobString::from_glob_str)
                .map(Matcher)
                .collect(),
        )
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
    ///
    ///   * FIXME: allow other conditions; evaluate them.
    ///   * FIXME: Treat `"*", "*"` specially?
    ///
    /// ```
    /// use assert2::check;
    /// use riki::config::model::MatcherStack;
    ///
    /// fn combine<'a, I: IntoIterator<Item=&'a str>>(globs: I) -> String {
    ///     MatcherStack::from_glob_strs(globs).as_glob_str()
    /// }
    ///
    /// check!(combine([]) == "/**");
    /// check!(combine(["/"]) == "/**");
    /// check!(combine(["/", "/"]) == "/**");
    /// check!(combine(["/", "foobar", "/zz/"]) == "/**/foobar/zz/**");
    /// check!(combine(["/", "/", "/zz"]) == "/zz{,/**}");
    /// check!(combine(["abc", "/", "/zz"]) == "/**/abc/zz{,/**}");
    /// check!(combine(["abc", "def", "/zz"]) == "/**/abc/**/def/zz{,/**}");
    ///
    /// check!(combine(["*"]) == "/**/*{,/**}");
    /// check!(combine(["**/foo"]) == "/**/foo{,/**}");
    /// check!(combine(["**", "**", "foo"]) == "/**/**/foo{,/**}");
    /// check!(combine(["**", "a"]) == "/**/a{,/**}");
    /// check!(combine(["**", "/a"]) == "/**/a{,/**}");
    /// check!(combine(["**/", "a"]) == "/**/**/a{,/**}");
    /// check!(combine(["**/", "/a"]) == "/**/a{,/**}");
    /// check!(combine(["**/**", "foobar"]) == "/**/**/foobar{,/**}");
    /// check!(combine(["/", "[abc]?", "/zz"]) == "/**/[abc]?/zz{,/**}");
    /// check!(combine(["abc", "/{foo,**}"]) == "/**/abc/{foo,**}{,/**}");
    /// ```
    #[must_use]
    #[inline]
    pub fn as_glob_str(&self) -> String {
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
            } else if glob_str.starts_with("**/") || glob_str == "**" {
                if !full_glob.ends_with('/') {
                    full_glob.push('/');
                }
                full_glob.push_str(&glob_str);
            } else {
                // Doesn’t start with / or **/
                full_glob.push_str(if full_glob.ends_with("/**") {
                    "/"
                } else if full_glob.ends_with('/') {
                    "**/"
                } else {
                    "/**/"
                });
                full_glob.push_str(&glob_str);
            }
        }

        // Match a path prefix, not just and exact path.
        if full_glob.ends_with('/') {
            full_glob.push_str("**");
        } else if !full_glob.ends_with("/**") {
            // It doesn’t actually matter if the full glob ends with `/**`,
            // since `{,/**}` can match nothing.
            full_glob.push_str("{,/**}");
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
pub struct Matcher<'src>(GlobString<'src>);

impl Matcher<'_> {
    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.canonical()
    }

    /// Get the matcher as a glob string
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    #[must_use]
    pub fn as_glob_str(&self) -> String {
        self.0.as_glob_str()
    }
}

impl<'src> TryFrom<Word<'src>> for Matcher<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(word: Word<'src>) -> Result<Self, Self::Error> {
        // FIXME validate glob
        Ok(Matcher(word.try_into()?))
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
    name: Identifier<'src>,

    /// The value
    value: Value<'src>,
}

impl Setting<'_> {
    /// Return the canonical representation of this setting
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} = {}", self.name.canonical(), self.value.canonical())
    }
}

impl<'src> TryFrom<(Identifier<'src>, Value<'src>)> for Setting<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(
        (name, value): (Identifier<'src>, Value<'src>),
    ) -> Result<Self, Self::Error> {
        // FIXME validate setting
        Ok(Self { name, value })
    }
}

/// A value for a configuration setting or to return as a response.
#[derive(Clone, Debug)]
pub enum Value<'src> {
    /// Call a function.
    Function(FunctionCall<'src>),

    /// A string of some kind.
    Literal(ParsedString<'src>),
}

impl Value<'_> {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.canonical(),
            Self::Literal(string) => string.canonical(),
        }
    }
}

impl<'src> TryFrom<Word<'src>> for Value<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(word: Word<'src>) -> Result<Self, Self::Error> {
        // FIXME parse string interpolation here
        Ok(Self::Literal(word.try_into()?))
    }
}

/// A function call
#[derive(Clone, Debug)]
pub struct FunctionCall<'src> {
    /// The function name
    name: Identifier<'src>,

    /// The parameters
    parameters: Parameters<'src>,
}

impl FunctionCall<'_> {
    /// Return the canonical representation of this function call
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}({})",
            self.name.canonical(),
            self.parameters
                .iter()
                .map(Value::canonical)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl<'src> TryFrom<(Identifier<'src>, Parameters<'src>)>
    for FunctionCall<'src>
{
    type Error = SpannedErrors<'src>;

    fn try_from(
        (name, parameters): (Identifier<'src>, Parameters<'src>),
    ) -> Result<Self, Self::Error> {
        // FIXME validate function call
        Ok(Self { name, parameters })
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

    /// Found something other than a glob token.
    #[error("expected a glob-compatible bare word, got {0:?}")]
    ExpectedGlobToken(TokenType),

    /// Found a lonely backslash at the end of a string.
    #[error("found unescaped '\\' at end of {0}")]
    StringTrailingBackslash(WordType),

    /// Found a lonely dollar in a string.
    #[error("found unescaped '$' without variable in {0}")]
    StringBadDollar(WordType),

    /// Unknown error raised by lexer in string.
    ///
    /// This should never be returned.
    #[error("invalid character in {0}")]
    StringUnknownError(WordType),
}

impl ParseError {
    /// Add a `src` to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub const fn spanned(self, src: &str) -> SpannedError<'_> {
        SpannedError { src, error: self }
    }

    /// Add a `src` and convert to [`SpannedErrors`]
    #[must_use]
    #[inline]
    pub fn spanned_s(self, src: &str) -> SpannedErrors<'_> {
        vec![self.spanned(src)]
    }
}

/// A [`ParseError`] along with its source.
#[derive(Clone, Debug)]
pub struct SpannedError<'src> {
    /// A slice of the full source indicating the source of the error.
    pub src: &'src str,

    /// The error.
    pub error: ParseError,
}

impl SpannedError<'_> {
    /// Convert `src` into a [`Span`].
    ///
    /// # Panics
    ///
    /// Panics if `src` is not actually within `source`.
    #[must_use]
    pub fn span(&self, source: &str) -> Span {
        let src_start: usize = self.src.as_ptr() as usize;
        let source_start: usize = source.as_ptr() as usize;

        let start = src_start
            .checked_sub(source_start)
            .expect("span not in source");
        assert!(start < source.len(), "span not in source");

        let end = start
            .checked_add(self.src.len())
            .expect("span not in source");
        assert!(end <= source.len(), "span not in source");

        start..end
    }

    /// Convert the error into a [`Diagnostic`].
    ///
    /// This requires the original source that [`SpannedError::src`] references.
    ///
    /// # Panics
    ///
    /// See [`Self::span()`].
    #[must_use]
    pub fn into_diagnostic(self, source: &str) -> Diagnostic {
        let span = self.span(source);
        Diagnostic::error()
            .with_message(self.error)
            .with_label(Label::primary((), span))
    }
}

/// Possibly multiple [`SpannedError`]s.
pub type SpannedErrors<'src> = Vec<SpannedError<'src>>;

/// `Result` type for config parsing.
pub type ParseResult<'src, T, E = SpannedErrors<'src>> = Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn into_glob<'a, I: IntoIterator<Item = &'a str>>(
        globs: I,
    ) -> Result<Glob, globset::Error> {
        let stack = MatcherStack::from_glob_strs(globs);
        println!("glob str: {}", stack.as_glob_str());
        stack.as_glob()
    }

    #[test_log::test]
    fn matcher_stack_paths_makes_valid_globs() {
        check!(into_glob([]).is_ok());
        check!(into_glob(["/"]).is_ok());
        check!(into_glob(["/", "/"]).is_ok());
        check!(into_glob(["/", "foobar", "/zz/"]).is_ok());
        check!(into_glob(["/", "/", "/zz"]).is_ok());
        check!(into_glob(["abc", "/", "/zz"]).is_ok());
        check!(into_glob(["abc", "def", "/zz"]).is_ok());
    }

    #[test_log::test]
    fn matcher_stack_globs_makes_valid_globs() {
        check!(into_glob(["*"]).is_ok());
        check!(into_glob(["**/foo"]).is_ok());
        check!(into_glob(["**/**", "foobar"]).is_ok());
        check!(into_glob(["/", "[abc]?", "/zz"]).is_ok());
    }

    #[test_log::test]
    fn match_file_glob() {
        let matcher = into_glob(["/foo/bar.md"]).unwrap().compile_matcher();
        check!(matcher.is_match("/foo/bar.md"));
        check!(matcher.is_match("/foo/bar.md/"));
        check!(matcher.is_match("/foo/bar.md/test"));
    }

    #[test_log::test]
    fn match_dir_glob() {
        let matcher = into_glob(["/foo/"]).unwrap().compile_matcher();
        check!(!matcher.is_match("/foo"));
        check!(matcher.is_match("/foo/"));
        check!(matcher.is_match("/foo/bar.md/test"));
    }
}
