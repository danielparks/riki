//! Objects created and used by parser2.

use super::super::actions;
use super::super::errors::{ParseError, ParseResult, SpannedErrors};
use super::super::lexer;
use super::super::model::{ConfigSettings, ParsedGlob, ParsedString};
use super::super::parser2::TokenType;
use globset::{Glob, GlobBuilder};
use std::fmt;
use std::slice;

/// The context stack for the parser.
///
/// Holds current settings and matcher for the extent of a new context.
#[derive(Clone, Debug, Default)]
pub struct ContextStack<'src> {
    /// The stack of matchers and settings.
    stack: Vec<(Matcher<'src>, ConfigSettings<'src>)>,

    /// Settings for the root context.
    root_settings: ConfigSettings<'src>,
}

impl<'src> ContextStack<'src> {
    /// Is the stack empty?
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get a clone of the current settings.
    #[must_use]
    pub fn settings_cloned(&self) -> ConfigSettings<'src> {
        self.stack
            .last()
            .map(|(_, settings)| settings.clone())
            .unwrap_or_else(|| self.root_settings.clone())
    }

    /// Get a mutable reference to the current settings.
    pub fn settings_mut(&mut self) -> &mut ConfigSettings<'src> {
        self.stack
            .last_mut()
            .map(|(_, settings)| settings)
            .unwrap_or(&mut self.root_settings)
    }

    /// Add a matcher to the stack.
    pub fn push(&mut self, matcher: Matcher<'src>) {
        self.stack.push((matcher, self.settings_cloned()));
    }

    /// Remove a matcher from the top of the stack.
    pub fn pop(&mut self) -> Option<Matcher<'src>> {
        self.stack.pop().map(|(matcher, _)| matcher)
    }

    /// Get the current matcher stack.
    #[must_use]
    pub fn matcher_stack(&self) -> MatcherStack<'src> {
        MatcherStack(
            self.stack
                .iter()
                .map(|(matcher, _)| matcher.clone())
                .collect(),
        )
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
                .map(ParsedGlob::from_glob_str)
                .map(Matcher)
                .collect(),
        )
    }

    /// Get the spans for each matcher
    #[must_use]
    pub fn spans(&self) -> Vec<&'src str> {
        self.0.iter().map(Matcher::span).collect()
    }

    /// Get the matcher stack as a glob
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    ///
    /// # Errors
    ///
    /// May pass through errors from [`GlobBuilder::build()`].
    pub fn as_glob(&self) -> ParseResult<'src, Glob> {
        GlobBuilder::new(&self.as_glob_str())
            .literal_separator(true)
            .backslash_escape(true)
            .empty_alternates(true)
            .build()
            .map_err(|error| {
                ParseError::BuildingGlob(error)
                    .with_spans(self.spans())
                    .into()
            })
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
    /// use riki::config::parser2::MatcherStack;
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
pub struct Matcher<'src>(ParsedGlob<'src>);

impl<'src> Matcher<'src> {
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

    /// Get the span of the original source for this matcher.
    #[must_use]
    pub const fn span(&self) -> &'src str {
        self.0.span()
    }
}

impl<'src> TryFrom<StringToken<'src>> for Matcher<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        // FIXME validate glob
        Ok(Matcher(token.try_into()?))
    }
}

/// A value for a configuration setting or to return as a response.
///
/// This is a temporary object that will be used to generate an
/// [`actions::Action`].
#[derive(Clone, Debug)]
pub enum Value<'src> {
    /// Call a function.
    Function(Box<Spanned<'src, actions::Function<'src>>>),

    /// A string of some kind.
    Literal(ParsedString<'src>),
}

impl<'src> Value<'src> {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.value.canonical(),
            Self::Literal(string) => string.canonical(),
        }
    }

    /// Get the span for the value.
    pub fn span(&self) -> Option<Span<'src>> {
        match self {
            Self::Function(function) => Some(function.span.clone()),
            Self::Literal(string) => string.span().map(Into::into),
        }
    }
}

impl<'src> TryFrom<StringToken<'src>> for Value<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        Ok(Self::Literal(token.try_into()?))
    }
}

/// Parameters to a function call.
pub type Parameters<'src> = Vec<Value<'src>>;

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

impl<'src> TryFrom<StringToken<'src>> for Identifier<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        match token.string_type {
            StringType::Identifier => Ok(Self(token.src)),
            other => Err(ParseError::ExpectedIdentifierToken(other.into())
                .spanned_s(token.src)),
        }
    }
}

