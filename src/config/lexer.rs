//! Configuration file lexer.

use codespan_reporting::diagnostic::Label;
use logos::{Lexer, Logos};

/// A diagnostic indicating an error or warning in the configuration file.
pub type Diagnostic = codespan_reporting::diagnostic::Diagnostic<()>;

/// A range of indices in the source
pub type Span = core::ops::Range<usize>;

/// An error encountered by the lexer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LexerError {
    /// An invalid token.
    #[default]
    Invalid,
    // TODO: add more errors if required
}

impl LexerError {
    /// Convert the error into a [`Diagnostic`].
    #[must_use]
    pub fn into_diagnostic(self, span: Span) -> Diagnostic {
        match self {
            Self::Invalid => Diagnostic::error()
                .with_message("invalid token")
                .with_label(Label::primary((), span)),
        }
    }
}

/// A token returned by the tokenizer.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum TokenType {
    /// End of file
    EOF,

    /// Left brace
    LBrace,

    /// Right brace
    RBrace,

    /// Left parenthesis
    LParen,

    /// Right parenthesis
    ///
    /// Only returned by the [`ParameterTokenType`] lexer.
    RParen,

    /// Comma
    ///
    /// Used as a separator in function parameter lists. Only returned by
    /// the [`ParameterTokenType`] lexer.
    Comma,

    /// Equals sign
    ///
    /// Appears between a variable and the value being set.
    Equal,

    /// An identifier (or a path matcher depending on context).
    Identifier,

    /// A path.
    Path,

    /// A glob or path literal.
    BareGlob,

    /// A double quoted string
    QuotedDouble,

    /// A single quoted string
    QuotedSingle,

    /// At least one newline
    Newline,

    /// An error encountered by the lexer
    Error,
}

/// A token returned by the outer lexer.
#[derive(Logos, Debug, PartialEq, Eq, Clone)]
#[logos(error = LexerError)]
#[logos(subpattern escape = r"\\[[:^cntrl:]]")]
#[logos(subpattern glob_start_char = r#"[^[:cntrl:] #()=\\{}"']|(?&escape)"#)]
#[logos(subpattern glob_ending_char = r"[^[:cntrl:] #()=\\]|(?&escape)")]
#[logos(subpattern identifier_char = r"[a-zA-Z0-9_]")]
#[logos(subpattern variable = r"\$(\{(?&identifier_char)+\}|(?&identifier_char)+)")]
#[logos(skip(r"#[^\r\n]*"))] // For comment right before EOF
#[logos(skip(r"[ \t]+"))]
#[logos(skip(r"\\\r?\n"))] // continuation: '\' + '\n'
pub enum OuterTokenType {
    /// End of file
    EOF,

    /// Left brace
    ///
    /// Starts a context block.
    #[token("{")]
    LBrace,

    /// Right brace
    ///
    /// Ends a context block.
    #[token("}")]
    RBrace,

    /// Left parenthesis
    ///
    /// Starts a function parameter list. Switches to the inner
    /// [`ParameterTokenType`] lexer.
    #[token("(", lex_parameters)]
    Parameters(Vec<(ParameterTokenType, Span)>),

    /// Right parenthesis
    ///
    /// Only returned by the [`ParameterTokenType`] lexer.
    RParen,

    /// Comma
    ///
    /// Used as a separator in function parameter lists. Only returned by
    /// the [`ParameterTokenType`] lexer.
    Comma,

    /// Equals sign
    ///
    /// Appears between a variable and the value being set.
    #[token("=")]
    Equal,

    /// An identifier (or a path matcher depending on context).
    #[regex(r"(?&identifier_char)+", priority = 20)]
    Identifier,

    /// A path.
    #[regex(r"([-%+./0-9:@A-Z_a-z|~]|(?&variable)|(?&escape))+", priority = 10)]
    Path,

