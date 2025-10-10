//! Handle globs in the configuration

use super::model::{
    ParseError, ParseResult, SpannedErrors, StringToken, StringType,
};
use std::borrow::Cow;

/// A string that’s been parsed to expand escapes and for easy interpolation
#[derive(Clone, Debug)]
pub struct GlobString<'src> {
    /// The unescaped contents of the glob.
    unescaped: Cow<'src, str>,
}

impl<'src> GlobString<'src> {
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

    /// Create from a glob string
    #[must_use]
    pub const fn from_glob_str(src: &'src str) -> Self {
        // FIXME
        Self { unescaped: Cow::Borrowed(src) }
    }

    /// Parse glob string contents
    #[expect(clippy::unnecessary_wraps, reason = "wip")]
    const fn from_string_content(src: &'src str) -> ParseResult<'src, Self> {
        // FIXME
        Ok(Self { unescaped: Cow::Borrowed(src) })
    }
}

impl<'src> TryFrom<StringToken<'src>> for GlobString<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(
        StringToken { string_type, src }: StringToken<'src>,
    ) -> Result<Self, Self::Error> {
        // FIXME validate glob
        match string_type {
            StringType::BareGlob
            | StringType::Identifier
            | StringType::Path => Self::from_string_content(src),
            other => Err(vec![
                (ParseError::ExpectedGlobToken(other.into()).spanned(src)),
            ]),
        }
    }
}
