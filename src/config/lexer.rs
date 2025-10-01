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
#[logos(subpattern hspace = r"[ \t]+((?&continuation)[ \t]*)*|(?&continuation)+[ \t]+((?&continuation)+[\t ]*)*")] // \t, ' ', '\' + '\n'
#[logos(skip(r"(?&comment)"))] // For comment right before EOF
#[logos(skip(r"(?&hspace)"))]
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
    // FIXME xtended regex?
    #[regex(r#"[./0-9A-Z_a-z~*]"#)]
    #[regex(r#"([./0-9A-Z_a-z~*$%\[{]|\\[ -~])([./0-9A-Z_a-z~*$%&+,\-:;<>?@\[\]^{|}]|\\[ -~])+"#)]
    BareWord,

    /// A glob, a path, some other value
    #[regex(r#""([ !#-~]|\\[ -~])*""#)]
    DoubleQuoted,

    /// A path, some other value (not a glob)
    #[regex(r#"'([ -&(-~]|\\[ -~])*'"#)]
    SingleQuoted,

    /// A sequences of newlines
    ///
    /// Possibly containing comments and horizontal whitespace — this means
    /// there should never be more than one Newlines token in a row.
    #[regex(r"[\n\r]+(?&hspace)?((?&comment)?[\n\r]+(?&hspace)?)*")]
    Newlines,

    /// An error encountered by the lexer
    Error,
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
