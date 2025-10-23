//! Perform actions defined in configuration file

use super::{
    Parameters, ParseError, ParseResult, ParsedString, PathValue,
    SpannedErrors, Value,
};

/// An action
#[derive(Clone, Debug)]
pub enum Action<'src> {
    /// Function call
    Function(Function<'src>),

    /// Literal (a path)
    Literal(PathValue<'src>),
}

impl Action<'_> {
    /// Return the canonical representation of this action
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.canonical(),
            Self::Literal(path) => path.canonical(),
        }
    }
}

impl<'src> TryFrom<Value<'src>> for Action<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(value: Value<'src>) -> Result<Self, Self::Error> {
        match value {
            Value::Function(function) => Ok(Self::Function(function)),
            Value::Literal(string) => Ok(Self::Literal(string.try_into()?)),
        }
    }
}

impl<'src> From<Action<'src>> for Value<'src> {
    fn from(action: Action<'src>) -> Self {
        match action {
            Action::Function(function) => Self::Function(function),
            Action::Literal(path_value) => Self::Literal(path_value.into()),
        }
    }
}

/// A function action
#[derive(Clone, Debug)]
pub enum Function<'src> {
    /// Return an error page with the passed status code
    Error(ParsedString<'src>),

    /// Return a literal as the body with a 200 code
    Literal(ParsedString<'src>),

    /// Render Markdown
    Markdown(Box<Action<'src>>),

    /// Render a page in a template
    Render(Box<Action<'src>>),
}

impl<'src> Function<'src> {
    /// Try to create a `Function` from the results of a parse.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if the source is invalid.
    pub fn from_parse(
        name: &'src str,
        parameters: Parameters<'src>,
        _rparen: Option<&'src str>,
    ) -> ParseResult<'src, Self> {
        let actual = parameters.len();
        let mut parameters = parameters.into_iter();
        match (name, parameters.next(), parameters.next()) {
            ("error", Some(value), None) => {
                Ok(Function::Error(value.try_into()?))
            }
            ("literal", Some(value), None) => {
                Ok(Self::Literal(value.try_into()?))
            }
            ("markdown", Some(value), None) => {
                Ok(Self::Markdown(Box::new(value.try_into()?)))
            }
            ("render", Some(value), None) => {
                Ok(Self::Render(Box::new(value.try_into()?)))
            }
            ("error" | "literal" | "markdown" | "render", ..) => {
                // FIXME wrong span
                Err(ParseError::WrongFunctionParameterCount {
                    name,
                    expected: 1,
                    actual,
                }
                .spanned_s(name))
            }
            (other, ..) => {
                Err(ParseError::UnknownFunctionName(other).spanned_s(other))
            }
        }
    }

    /// Get the function’s name
    #[must_use]
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Error(..) => "error",
            Self::Literal(..) => "literal",
            Self::Markdown(..) => "markdown",
            Self::Render(..) => "render",
        }
    }

    /// Return the canonical representation of this function call
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}({})",
            self.name(),
            match self {
                Self::Error(value) | Self::Literal(value) => value.canonical(),
                Self::Markdown(value) | Self::Render(value) =>
                    value.canonical(),
            },
        )
    }
}