/// The type of a string
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringType {
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

impl fmt::Display for StringType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identifier => "identifier",
            Self::Path => "path",
            Self::BareGlob => "glob",
            Self::QuotedSingle | Self::QuotedDouble => "string",
        })
    }
}

impl TryFrom<TokenType> for StringType {
    type Error = ParseError<'static>;

    fn try_from(value: TokenType) -> Result<Self, Self::Error> {
        match value {
            TokenType::Identifier => Ok(Self::Identifier),
            TokenType::Path => Ok(Self::Path),
            TokenType::BareGlob => Ok(Self::BareGlob),
            TokenType::QuotedSingle => Ok(Self::QuotedSingle),
            TokenType::QuotedDouble => Ok(Self::QuotedDouble),
            other => Err(ParseError::ExpectedStringToken(other)),
        }
    }
}

impl From<StringType> for TokenType {
    fn from(string_type: StringType) -> Self {
        match string_type {
            StringType::Identifier => Self::Identifier,
            StringType::Path => Self::Path,
            StringType::BareGlob => Self::BareGlob,
            StringType::QuotedSingle => Self::QuotedSingle,
            StringType::QuotedDouble => Self::QuotedDouble,
        }
    }
}

/// A reference to a string (string, identifier, path, or glob) in the source
#[derive(Clone, Debug)]
pub struct StringToken<'src> {
    /// The type of string
    pub string_type: StringType,

    /// The slice of the source representing this string
    pub src: &'src str,
}

/// A value representable by a string in the configuration file.
#[derive(Clone, Debug)]
pub struct Spanned<'src, T: Clone + fmt::Debug> {
    /// The value.
    pub value: T,

    /// The string in the configuration file.
    pub span: Span<'src>,
}

impl<'src, T: Clone + fmt::Debug> Spanned<'src, T> {
    /// Create a new [`Spanned`].
    pub const fn new(value: T, span: Span<'src>) -> Self {
        Self { value, span }
    }
}

/// A span of the source
#[derive(Clone, Debug)]
pub enum Span<'src> {
    /// A single slice of the source string.
    Slice(&'src str),
    /// From the beginning of the `start` slice to end of the `end` slice.
    SliceRange {
        /// The slice that starts the span.
        start: &'src str,
        /// The slice that ends the span (inclusive).
        end: &'src str,
    },
}

impl<'src> Span<'src> {
    /// Convert a [`Span`] into a [`lexer::Span`].
    ///
    /// # Panics
    ///
    /// Panics if any of the slices are not actually within `source`.
    #[must_use]
    pub fn to_lexer_span(&self, source: &'src str) -> lexer::Span {
        let source_start_ptr: usize = source.as_ptr() as usize;

        let start_ptr: usize = self.start_slice().as_ptr() as usize;
        let start_index = start_ptr
            .checked_sub(source_start_ptr)
            .expect("start slice not in source");
        assert!(start_index < source.len(), "start slice not in source");

        let end_ptr: usize = (self.end_slice().as_ptr() as usize)
            .checked_add(self.end_slice().len())
            .expect("end slice blows out memory");
        let end_index = end_ptr
            .checked_sub(source_start_ptr)
            .expect("end slice not in source");
        assert!(end_index <= source.len(), "end slice not in source");
        assert!(start_index <= end_index, "start slice not before end slice");

        start_index..end_index
    }

    /// Get a slice that start the span.
    #[must_use]
    #[inline]
    const fn start_slice(&self) -> &'src str {
        match self {
            Self::Slice(slice) => slice,
            Self::SliceRange { start, .. } => start,
        }
    }

    /// Get a slice that ends the span.
    #[must_use]
    #[inline]
    const fn end_slice(&self) -> &'src str {
        match self {
            Self::Slice(slice) => slice,
            Self::SliceRange { end, .. } => end,
        }
    }
}

impl<'src> From<&'src str> for Span<'src> {
    #[inline]
    fn from(slice: &'src str) -> Self {
        Self::Slice(slice)
    }
}

impl<'src, I> From<(&'src str, I)> for Span<'src>
where
    I: Into<Self>,
{
    #[inline]
    fn from((start, end): (&'src str, I)) -> Self {
        Self::SliceRange { start, end: end.into().end_slice() }
    }
}

impl<'src, I> From<(&'src str, Option<I>)> for Span<'src>
where
    I: Into<Self>,
{
    #[inline]
    fn from((start, end): (&'src str, Option<I>)) -> Self {
        match end {
            Some(end) => (start, end).into(),
            None => start.into(),
        }
    }
}
