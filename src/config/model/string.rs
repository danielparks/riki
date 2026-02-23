//! Handle strings of various types in the configuration

use crate::actions::{Variable, VariableMap};
use crate::config::errors::{ParseError, ParseResult, SpannedErrors};
use crate::config::parser2::{StringToken, StringType};
use crate::misc::bitfilter::BitFilter;
use logos::Logos;
use std::borrow::Cow;
use std::ops::Range;
use std::path::PathBuf;
use std::slice;
use tinyvec::{ArrayVec, TinyVec};

/// A string that’s been parsed to expand escapes and for easy interpolation
///
/// This can be used as a regular string, or a path. There are a few special
/// methods for paths ([`path_content()`][Self::path_content()],
/// [`push_path()`][Self::push_path()], and [`join_path()`][Self::join_path()]).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedString<'src> {
    /// The unescaped contents of the string.
    unescaped: Cow<'src, str>,

    /// Any variables to be interpolated.
    variables: TinyVec<[Interpolation<'src>; 1]>,
}

impl<'src> ParsedString<'src> {
    /// Get span in the source, if available.
    #[must_use]
    pub const fn span(&self) -> Option<&'src str> {
        if let Cow::Borrowed(src) = self.unescaped {
            Some(src)
        } else {
            None
        }
    }

    /// Return the content of this string.
    pub fn content<'a, V: VariableMap<'a>>(&self, variables: &'a V) -> String {
        self.split_on_variables()
            .map(|part| match part {
                StringPart::Fixed(fixed) => Cow::Borrowed(fixed),
                StringPart::Variable(var) => variables.get(var.variable),
            })
            .collect()
    }

    /// Append a character to the end of the string.
    pub fn push(&mut self, c: char) {
        self.unescaped.to_mut().push(c);
    }

    /// Append a string to the end of the string.
    pub fn push_str(&mut self, s: &str) {
        self.unescaped.to_mut().push_str(s);
    }

    /// Append a string to the end of the string.
    ///
    /// # Panics
    ///
    /// Panics if the final combined string would be more than `usize::MAX`
    /// bytes long.
    pub fn push_string(&mut self, other: &Self) {
        self.variables.extend(other.variables.iter().map(|var| {
            Interpolation::<'src> {
                variable: var.variable,
                span: var.span,
                range: Range {
                    start: var
                        .range
                        .start
                        .checked_add(self.unescaped.len())
                        .unwrap(),
                    end: var
                        .range
                        .end
                        .checked_add(self.unescaped.len())
                        .unwrap(),
                },
            }
        }));
        self.unescaped.to_mut().push_str(&other.unescaped);
    }

    /// Return the content while treating this as a path.
    ///
    /// If this starts with a variable then leading `'/'`s will be removed.
    #[inline]
    pub fn path_content<'vars, V: VariableMap<'vars>>(
        &self,
        variables: &'vars V,
    ) -> String {
        let path = self.content(variables);
        if self.starts_with_variable() {
            // FIXME handle Windows absolute/root paths
            let new_path = path.trim_start_matches('/');
            if new_path == path {
                path
            } else {
                new_path.to_owned()
            }
        } else {
            path
        }
    }

    /// Join another path onto this one.
    ///
    /// If `other` starts with `'/'`, then it will replace `self`. If `other`
    /// starts with a variable, then we consider it a relative path.
    pub fn push_path(&mut self, other: &Self) {
        if other.starts_with('/') == Some(true) {
            other.clone_into(self);
        } else if self.ends_with('/') == Some(true) {
            self.push_string(other);
        } else {
            // A double or triple / doesn’t matter.
            self.push('/');
            self.push_string(other);
        }
    }

    /// Join two paths together.
    #[must_use]
    pub fn join_path(&self, other: &Self) -> Self {
        // If other is absolute, and we moved other, this could just return it.
        let mut new = self.clone();
        new.push_path(other);
        new
    }

    /// Does this start with a variable?
    #[must_use]
    pub fn starts_with_variable(&self) -> bool {
        self.variables
            .first()
            .is_some_and(|var| var.range.start == 0)
    }

    /// Does this end with a variable?
    #[must_use]
    pub fn ends_with_variable(&self) -> bool {
        self.variables
            .last()
            .is_some_and(|var| var.range.end >= self.unescaped.len())
    }

    /// Does this start with `c`?
    ///
    /// Returns `None` if this starts with a variable, and thus the final start
    /// is unknown.
    #[must_use]
    pub fn starts_with(&self, c: char) -> Option<bool> {
        if self.starts_with_variable() {
            None
        } else {
            Some(self.unescaped.starts_with(c))
        }
    }

    /// Does this end with `c`?
    ///
    /// Returns `None` if this ends with a variable, and thus the final ending
    /// is unknown.
    #[must_use]
    pub fn ends_with(&self, c: char) -> Option<bool> {
        if self.ends_with_variable() {
            None
        } else {
            Some(self.unescaped.ends_with(c))
        }
    }

    /// Get the variables used in the string.
    #[must_use]
    pub fn variables(&self) -> &[Interpolation<'src>] {
        &self.variables
    }

    /// Split on interpolated variables
    ///
    /// The iterator yields `(&'src str, Option<&'_ Interpolation<'src>>)`.
    #[must_use]
    pub fn split_on_variables(&self) -> VariableSplitIter<'_, 'src> {
        VariableSplitIter {
            source: &self.unescaped,
            iter: self.variables.iter(),
            current: None,
            last_end: 0,
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
        out.extend(self.split_on_variables().map(StringPart::canonical));
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

    /// Parse string contents.
    ///
    /// # Errors
    ///
    /// May return [`ParseError`].
    ///
    /// # Panics
    ///
    /// Panics if the string length is greater than or equal to [`isize::MAX`].
    #[expect(clippy::allow_attributes, reason = "rust-clippy issue #13358")]
    #[allow(clippy::enum_glob_use, reason = "readability")]
    #[expect(clippy::arithmetic_side_effects, reason = "len < isize::MAX")]
    pub fn from_string_content(
        src: &'src str,
        string_type: StringType,
    ) -> ParseResult<'src, Self> {
        let lexer = StringLexToken::lexer(src);
        let mut out = String::new();
        let mut variables = TinyVec::default();
        let mut offset: isize = 0;
        let mut next_index_to_copy = 0;
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
                    if next_index_to_copy == 0 {
                        out.reserve_exact(src.len() - 1);
                    }
                    out.push_str(&src[next_index_to_copy..span.start]);

                    match escape {
                        LiteralEscape => {
                            // Copy the escaped byte next time.
                            next_index_to_copy = span.start + 1;
                        }
                        NewlineEscape => {
                            out.push('\n');
                            next_index_to_copy = span.end;
                        }
                        CarriageReturnEscape => {
                            out.push('\r');
                            next_index_to_copy = span.end;
                        }
                        TabEscape => {
                            out.push('\t');
                            next_index_to_copy = span.end;
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
            let unescaped = if next_index_to_copy == 0 {
                Cow::Borrowed(src)
            } else {
                out.push_str(&src[next_index_to_copy..]);
                Cow::Owned(out)
            };

            Ok(Self { unescaped, variables })
        } else {
            Err(errors)
        }
    }
}

impl<'src> From<&'src str> for ParsedString<'src> {
    fn from(src: &'src str) -> Self {
        Self { unescaped: Cow::Borrowed(src), variables: empty_tinyvec() }
    }
}

