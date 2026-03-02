//! Handle globs in the configuration

use super::super::errors::{ParseError, ParseResult, SpannedErrors};
use super::super::parser2::{StringToken, StringType};
use std::borrow::Cow;

/// A string that’s been parsed to expand escapes and for easy interpolation
#[derive(Clone, Debug)]
pub struct ParsedGlob<'src> {
    /// The unescaped contents of the glob.
    unescaped: Cow<'src, str>,
    /// The span in the source
    span: &'src str,
}

impl<'src> ParsedGlob<'src> {
    /// Return the canonical representation of this glob
    #[must_use]
    pub fn canonical(&self) -> String {
        // FIXME
        self.unescaped.to_string()
    }

    /// Get the unescaped glob string
    #[must_use]
    pub fn as_glob_str(&self) -> String {
        // FIXME
        self.unescaped.to_string()
    }

    /// Get the span of the original source for this glob.
    #[must_use]
    pub const fn span(&self) -> &'src str {
        self.span
    }

    /// Create from a glob string
    #[must_use]
    pub const fn from_glob_str(src: &'src str) -> Self {
        // FIXME
        Self { unescaped: Cow::Borrowed(src), span: src }
    }

    /// Parse glob string contents
    #[expect(clippy::unnecessary_wraps, reason = "FIXME")]
    const fn from_string_content(src: &'src str) -> ParseResult<'src, Self> {
        // FIXME
        Ok(Self { unescaped: Cow::Borrowed(src), span: src })
    }
}

impl<'src> TryFrom<StringToken<'src>> for ParsedGlob<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(
        StringToken { string_type, src }: StringToken<'src>,
    ) -> Result<Self, Self::Error> {
        // FIXME validate glob
        match string_type {
            StringType::BareGlob
            | StringType::Identifier
            | StringType::Path => Self::from_string_content(src),
            other => {
                Err(ParseError::ExpectedGlobToken(other.into()).spanned_s(src))
            }
        }
    }
}