    /// A glob or path literal.
    ///
    /// All ASCII printable are (mid-alphabet elided):
    ///
    /// ```text
    /// !"#$%&'()*+,-./0123456789:;<=>?@A...Z[\]^_`a...z{|}~
    /// ```
    ///
    /// Special characters:
    ///
    ///   * `' '`, `'\t'`, `'\n'` — word separators
    ///   * `'{'`, `'}'` — block start or end
    ///   * `'('`, `')'` — params start or end
    ///   * `','` — params separator
    ///   * `'='` — setting operator
    ///   * `'"'` — double-quoted word
    ///   * `'\''` — single-quoted word
    ///   * `'#'` — comment
    ///   * `'$'` — variable
    ///   * `'\\'` — escaping
    ///
    /// We want to be able to use `{abc,def}` syntax, so this requires that
    /// block-delimiting braces be surrounded by whitespace. Commas in
    /// parameters must be followed (or preceded) by whitespace.
    ///
    /// `'('`, `')'`, and `'='` are illegal in bare words without escaping; they
    /// will be be interpreted as their own token.
    ///
    /// Quotes are illegal in bare globs without escaping, but they will raise
    /// an error.
    #[regex(r#"(?&glob_start_char)(?&glob_ending_char)*"#)]
    #[regex(r#"[{}](?&glob_ending_char)+"#)]
    BareGlob,

    /// A double quoted string
    #[regex(r#""([\t\n\r[:^cntrl:]--"]|\\[\n\t[:^cntrl:]]|\\\r\n)*""#)]
    QuotedDouble,

    /// A single quoted string
    #[regex(r"'([\t\n\r[:^cntrl:]--']|\\[\n\t[:^cntrl:]]|\\\r\n)*'")]
    QuotedSingle,

    /// At least one newline
    #[regex(r"[\n\r]+")]
    Newline,

    /// An error encountered by the lexer
    Error,
}

/// A token returned by the inner parameter lexer.
#[derive(Logos, Debug, PartialEq, Eq, Copy, Clone)]
#[logos(error = LexerError)]
#[logos(subpattern escape = r"\\[[:^cntrl:]]")]
#[logos(subpattern identifier_char = r"[a-zA-Z0-9_]")]
#[logos(subpattern variable = r"\$(\{(?&identifier_char)+\}|(?&identifier_char)+)")]
#[logos(skip(r"#[^\r\n]*"))] // For comment right before EOF
#[logos(skip(r"[ \t\n]+"))] // Skip newlines too
#[logos(skip(r"\\\r?\n"))] // continuation: '\' + '\n'
pub enum ParameterTokenType {
    /// End of file
    EOF,

    /// Right parenthesis
    ///
    /// Starts a function parameter list.
    #[token("(")]
    LParen,

    /// Left parenthesis
    ///
    /// Ends a function parameter list.
    #[token(")")]
    RParen,

    /// Comma
    ///
    /// Separator in function parameter lists.
    #[token(",")]
    Comma,

    /// An identifier (or a path depending on context).
    #[regex(r"(?&identifier_char)+", priority = 20)]
    Identifier,

    /// A path.
    #[regex(r"([-%+./0-9:@A-Z_a-z|~]|(?&variable)|(?&escape))+", priority = 10)]
    Path,

    /// A double quoted string
    #[regex(r#""([\t\n\r[:^cntrl:]--"]|\\[\n\t[:^cntrl:]]|\\\r\n)*""#)]
    QuotedDouble,

    /// A single quoted string
    #[regex(r"'([\t\n\r[:^cntrl:]--']|\\[\n\t[:^cntrl:]]|\\\r\n)*'")]
    QuotedSingle,

    /// An error encountered by the lexer
    Error,
}

/// Switch to [`ParameterTokenType`] and lex a parameter list.
fn lex_parameters(
    outer_lexer: &mut Lexer<OuterTokenType>,
) -> Result<Vec<(ParameterTokenType, Span)>, LexerError> {
    let mut parameter_lexer =
        outer_lexer.clone().morph::<ParameterTokenType>().spanned();
    let mut tokens = Vec::new();
    let mut depth: usize = 1;
    for (type_, span) in parameter_lexer.by_ref() {
        let type_ = type_?;
        #[expect(clippy::arithmetic_side_effects, reason = "breaks on depth 0")]
        match &type_ {
            ParameterTokenType::LParen => {
                depth += 1;
            }
            ParameterTokenType::RParen => {
                depth -= 1;
            }
            _ => {}
        }
        tokens.push((type_, span));
        if depth == 0 {
            // Found outer ')'.
            break;
        }
    }
    *outer_lexer = <logos::Lexer<'_, ParameterTokenType> as Clone>::clone(
        &parameter_lexer,
    )
    .morph();
    Ok(tokens)
}

