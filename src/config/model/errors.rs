//! Errors related to the second pass parsing

use super::StringType;
use crate::config::lexer::{self, Diagnostic, TokenType};
use codespan_reporting::diagnostic::Label;

/// Errors that could be produced from parsing code.
///
/// Does not include lexer errors or errors sent to diagnostics.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ParseError<'src> {
    /// Found something other than a string-type token.
    #[error("expected am identifier, glob, path, or string token, got {0:?}")]
    ExpectedStringToken(TokenType),

    /// Found something other than an identifier token.
    #[error("expected an identifier token, got {0:?}")]
    ExpectedIdentifierToken(TokenType),

    /// Found something other than a glob token.
    #[error("expected a glob-compatible bare string, got {0:?}")]
    ExpectedGlobToken(TokenType),

    /// Found [`super::Value::Function`] instead of [`super::Value::Literal`].
    #[error("expected a string literal, got a function call")]
    ExpectedLiteralNotFunction,

    /// Error creating a glob
    #[error("invalid matcher: {0}")]
    BuildingGlob(globset::Error),

    /// Error creating a globset
    #[error("invalid matchers: {0}")]
    BuildingGlobSet(globset::Error),

    /// Path cannot start with variable
    #[error("path cannot start with variable; try prepending \"/\" or \"./\"")]
    PathStartsWithVariable,

    /// Setting with an invalid function value.
    #[error("setting {0:?} does not accept a function result as a value")]
    SettingDoesNotAcceptFunction(&'src str),

    /// Found a lonely backslash at the end of a string.
    #[error("found unescaped '\\' at end of {0}")]
    StringTrailingBackslash(StringType),

    /// Found a lonely dollar in a string.
    #[error("found unescaped '$' without variable in {0}")]
    StringBadDollar(StringType),

    /// Unknown error raised by lexer in string.
    ///
    /// This should never be returned.
    #[error("invalid character in {0}")]
    StringUnknownError(StringType),

    /// Unknown function reference.
    #[error("found unknown function {0:?}")]
    UnknownFunctionName(&'src str),

    /// Unknown setting name.
    #[error("found unknown setting name {0:?}")]
    UnknownSettingName(&'src str),

    /// Unknown variable reference.
    #[error("found unknown variable {0:?}")]
    UnknownVariable(&'src str),

    /// Wrong number of parameters in a function call.
    #[error(
        "found {actual} parameters in call to {name}(); expected {expected}"
    )]
    WrongFunctionParameterCount {
        /// Name of the function
        name: &'src str,
        /// Expected number of parameters
        expected: usize,
        /// Actual number of parameters
        actual: usize,
    },
}

impl<'src> ParseError<'src> {
    /// Add a `span` to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub fn spanned<S: Into<Span<'src>>>(self, span: S) -> SpannedError<'src> {
        SpannedError { spans: vec![span.into()], error: self }
    }

    /// Add `spans` to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub fn with_spans<S: Into<Span<'src>>>(
        self,
        spans: Vec<S>,
    ) -> SpannedError<'src> {
        SpannedError {
            spans: spans.into_iter().map(std::convert::Into::into).collect(),
            error: self,
        }
    }

    /// Get a [`SpannedError`] without spans.
    #[must_use]
    #[inline]
    pub const fn without_spans(self) -> SpannedError<'src> {
        SpannedError { spans: Vec::new(), error: self }
    }

    /// Add a `span` and convert to [`SpannedErrors`]
    #[must_use]
    #[inline]
    pub fn spanned_s<S: Into<Span<'src>>>(
        self,
        span: S,
    ) -> SpannedErrors<'src> {
        vec![self.spanned(span.into())]
    }
}

/// `Result` type for config parsing.
pub type ParseResult<'src, T, E = SpannedErrors<'src>> = Result<T, E>;

/// Possibly multiple [`SpannedError`]s.
pub type SpannedErrors<'src> = Vec<SpannedError<'src>>;

/// A [`ParseError`] along with its source.
#[derive(Clone, Debug)]
pub struct SpannedError<'src> {
    /// Spans of the full source indicating the source of the error.
    pub spans: Vec<Span<'src>>,

    /// The error.
    pub error: ParseError<'src>,
}

impl<'src> SpannedError<'src> {
    /// Convert the error into a [`Diagnostic`].
    ///
    /// This requires the original source that entries in
    /// [`SpannedError::spans`] reference.
    ///
    /// # Panics
    ///
    /// See [`Span::to_lexer_span()`].
    #[must_use]
    pub fn into_diagnostic(self, source: &str) -> Diagnostic {
        let mut diagnostic = Diagnostic::error().with_message(self.error);
        let mut iter = self.spans.iter().map(|span| span.to_lexer_span(source));
        if let Some(span) = iter.next() {
            diagnostic = diagnostic.with_label(Label::primary((), span));
            for span in iter {
                diagnostic = diagnostic.with_label(Label::secondary((), span));
            }
        }
        diagnostic
    }

    /// Convert this into [`SpannedErrors`]
    #[must_use]
    #[inline]
    pub fn plural(self) -> SpannedErrors<'src> {
        vec![self]
    }
}

impl<'src> From<SpannedError<'src>> for SpannedErrors<'src> {
    fn from(val: SpannedError<'src>) -> Self {
        val.plural()
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
