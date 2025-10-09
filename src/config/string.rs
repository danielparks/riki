//! Handle strings of various types in the configuration

use super::model::{ParseError, ParseResult, SpannedErrors, Word, WordType};
use logos::Logos;
use std::borrow::Cow;
use std::ops::Range;
use tinyvec::{ArrayVec, TinyVec};

/// A string that’s been parsed to expand escapes and for easy interpolation
#[expect(dead_code, reason = "wip")]
#[derive(Clone, Debug)]
pub struct ParsedString<'src> {
    /// The unescaped contents of the string.
    unescaped: Cow<'src, str>,

    /// Any variables to be interpolated.
    variables: TinyVec<[Interpolation<'src>; 1]>,

    /// The type of string.
    word_type: WordType,
}

impl<'src> ParsedString<'src> {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self) -> String {
        #[expect(clippy::match_same_arms, reason = "wip")]
        match self.word_type {
            WordType::Identifier => self.unescaped.to_string(),
            WordType::Path => {
                // FIXME escape
                self.unescaped.to_string()
            }
            WordType::BareGlob => {
                // FIXME escape
                self.unescaped.to_string()
            }
            WordType::QuotedSingle => {
                // FIXME escape
                format!(r"'{}'", self.unescaped)
            }
            WordType::QuotedDouble => {
                // FIXME escape
                format!(r#""{}""#, self.unescaped)
            }
        }
    }

    /// Create from a glob string
    #[must_use]
    pub const fn from_glob_str(src: &'src str) -> Self {
        // FIXME
        Self::from_literal(src, WordType::BareGlob)
    }

    /// Create from a final string.
    ///
    /// This will evaluate to exactly the contents of `src` without unescaping
    /// or interpolation.
    #[must_use]
    #[inline]
    pub const fn from_literal(src: &'src str, word_type: WordType) -> Self {
        Self {
            unescaped: Cow::Borrowed(src),
            variables: empty_tinyvec(),
            word_type,
        }
    }

    /// Parse string contents
    #[expect(clippy::allow_attributes, reason = "FIXME bug; expect fails")]
    #[allow(clippy::enum_glob_use, reason = "readability")]
    #[expect(clippy::arithmetic_side_effects, reason = "len < isize::MAX")]
    fn from_string_content(
        src: &'src str,
        word_type: WordType,
    ) -> ParseResult<'src, Self> {
        let lexer = StringToken::lexer(src);
        let mut out = String::new();
        let mut variables = TinyVec::default();
        let mut offset: isize = 0;
        let mut end_copied = 0;
        let mut errors = Vec::new();

        assert!(src.len() < isize::MAX as usize, "string too long");

        for (token, span) in lexer.spanned() {
            use StringToken::*;
            match token {
                Ok(BracketsVariable) => {
                    variables.push(Interpolation {
                        variable: &src[add(span.start, 2)..add(span.end, -1)],
                        range: add(span.start, offset)..add(span.end, offset),
                    });
                }
                Ok(Variable) => {
                    variables.push(Interpolation {
                        variable: &src[add(span.start, 1)..span.end],
                        range: add(span.start, offset)..add(span.end, offset),
                    });
                }
                Ok(BadDollar) => errors.push(
                    ParseError::StringBadDollar(word_type).spanned(&src[span]),
                ),
                Ok(TrailingEscape) => errors.push(
                    ParseError::StringTrailingBackslash(word_type)
                        .spanned(&src[span]),
                ),
                Ok(Content) => {}
                Ok(escape) => {
                    offset -= 1;
                    if end_copied == 0 {
                        out.reserve_exact(src.len() - 1);
                    }
                    out.push_str(&src[end_copied..span.start]);

                    match escape {
                        LiteralEscape => {
                            // Copy the escaped byte next time.
                            end_copied = span.end;
                        }
                        NewlineEscape => {
                            out.push('\n');
                            end_copied = span.end + 1;
                        }
                        CarriageReturnEscape => {
                            out.push('\r');
                            end_copied = span.end + 1;
                        }
                        TabEscape => {
                            out.push('\t');
                            end_copied = span.end + 1;
                        }
                        _ => unreachable!("nested matches"),
                    }
                }
                Err(()) => errors.push(
                    ParseError::StringUnknownError(word_type)
                        .spanned(&src[span]),
                ),
            }
        }

        if errors.is_empty() {
            let unescaped = if end_copied == 0 {
                Cow::Borrowed(src)
            } else {
                out.push_str(&src[end_copied..]);
                Cow::Owned(out)
            };

            Ok(Self { unescaped, variables, word_type })
        } else {
            Err(errors)
        }
    }
}

impl<'src> TryFrom<Word<'src>> for ParsedString<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(Word { type_, src }: Word<'src>) -> Result<Self, Self::Error> {
        // FIXME parse string interpolation here
        match type_ {
            WordType::Identifier => {
                // No escapes, no interpolation
                Ok(Self::from_literal(src, type_))
            }
            WordType::Path | WordType::BareGlob => {
                Self::from_string_content(src, type_)
            }
            WordType::QuotedSingle | WordType::QuotedDouble => {
                // Quoted... guarantee that they start and end with a quote byte
                Self::from_string_content(&src[1..add(src.len(), -1)], type_)
            }
        }
    }
}

/// Interpolated variable
///
/// This only implements `Default` for [`TinyVec`].
#[derive(Clone, Debug, Default)]
pub struct Interpolation<'src> {
    /// The name of the variable.
    pub variable: &'src str,

    /// The range in the string to replace.
    pub range: Range<usize>,
}

impl Interpolation<'_> {
    /// This should only be used to initialize `TinyVec`.
    const fn empty() -> Self {
        Self { variable: "", range: 0..0 }
    }
}

/// A token from lexing a string literal.
#[derive(Logos, Debug, PartialEq, Eq, Clone)]
#[logos(subpattern identifier_char = r"[a-zA-Z0-9_]")]
pub enum StringToken {
    /// Variable interpolation in brackets
    #[regex(r"\$\{(?&identifier_char)+\}")]
    BracketsVariable,

    /// Variable interpolation without brackets
    #[regex(r"\$(?&identifier_char)+")]
    Variable,

    /// Dollar not part of variable
    #[token("$")]
    BadDollar,

    /// Escape anything that passes through unchanged.
    #[regex(r"\\[^nrt]")]
    LiteralEscape,

    /// Newline escape
    #[token(r"\n")]
    NewlineEscape,

    /// Carriage return escape
    #[token(r"\r")]
    CarriageReturnEscape,

    /// Tab escape
    #[token(r"\t")]
    TabEscape,

    /// Newline escape
    ///
    /// Should only match at the very end of the string.
    #[token(r"\", priority = 10)]
    TrailingEscape,

    /// Other string content
    #[regex(r"[^$\\]")]
    Content,
}

/// Generate an empty [`TinyVec`] in `const`.
const fn empty_tinyvec<'a>() -> TinyVec<[Interpolation<'a>; 1]> {
    TinyVec::Inline(ArrayVec::from_array_empty([Interpolation::empty(); 1]))
}

/// Helper for `usize + isize`.
///
/// # Panics
///
/// Panics on overflow.
#[must_use]
#[inline]
const fn add(a: usize, b: isize) -> usize {
    a.checked_add_signed(b).expect("invalid offset")
}