/// Tokenize
#[expect(clippy::allow_attributes, reason = "FIXME bug; expect fails")]
#[allow(clippy::enum_glob_use, reason = "readability")]
pub fn tokenize(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(TokenType, Span)> {
    let lexer = OuterTokenType::lexer(source);
    let mut out = Vec::new();

    for (token, span) in lexer.spanned() {
        use TokenType::*;
        match token {
            Ok(OuterTokenType::EOF) => out.push((EOF, span)),
            Ok(OuterTokenType::LBrace) => out.push((LBrace, span)),
            Ok(OuterTokenType::RBrace) => out.push((RBrace, span)),
            Ok(OuterTokenType::Parameters(items)) => {
                out.push((LParen, span));
                for (token, span) in items {
                    out.push((
                        match token {
                            ParameterTokenType::EOF => EOF,
                            ParameterTokenType::LParen => LParen,
                            ParameterTokenType::RParen => RParen,
                            ParameterTokenType::Comma => Comma,
                            ParameterTokenType::Identifier => Identifier,
                            ParameterTokenType::Path => Path,
                            ParameterTokenType::QuotedDouble => QuotedDouble,
                            ParameterTokenType::QuotedSingle => QuotedSingle,
                            ParameterTokenType::Error => Error,
                        },
                        span,
                    ));
                }
            }
            Ok(OuterTokenType::RParen) => out.push((RParen, span)),
            Ok(OuterTokenType::Comma) => out.push((Comma, span)),
            Ok(OuterTokenType::Equal) => out.push((Equal, span)),
            Ok(OuterTokenType::Identifier) => out.push((Identifier, span)),
            Ok(OuterTokenType::Path) => out.push((Path, span)),
            Ok(OuterTokenType::BareGlob) => out.push((BareGlob, span)),
            Ok(OuterTokenType::QuotedDouble) => out.push((QuotedDouble, span)),
            Ok(OuterTokenType::QuotedSingle) => out.push((QuotedSingle, span)),
            Ok(OuterTokenType::Newline) => out.push((Newline, span)),
            Ok(OuterTokenType::Error) => out.push((Error, span)),
            Err(error) => {
                diagnostics.push(error.into_diagnostic(span.clone()));
                out.push((Error, span));
            }
        }
    }
    out
}

/// Validate a glob matching a URL path
///
/// URL characters ([RFC 3986 §2.2] and [§2.3]):
///
/// ```ABNF
/// reserved    = gen-delims / sub-delims
///
/// gen-delims  = ":" / "/" / "?" / "#" / "[" / "]" / "@"
///
/// sub-delims  = "!" / "$" / "&" / "'" / "(" / ")"
///             / "*" / "+" / "," / ";" / "="
///
/// unreserved  = ALPHA / DIGIT / "-" / "." / "_" / "~"
/// ```
///
/// Note that "#" will never be passed to the server. We match it anyway
/// so that we can throw a good error.
///
/// There is also the special escape character, "%" ([§2.1]).
///
/// Glob characters (from [globset][globset#syntax]) already covered above:
/// `?*[],!`
///
/// Glob characters not covered above: `{}\`
///
/// [RFC 3986 §2.2]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.2
/// [§2.3]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.3
/// [§2.1]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.1
#[expect(dead_code, reason = "saving comment")]
const fn validate_glob() {}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenType::*;
    use assert2::check;

    fn just_tokens(input: &str) -> Vec<TokenType> {
        let mut diagnostics = Vec::new();
        let tokens = tokenize(input, &mut diagnostics);
        check!(
            tokens.iter().any(|(type_, _)| *type_ == TokenType::Error)
                == !diagnostics.is_empty(),
            "Diagnostics should only be present iff there is an Error token",
        );

        tokens.iter().map(|(type_, _)| *type_).collect()
    }

    #[test_log::test]
    fn comment() {
        check!(just_tokens("#comment").as_slice() == []);
    }

    #[test_log::test]
    fn newline() {
        check!(just_tokens("\n").as_slice() == [Newline]);
        check!(just_tokens("\n  ").as_slice() == [Newline]);
        check!(just_tokens("\n  \n").as_slice() == [Newline, Newline]);
        check!(just_tokens("  \n").as_slice() == [Newline]);
        check!(just_tokens("\n#comment").as_slice() == [Newline]);
        check!(just_tokens("\n  #comment").as_slice() == [Newline]);
        check!(
            just_tokens("\n  #comment\n  ").as_slice() == [Newline, Newline]
        );
    }

    #[test_log::test]
    fn identifier() {
        check!(just_tokens("a").as_slice() == [Identifier]);
        check!(just_tokens("a123").as_slice() == [Identifier]);
        check!(just_tokens("bare_word").as_slice() == [Identifier]);
    }

    #[test_log::test]
    fn bare_glob() {
        check!(just_tokens("*").as_slice() == [BareGlob]);
        check!(just_tokens("/*.foo[a-z]").as_slice() == [BareGlob]);
        check!(just_tokens("?").as_slice() == [BareGlob]);
    }

    #[test_log::test]
    fn path() {
        check!(just_tokens("/").as_slice() == [Path]);
        check!(just_tokens(".").as_slice() == [Path]);
        check!(just_tokens("abc/").as_slice() == [Path]);
        check!(just_tokens("/abc").as_slice() == [Path]);
        check!(just_tokens("/abc/").as_slice() == [Path]);
        check!(just_tokens("a/abc/c").as_slice() == [Path]);
    }

    #[test_log::test]
    fn path_escape() {
        check!(just_tokens(r"bare\ word").as_slice() == [Path]);
        check!(just_tokens(r"bare\\word").as_slice() == [Path]);
        check!(just_tokens(r#"\"abc\""#).as_slice() == [Path]);
        check!(just_tokens(r"\'abc\'").as_slice() == [Path]);
        check!(just_tokens(r#"\""#).as_slice() == [Path]);
        check!(just_tokens(r"\é").as_slice() == [Path]);
        check!(just_tokens(r"\😀").as_slice() == [Path]);
        check!(just_tokens(r"\*").as_slice() == [Path]);
    }

    #[test_log::test]
    fn bare_word_escape() {
        check!(just_tokens(r"bare\ word?").as_slice() == [BareGlob]);
        check!(just_tokens(r"bare\\word*").as_slice() == [BareGlob]);
        check!(just_tokens(r#"\"ab[e-x]c\""#).as_slice() == [BareGlob]);
        check!(just_tokens(r"\'ab{a,b}c\'").as_slice() == [BareGlob]);
        check!(just_tokens(r#"\"*"#).as_slice() == [BareGlob]);
        check!(just_tokens(r"\é*").as_slice() == [BareGlob]);
        check!(just_tokens("*\\😀").as_slice() == [BareGlob]);
    }

    #[test_log::test]
    fn globs() {
        check!(just_tokens(" {} ").as_slice() == [BareGlob]);
        check!(just_tokens(" {ab} ").as_slice() == [BareGlob]);
        check!(just_tokens(" {a,b} ").as_slice() == [BareGlob]);
        check!(just_tokens(" a{a,b} ").as_slice() == [BareGlob]);
        check!(just_tokens(" {a,b}b ").as_slice() == [BareGlob]);
        check!(just_tokens("a{  ").as_slice() == [BareGlob]);
        check!(just_tokens(" {a ").as_slice() == [BareGlob]);
        check!(just_tokens(" a,a ").as_slice() == [BareGlob]);
        check!(just_tokens(r" {\ ").as_slice() == [BareGlob]);
    }

    #[test_log::test]
    fn settings() {
        check!(
            just_tokens("a=b").as_slice() == [Identifier, Equal, Identifier]
        );
        check!(
            just_tokens("a= b").as_slice() == [Identifier, Equal, Identifier]
        );
        check!(
            just_tokens("a = b").as_slice() == [Identifier, Equal, Identifier]
        );
        check!(
            just_tokens("a =b").as_slice() == [Identifier, Equal, Identifier]
        );
    }

    #[test_log::test]
    fn function() {
        check!(just_tokens("a()").as_slice() == [Identifier, LParen, RParen]);
        check!(
            just_tokens("a(b)").as_slice()
                == [Identifier, LParen, Identifier, RParen]
        );
        check!(
            just_tokens("a(b,b)").as_slice()
                == [Identifier, LParen, Identifier, Comma, Identifier, RParen]
        );
        check!(
            just_tokens("a(b, c)").as_slice()
                == [Identifier, LParen, Identifier, Comma, Identifier, RParen]
        );
        check!(
            just_tokens("a ( b )").as_slice()
                == [Identifier, LParen, Identifier, RParen]
        );
        check!(
            just_tokens("a ( b , c ,d)").as_slice()
                == [
                    Identifier, LParen, Identifier, Comma, Identifier, Comma,
                    Identifier, RParen
                ]
        );
    }

    #[test_log::test]
    fn errors() {
        check!(just_tokens("\\\t").as_slice() == [Error]);
    }
}
