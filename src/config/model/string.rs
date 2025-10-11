//! Handle strings of various types in the configuration

use super::{ParseError, ParseResult, SpannedErrors, StringToken, StringType};
use crate::config::bitfilter::BitFilter;
use crate::config::is_valid_variable;
use logos::Logos;
use std::borrow::Cow;
use std::ops::Range;
use std::slice;
use tinyvec::{ArrayVec, TinyVec};

/// A string that’s been parsed to expand escapes and for easy interpolation
#[derive(Clone, Debug)]
pub struct ParsedString<'src> {
    /// The unescaped contents of the string.
    unescaped: Cow<'src, str>,

    /// Any variables to be interpolated.
    variables: TinyVec<[Interpolation<'src>; 1]>,
}

impl<'src> ParsedString<'src> {
    /// Return the content of this string
    #[must_use]
    pub fn content(&self) -> String {
        // FIXME variables
        self.unescaped.to_string()
    }

    /// Split on interpolated variables
    ///
    /// The iterator yields `(&'src str, Option<&'_ Interpolation<'src>>)`.
    #[must_use]
    pub fn split_on_variables(&self) -> VariableSplitIter<'_, 'src> {
        VariableSplitIter {
            source: &self.unescaped,
            variable_iter: self.variables.iter(),
            last_variable_end: 0,
        }
    }

    /// Return the canonical representation of this value
    ///
    /// # Panics
    ///
    /// Panics if the escaped string would have more than `usize::MAX` bytes.
    #[must_use]
    pub fn canonical<'a>(&'a self) -> String
    where
        'src: 'a,
    {
        let mut out = String::with_capacity(add(self.unescaped.len(), 2));
        out.push('"');
        // FIXME: be smart check for quote characters or something
        out.extend(self.split_on_variables().map(|(fixed, var)| match var {
            Some(var) => {
                format!("{}${{{}}}", escape_quoted(fixed, b'"'), var.variable)
            }
            None => escape_quoted(fixed, b'"').to_string(),
        }));
        out.push('"');
        out
    }

    /// Create from a final string.
    ///
    /// This will evaluate to exactly the contents of `src` without unescaping
    /// or interpolation.
    #[must_use]
    #[inline]
    pub const fn from_literal(src: &'src str) -> Self {
        Self { unescaped: Cow::Borrowed(src), variables: empty_tinyvec() }
    }

    /// Parse string contents
    #[expect(clippy::allow_attributes, reason = "FIXME bug; expect fails")]
    #[allow(clippy::enum_glob_use, reason = "readability")]
    #[expect(clippy::arithmetic_side_effects, reason = "len < isize::MAX")]
    fn from_string_content(
        src: &'src str,
        string_type: StringType,
    ) -> ParseResult<'src, Self> {
        let lexer = StringLexToken::lexer(src);
        let mut out = String::new();
        let mut variables = TinyVec::default();
        let mut offset: isize = 0;
        let mut end_copied = 0;
        let mut errors = Vec::new();

        assert!(src.len() < isize::MAX as usize, "string too long");

        for (token, span) in lexer.spanned() {
            use StringLexToken::*;
            match token {
                Ok(var_type @ (BracketsVariable | Variable)) => {
                    let name_range = match var_type {
                        BracketsVariable => {
                            add(span.start, 2)..add(span.end, -1)
                        }
                        Variable => add(span.start, 1)..span.end,
                        _ => unreachable!("nested matches"),
                    };
                    let var_range =
                        add(span.start, offset)..add(span.end, offset);

                    match Interpolation::try_from((
                        &src[name_range],
                        var_range.clone(),
                    )) {
                        Ok(interpolation) => variables.push(interpolation),
                        Err(error) => {
                            errors.push(error.spanned(&src[var_range]));
                        }
                    }
                }
                Ok(BadDollar) => errors.push(
                    ParseError::StringBadDollar(string_type)
                        .spanned(&src[span]),
                ),
                Ok(TrailingEscape) => errors.push(
                    ParseError::StringTrailingBackslash(string_type)
                        .spanned(&src[span]),
                ),
                Ok(Content) => {}
                Ok(
                    escape @ (LiteralEscape | NewlineEscape
                    | CarriageReturnEscape | TabEscape),
                ) => {
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
                    ParseError::StringUnknownError(string_type)
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

            Ok(Self { unescaped, variables })
        } else {
            Err(errors)
        }
    }
}

