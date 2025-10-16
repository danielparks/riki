//! Errors related to the second pass parsing

use super::StringType;
use crate::config::lexer::{Diagnostic, Span, TokenType};
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

    /// Error creating a glob
    #[error("invalid matcher: {0}")]
    BuildingGlob(globset::Error),

    /// Error creating a globset
    #[error("invalid matchers: {0}")]
    BuildingGlobSet(globset::Error),

    /// Found a function call with more than 255 parameters.
    #[error("invalid function call (more than 255 parameters)")]
    TooManyParameters,

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
        expected: u8,
        /// Actual number of parameters
        actual: u8,
    },
}

impl<'src> ParseError<'src> {
    /// Add a `src` to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub fn spanned(self, src: &'src str) -> SpannedError<'src> {
        SpannedError { srcs: vec![src], error: self }
    }

    /// Add `src`s to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub const fn with_spans(self, srcs: Vec<&'src str>) -> SpannedError<'src> {
        SpannedError { srcs, error: self }
    }

    /// Add a `src` and convert to [`SpannedErrors`]
    #[must_use]
    #[inline]
    pub fn spanned_s(self, src: &'src str) -> SpannedErrors<'src> {
        vec![self.spanned(src)]
    }
}

/// A [`ParseError`] along with its source.
#[derive(Clone, Debug)]
pub struct SpannedError<'src> {
    /// Slices of the full source indicating the source of the error.
    pub srcs: Vec<&'src str>,

    /// The error.
    pub error: ParseError<'src>,
}

impl<'src> SpannedError<'src> {
    /// Convert the error into a [`Diagnostic`].
    ///
    /// This requires the original source that entries in [`SpannedError::srcs`]
    /// reference.
    ///
    /// # Panics
    ///
    /// See [`slice_to_span()`].
    #[must_use]
    pub fn into_diagnostic(self, source: &str) -> Diagnostic {
        let mut diagnostic = Diagnostic::error().with_message(self.error);
        let mut iter = self.srcs.iter().map(|src| slice_to_span(src, source));
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

/// Possibly multiple [`SpannedError`]s.
pub type SpannedErrors<'src> = Vec<SpannedError<'src>>;

/// `Result` type for config parsing.
pub type ParseResult<'src, T, E = SpannedErrors<'src>> = Result<T, E>;

/// Convert a slice of the source into a [`Span`].
///
/// # Panics
///
/// Panics if `slice` is not actually within `source`.
#[must_use]
pub fn slice_to_span(slice: &str, source: &str) -> Span {
    let src_start: usize = slice.as_ptr() as usize;
    let source_start: usize = source.as_ptr() as usize;

    let start = src_start
        .checked_sub(source_start)
        .expect("slice not in source");
    assert!(start < source.len(), "slice not in source");

    let end = start.checked_add(slice.len()).expect("slice not in source");
    assert!(end <= source.len(), "slice not in source");

    start..end
}
