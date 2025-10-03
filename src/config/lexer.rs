//! Configuration file lexer.

use codespan_reporting::diagnostic::Label;
use logos::Logos;

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

/// A token returned by the lexer.
#[derive(Logos, Debug, PartialEq, Eq, Copy, Clone)]
#[logos(error = LexerError)]
//#[logos(skip(r"#[^\n\r]*"))] // Comments
#[logos(subpattern comment = r"#[^\r\n]*")]
#[logos(subpattern continuation = r"\\\r?\n")] // '\' + '\n'
#[logos(subpattern hspace = r"[ \t]+")] // \t, ' '
#[logos(skip(r"(?&comment)"))] // For comment right before EOF
#[logos(skip(r"(?&hspace)"))]
#[logos(skip(r"(?&continuation)"))]
pub enum TokenType {
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

    /// Equals sign
    ///
    /// Appears between a variable and the value being set.
    #[token("=")]
    Equal,

    /// A glob, path, identifier, etc.
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
    /// Quotes are illegal in bare words without escaping, but they will raise
    /// an error.
    #[regex(r#"([-!$%&*+./0-9:;<>?@A-Z\[\]^_`a-z|~]|\\[[:^cntrl:]])([,'"]*([-!$%&*+./0-9:;<>?@A-Z\[\]^_`a-z|~{}]|\\[[:^cntrl:]]))*"#)]
    #[regex(r#"[{}]([,'"]*([-!$%&*+./0-9:;<>?@A-Z\[\]^_`a-z|~{}]|\\[[:^cntrl:]]))+"#)]
    BareWord,

    /// A double quoted string
    #[regex(r#""([\t\n\r[:^cntrl:]--"]|\\[\n\t[:^cntrl:]]|\\\r\n)*""#)]
    DoubleQuoted,

    /// A single quoted string
    #[regex(r"'([\t\n\r[:^cntrl:]--']|\\[\n\t[:^cntrl:]]|\\\r\n)*'")]
    SingleQuoted,

    /// A sequences of newlines
    #[regex(r"[\n\r]+")]
    Newlines,

    /// An error encountered by the lexer
    Error,
}

/// Tokenize
pub fn tokenize(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(TokenType, Span)> {
    let lexer = TokenType::lexer(source);
    let mut output = vec![];

    for (token, span) in lexer.spanned() {
        let token = token.unwrap_or_else(|err| {
            diagnostics.push(err.into_diagnostic(span.clone()));
            TokenType::Error
        });

        output.push((token, span));
    }
    output
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
/// There is also the special escape character, "%" [§2.1].
///
/// Glob characters (from [fast-glob]) already covered above: `?*[],!`
///
/// Glob characters not covered above: `{}\`
///
/// [RFC 3986 §2.2]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.2
/// [§2.3]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.3
/// [§2.1]: https://datatracker.ietf.org/doc/html/rfc3986/#section-2.1
/// [fast-glob]: https://crates.io/crates/fast-glob#syntax
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
            tokens
                .iter()
                .any(|(type_, _span)| *type_ == TokenType::Error)
                == !diagnostics.is_empty(),
            "Diagnostics should only be present iff there is an Error token",
        );

        tokens.iter().map(|(type_, _span)| *type_).collect()
    }

    #[test_log::test]
    fn comment() {
        check!(just_tokens("#comment").as_slice() == []);
    }

    #[test_log::test]
    fn newlines() {
        check!(just_tokens("\n").as_slice() == [Newlines]);
        check!(just_tokens("\n  ").as_slice() == [Newlines]);
        check!(just_tokens("\n  \n").as_slice() == [Newlines, Newlines]);
        check!(just_tokens("  \n").as_slice() == [Newlines]);
        check!(just_tokens("\n#comment").as_slice() == [Newlines]);
        check!(just_tokens("\n  #comment").as_slice() == [Newlines]);
        check!(
            just_tokens("\n  #comment\n  ").as_slice() == [Newlines, Newlines]
        );
    }

    #[test_log::test]
    fn bare_word() {
        check!(just_tokens("bare_word").as_slice() == [BareWord]);
        check!(just_tokens("*").as_slice() == [BareWord]);
        check!(just_tokens("?").as_slice() == [BareWord]);
        check!(just_tokens("/").as_slice() == [BareWord]);
        check!(just_tokens(".").as_slice() == [BareWord]);
        check!(just_tokens("bare_word").as_slice() == [BareWord]);
    }

    #[test_log::test]
    fn bare_word_escape() {
        check!(just_tokens(r"bare\ word").as_slice() == [BareWord]);
        check!(just_tokens(r"bare\\word").as_slice() == [BareWord]);
        check!(just_tokens(r#"\"abc\""#).as_slice() == [BareWord]);
        check!(just_tokens(r"\'abc\'").as_slice() == [BareWord]);
        check!(just_tokens(r#"\""#).as_slice() == [BareWord]);
        check!(just_tokens(r"\é").as_slice() == [BareWord]);
        check!(just_tokens("\\😀").as_slice() == [BareWord]);
    }

    #[test_log::test]
    fn globs() {
        check!(just_tokens(" {} ").as_slice() == [BareWord]);
        check!(just_tokens(" {ab} ").as_slice() == [BareWord]);
        check!(just_tokens(" {a,b} ").as_slice() == [BareWord]);
        check!(just_tokens("a{  ").as_slice() == [BareWord]);
        check!(just_tokens(" {a ").as_slice() == [BareWord]);
        check!(just_tokens(" a,a ").as_slice() == [BareWord]);
        check!(just_tokens(r" {\ ").as_slice() == [BareWord]);
    }

    #[test_log::test]
    fn settings() {
        check!(just_tokens("a=b").as_slice() == [BareWord, Equal, BareWord]);
        check!(just_tokens("a= b").as_slice() == [BareWord, Equal, BareWord]);
        check!(just_tokens("a = b").as_slice() == [BareWord, Equal, BareWord]);
        check!(just_tokens("a =b").as_slice() == [BareWord, Equal, BareWord]);
    }

    #[test_log::test]
    fn function() {
        check!(just_tokens("a()").as_slice() == [BareWord, LParen, RParen]);
        check!(
            just_tokens("a(b)").as_slice()
                == [BareWord, LParen, BareWord, RParen]
        );
        check!(
            just_tokens("a(b,b)").as_slice()
                == [BareWord, LParen, BareWord, RParen]
        );
        check!(
            just_tokens("a(b, c)").as_slice()
                == [BareWord, LParen, BareWord, Comma, BareWord, RParen]
        );
        check!(
            just_tokens("a ( b )").as_slice()
                == [BareWord, LParen, BareWord, RParen]
        );
        check!(
            just_tokens("a ( b , c ,d)").as_slice()
                == [
                    BareWord, LParen, BareWord, Comma, BareWord, Comma,
                    BareWord, RParen
                ]
        );
    }

    #[test_log::test]
    fn errors() {
        check!(just_tokens("\\\t").as_slice() == [Error]);
    }
}