impl From<String> for ParsedString<'_> {
    fn from(src: String) -> Self {
        Self { unescaped: Cow::Owned(src), variables: empty_tinyvec() }
    }
}

#[expect(clippy::fallible_impl_from, reason = "wip")]
impl From<PathBuf> for ParsedString<'_> {
    fn from(value: PathBuf) -> Self {
        // FIXME this should support non-UTF-8 rather than panicking.
        value.to_str().unwrap().to_owned().into()
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
    iter: slice::Iter<'cow, Interpolation<'src>>,
    /// Current variable.
    current: Option<&'cow Interpolation<'src>>,
    /// The end of the last variable.
    last_end: usize,
}

impl<'cow, 'src> Iterator for VariableSplitIter<'cow, 'src> {
    type Item = StringPart<'cow, 'src>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.take() {
            // Already outputted fixed prefix
            Some(StringPart::Variable(current))
        } else if let Some(next) = self.iter.next() {
            let fixed = &self.source[self.last_end..next.range.start];
            self.last_end = next.range.end;
            self.current = Some(next);
            Some(StringPart::Fixed(fixed))
        } else if self.last_end < self.source.len() {
            let fixed = &self.source[self.last_end..];
            self.last_end = self.source.len();
            Some(StringPart::Fixed(fixed))
        } else {
            None
        }
    }
}

