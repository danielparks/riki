//! Errors related to the second pass parsing.

use super::lexer::{Diagnostic, TokenType};
use super::parser2::{Source, Span, StringType};
use codespan_reporting::diagnostic::Label;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use std::io::Write;
use termcolor::StandardStream;

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

    /// Found [a function][crate::config::actions::Action::Function] instead of
    /// [a literal][crate::config::actions::Action::Literal].
    #[error("expected a string literal, got a function call")]
    ExpectedLiteralNotFunction,

    /// Error creating a glob.
    #[error("invalid matcher: {0}")]
    BuildingGlob(globset::Error),

    /// Error creating a globset.
    #[error("invalid matchers: {0}")]
    BuildingGlobSet(globset::Error),

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
        /// Name of the function.
        name: &'src str,
        /// Expected number of parameters.
        expected: usize,
        /// Actual number of parameters.
        actual: usize,
    },
}

impl<'src> ParseError<'src> {
    /// Add a `span` to get a [`SpannedError`].
    #[must_use]
    #[inline]
    pub fn spanned<S: Into<Span<'src>>>(self, span: S) -> SpannedError<'src> {
        SpannedError { spans: vec![span.into()], error: self }
    }

    /// Add `spans` to get a [`SpannedError`].
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

    /// Add a `span` and convert to [`SpannedErrors`].
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

/// A sequence of [`Diagnostic`]s with their [`Source`].
#[derive(Clone, Debug)]
pub struct Diagnostics<S: Source> {
    /// The diagnostics.
    pub diagnostics: Vec<Diagnostic>,

    /// The source.
    pub source: S,
}

impl<S: Source> Diagnostics<S> {
    /// Create [`Diagnostics`] from [`SpannedErrors`].
    pub fn from_errors(errors: SpannedErrors<'_>, source: S) -> Self {
        Self {
            diagnostics: errors
                .into_iter()
                .map(|error| error.into_diagnostic(&source))
                .collect(),
            source,
        }
    }

    /// Create [`Diagnostics`] from `Vec<Diagnostic>`.
    pub const fn from_diagnostics(
        diagnostics: Vec<Diagnostic>,
        source: S,
    ) -> Self {
        Self { diagnostics, source }
    }

    /// Output diagnostics and exit with code 1.
    pub fn check(&self, err_stream: &StandardStream) -> ! {
        self.print(err_stream);
        std::process::exit(1)
    }

    /// Print diagnostics found in configuration file.
    ///
    /// # Panics
    ///
    /// Panics if it can’t write to `err_stream` (probably stderr).
    pub fn print(&self, err_stream: &StandardStream) {
        let out = &mut err_stream.lock();
        let config = Config::default();
        let file = SimpleFile::new(
            self.source.name(),
            if let Some(content) = self.source.source() {
                content
            } else {
                writeln!(out, "Found errors in {}:", self.source.name())
                    .unwrap();
                ""
            },
        );

        for diag in &self.diagnostics {
            term::emit_to_write_style(out, &config, &file, diag).unwrap();
        }
    }
}

/// Unwrap a `Result<_, Diagnostics>`.
pub fn unwrap_diagnostics_result<T, S: Source>(
    result: Result<T, Diagnostics<S>>,
    err_stream: &StandardStream,
) -> T {
    match result {
        Ok(value) => value,
        Err(diagnostics) => diagnostics.check(err_stream),
    }
}

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
    pub fn into_diagnostic<S: Source + ?Sized>(self, source: &S) -> Diagnostic {
        let mut diagnostic = Diagnostic::error().with_message(self.error);
        if let Some(source) = source.source() {
            let mut iter =
                self.spans.iter().map(|span| span.to_lexer_span(source));
            if let Some(span) = iter.next() {
                diagnostic = diagnostic.with_label(Label::primary((), span));
                for span in iter {
                    diagnostic =
                        diagnostic.with_label(Label::secondary((), span));
                }
            }
        }
        diagnostic
    }

    /// Convert this into [`SpannedErrors`].
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