impl<'src> TryFrom<StringToken<'src>> for ParsedString<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(
        StringToken { string_type, src }: StringToken<'src>,
    ) -> Result<Self, Self::Error> {
        match string_type {
            StringType::Identifier => {
                // No escapes, no interpolation
                Ok(Self::from_literal(src))
            }
            StringType::Path | StringType::BareGlob => {
                Self::from_string_content(src, string_type)
            }
            StringType::QuotedSingle | StringType::QuotedDouble => {
                // Quoted... guarantee that they start and end with a quote byte
                Self::from_string_content(
                    &src[1..add(src.len(), -1)],
                    string_type,
                )
            }
        }
    }
}

/// An iterator over pairs of fixed slices and variable interpolations.
pub struct VariableSplitIter<'cow, 'src> {
    /// The source string.
    source: &'cow Cow<'src, str>,
    /// The variable interpolations in the source string.
    variable_iter: slice::Iter<'cow, Interpolation<'src>>,
    /// The end of the last variable.
    last_variable_end: usize,
}

impl<'cow, 'src> Iterator for VariableSplitIter<'cow, 'src> {
    type Item = (&'cow str, Option<&'cow Interpolation<'src>>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.variable_iter.next() {
            Some(var) => {
                let value =
                    &self.source[self.last_variable_end..var.range.start];
                self.last_variable_end = var.range.end;
                Some((value, Some(var)))
            }
            None if self.last_variable_end < self.source.len() => {
                let value = &self.source[self.last_variable_end..];
                self.last_variable_end = self.source.len();
                Some((value, None))
            }
            None => None,
        }
    }
}

/// Escape inside of quoted string.
///
/// Invalid characters are control characters (`[\x00-\x1F\x7F]`) including tab
/// and newline, backslash, dollar sign, and the quote character.
///
/// # Panics
///
/// Panics if a non-ASCII byte is passed as `quote`.
#[expect(clippy::arithmetic_side_effects, reason = "checks first")]
#[must_use]
#[inline]
pub fn escape_quoted(input: &str, quote: u8) -> Cow<'_, str> {
    let mut invalid_filter = BitFilter::from_bytes(b"\x00-\x1F\x7F\\$");
    invalid_filter.add(quote).unwrap();

    let invalid: Vec<_> = input
        .bytes()
        .enumerate()
        .filter(|(_, b)| invalid_filter.match_byte(*b))
        .collect();
    if invalid.is_empty() {
        Cow::Borrowed(input)
    } else {
        let mut out = String::with_capacity(input.len() + invalid.len());
        let mut offset = 0;
        let mut rest = input;
        for (i, b) in invalid {
            let prefix;
            (prefix, rest) = rest.split_at(i - offset);
            out.push_str(prefix);
            out.push('\\');
            out.push(match b {
                b'\n' => 'n',
                b'\r' => 'r',
                b'\t' => 't',
                b => b as char,
            });
            // Skip the first byte of rest (already handled as b)
            rest = &rest[1..];
            offset = i + 1;
        }
        out.push_str(rest);
        Cow::Owned(out)
    }
}

/// Interpolated variable
///
/// This only implements `Default` for [`TinyVec`].
#[derive(Clone, Debug, Default)]
pub struct Interpolation<'src> {
    /// The name of the variable.
    variable: &'src str,

    /// The range in the string to replace.
    range: Range<usize>,
}

impl Interpolation<'_> {
    /// This should only be used to initialize `TinyVec`.
    const fn empty() -> Self {
        Self { variable: "", range: 0..0 }
    }
}

impl<'src> TryFrom<(&'src str, Range<usize>)> for Interpolation<'src> {
    type Error = ParseError<'src>;

    fn try_from(
        (variable, range): (&'src str, Range<usize>),
    ) -> Result<Self, Self::Error> {
        if is_valid_variable(variable) {
            Ok(Interpolation { variable, range })
        } else {
            Err(ParseError::UnknownVariable(variable))
        }
    }
}

/// A token from lexing a string literal.
#[derive(Logos, Debug, PartialEq, Eq, Clone)]
#[logos(subpattern identifier_char = r"[a-zA-Z0-9_]")]
pub enum StringLexToken {
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[test_log::test]
    fn escape_quote_simple() {
        for &quote in b"'\"" {
            check!(escape_quoted("", quote) == "");
            check!(escape_quoted("/", quote) == "/");
            check!(escape_quoted("/abc/def", quote) == "/abc/def");
            check!(escape_quoted("/abc/def$part", quote) == r"/abc/def\$part");
            check!(escape_quoted(r"\backslash", quote) == r"\\backslash");
        }
    }

    #[test_log::test]
    fn escape_quote_quotes() {
        check!(escape_quoted("single'double\"", b'\'') == r#"single\'double""#);
        check!(escape_quoted("single'double\"", b'"') == r#"single'double\""#);
    }

    #[test_log::test]
    fn escape_quote_newline() {
        check!(escape_quoted("/a b\nc\td", b'\'') == r"/a b\nc\td");
    }
}