/// Part of a string returned by [`ParsedString::split_on_variables()`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringPart<'cow, 'src> {
    /// A fixed string
    Fixed(&'cow str),
    /// An interpolated variable
    Variable(&'cow Interpolation<'src>),
}

impl StringPart<'_, '_> {
    /// Canonical representation of this part of the string.
    #[must_use]
    pub fn canonical(self) -> String {
        match self {
            // FIXME return Cow?
            Self::Fixed(fixed) => escape_quoted(fixed, b'"').to_string(),
            Self::Variable(var) => format!("${{{}}}", var.variable),
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Interpolation<'src> {
    /// The variable.
    pub variable: Variable,

    /// The span of the source representing the variable name.
    pub span: &'src str,

    /// The range in the string to replace.
    range: Range<usize>,
}

impl Interpolation<'_> {
    /// This should only be used to initialize `TinyVec`.
    const fn empty() -> Self {
        Self { variable: Variable::Host, span: "", range: 0..0 }
    }
}

impl Default for Interpolation<'_> {
    /// This should only be used to initialize `TinyVec`.
    fn default() -> Self {
        Self::empty()
    }
}

impl<'src> TryFrom<(&'src str, Range<usize>)> for Interpolation<'src> {
    type Error = ParseError<'src>;

    fn try_from(
        (name, range): (&'src str, Range<usize>),
    ) -> Result<Self, Self::Error> {
        let variable = name
            .try_into()
            .map_err(|_| ParseError::UnknownVariable(name))?;
        Ok(Interpolation { variable, span: name, range })
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
    use crate::actions::StaticVariables;
    use assert2::{check, let_assert};

    /// Create a [`ParsedString`] easily.
    fn parse_str(s: &str) -> ParseResult<'_, ParsedString<'_>> {
        ParsedString::from_string_content(s, StringType::QuotedDouble)
    }

    /// Test variables
    const VARS: StaticVariables = StaticVariables {
        request_path: "/abc/",
        verb: "GET",
        host: "example.com",
    };

    #[test_log::test]
    fn parse_string_literal_escape() {
        let_assert!(Ok(s) = parse_str(r"\z"));
        check!(s.content(&VARS) == "z");

        let_assert!(Ok(s) = parse_str(r"a\zb"));
        check!(s.content(&VARS) == "azb");
    }

    #[test_log::test]
    fn parse_string_newline_escape() {
        let_assert!(Ok(s) = parse_str(r"\n"));
        check!(s.content(&VARS) == "\n");

        let_assert!(Ok(s) = parse_str(r"a\nb"));
        check!(s.content(&VARS) == "a\nb");
    }

    #[test_log::test]
    fn parse_string_just_var_no_braces() {
        let_assert!(Ok(s) = parse_str("$clean_path"));
        check!(s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_just_var_braces() {
        let_assert!(Ok(s) = parse_str("${clean_path}"));
        check!(s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_no_braces() {
        let_assert!(Ok(s) = parse_str("$clean_path/foo"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('o') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_braces() {
        let_assert!(Ok(s) = parse_str("${clean_path}bar"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('r') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_no_braces_c() {
        let_assert!(Ok(s) = parse_str("$clean_path/"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(true));
        check!(s.ends_with('o') == Some(false));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_braces_c() {
        let_assert!(Ok(s) = parse_str("${clean_path}/"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(true));
        check!(s.ends_with('o') == Some(false));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_no_braces_escape() {
        let_assert!(Ok(s) = parse_str(r"$clean_path\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_no_braces_escape2() {
        let_assert!(Ok(s) = parse_str(r"$clean_path\z\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_braces_escape() {
        let_assert!(Ok(s) = parse_str(r"${clean_path}\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_start_var_braces_escape2() {
        let_assert!(Ok(s) = parse_str(r"${clean_path}\z\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_end_var_no_braces() {
        let_assert!(Ok(s) = parse_str("/foo/$clean_path"));
        check!(!s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == Some(true));
        check!(s.starts_with('o') == Some(false));
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_string_end_var_braces() {
        let_assert!(Ok(s) = parse_str("/foo/${clean_path}"));
        check!(!s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == Some(true));
        check!(s.starts_with('o') == Some(false));
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn push_c() {
        let mut s = parse_str("/foo/${clean_path}").unwrap();
        s.push('Z');
        check!(s.ends_with('Z') == Some(true));
    }

    #[test_log::test]
    fn push_string_no_vars() {
        let mut a = parse_str("abc").unwrap();
        let b = parse_str("def").unwrap();
        a.push_string(&b);
        check!(a.content(&VARS) == "abcdef");
        check!(a.variables().is_empty());
    }

    #[test_log::test]
    fn push_string_both_vars() {
        let mut a = parse_str("/foo/${clean_path}").unwrap();
        let b = parse_str("${clean_path}/foo").unwrap();
        a.push_string(&b);
        check!(a.canonical() == r#""/foo/${clean_path}${clean_path}/foo""#);
        check!(
            a.variables()
                == [
                    Interpolation {
                        variable: Variable::CleanPath,
                        span: "clean_path",
                        range: 5..18
                    },
                    Interpolation {
                        variable: Variable::CleanPath,
                        span: "clean_path",
                        range: 18..31
                    }
                ]
        );
    }

    #[test_log::test]
    fn push_string_b_var() {
        let mut a = parse_str("/foo/").unwrap();
        let b = parse_str("${clean_path}/foo").unwrap();
        a.push_string(&b);
        check!(a.canonical() == r#""/foo/${clean_path}/foo""#);
        check!(
            a.variables()
                == [Interpolation {
                    variable: Variable::CleanPath,
                    span: "clean_path",
                    range: 5..18
                }]
        );
    }

    #[test_log::test]
    fn string_content() {
        let string = parse_str("/foo/").unwrap();
        check!(string.content(&VARS) == "/foo/");
    }

    #[test_log::test]
    fn string_content_1_var() {
        let string = parse_str("/foo/${clean_path}/").unwrap();
        check!(string.content(&VARS) == "/foo//abc/");
    }

    #[test_log::test]
    fn string_content_2_vars() {
        let string = parse_str("/foo/$verb${clean_path}").unwrap();
        check!(string.content(&VARS) == "/foo/GET/abc");
    }

    #[test_log::test]
    fn string_content_3_vars() {
        let string = parse_str("${host}/$verb/$clean_path").unwrap();
        check!(string.content(&VARS) == "example.com/GET//abc");
    }

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

    // Path handling tests

    #[test_log::test]
    fn push_path_no_vars() {
        let mut a = parse_str("abc").unwrap();
        a.push_path(&parse_str("def").unwrap());
        check!(a.path_content(&VARS) == PathBuf::from("abc/def"));
        check!(a.variables().is_empty());
    }

    #[test_log::test]
    fn push_path_both_vars() {
        let mut a = parse_str("/foo/${clean_path}").unwrap();
        a.push_path(&parse_str("${clean_path}/foo").unwrap());
        check!(a.canonical() == r#""/foo/${clean_path}/${clean_path}/foo""#);
        check!(
            a.variables().iter().map(|i| i.variable).collect::<Vec<_>>()
                == [Variable::CleanPath, Variable::CleanPath]
        );
    }

    #[test_log::test]
    fn push_path_b_var() {
        let mut a = parse_str("/foo/").unwrap();
        a.push_path(&parse_str("${clean_path}/foo").unwrap());
        check!(a.canonical() == r#""/foo/${clean_path}/foo""#);
        check!(
            a.variables().iter().map(|i| i.variable).collect::<Vec<_>>()
                == [Variable::CleanPath]
        );
    }

    #[test_log::test]
    fn push_path_absolute() {
        let mut a = parse_str("/foo/").unwrap();
        a.push_path(&parse_str("/bar").unwrap());
        check!(a.canonical() == r#""/bar""#);
        check!(a.variables().is_empty());
    }

    #[test_log::test]
    fn path_content() {
        let path = parse_str("/foo/").unwrap();
        check!(path.path_content(&VARS) == PathBuf::from("/foo/"));
    }

    #[test_log::test]
    fn path_content_1_var() {
        let string = parse_str("/foo/${clean_path}/").unwrap();
        check!(string.path_content(&VARS) == PathBuf::from("/foo//abc/"));
    }

    #[test_log::test]
    fn path_content_2_vars() {
        let string = parse_str("/foo/$verb${clean_path}").unwrap();
        check!(string.path_content(&VARS) == PathBuf::from("/foo/GET/abc"));
    }

    #[test_log::test]
    fn path_content_3_vars() {
        let string = parse_str("${host}/$verb/$clean_path").unwrap();
        check!(
            string.path_content(&VARS) == PathBuf::from("example.com/GET//abc")
        );
    }

    #[test_log::test]
    fn path_content_start_path_var() {
        let string = parse_str("$clean_path/foo").unwrap();
        check!(string.path_content(&VARS) == PathBuf::from("abc/foo"));
    }
}
