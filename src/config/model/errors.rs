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

    /// Unknown variable reference.
    #[error("found unknown variable {0:?}")]
    UnknownVariable(&'src str),
}

impl<'src> ParseError<'src> {
    /// Add a `src` to get a [`SpannedError`]
    #[must_use]
    #[inline]
    pub const fn spanned(self, src: &'src str) -> SpannedError<'src> {
        SpannedError { src, error: self }
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
    /// A slice of the full source indicating the source of the error.
    pub src: &'src str,

    /// The error.
    pub error: ParseError<'src>,
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
