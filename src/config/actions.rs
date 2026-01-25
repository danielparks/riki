//! Perform actions defined in configuration file

use super::errors::{ParseError, ParseResult};
use super::model::{ParsedPath, ParsedString};
use super::parser2::{Parameters, Span, Value};
use crate::actions::{
    self, ContentReturn, Context, RealFileReturn, VariableMap,
};

/// An action
#[derive(Clone, Debug, derive_more::From)]
pub enum Action<'src> {
    /// Function call
    Function(Function<'src>),

    /// Literal (a path)
    Literal(ParsedPath<'src>),
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

    /// Evaluate the action for a request
    ///
    /// # Errors
    ///
    /// Returns [`actions::Error`] if there is a problem evaluating a function
    /// or opening a path.
    pub fn evaluate<'vars, V: VariableMap<'vars>>(
        &self,
        context: &'vars Context<'vars, V>,
    ) -> actions::Result {
        match self {
            Self::Function(function) => function.evaluate(context),
            Self::Literal(path) => Ok(RealFileReturn::from_url_path(
                path.content(&context.variables),
                context,
            )?
            .into()),
        }
    }
}

impl<'src> From<Value<'src>> for Action<'src> {
    fn from(value: Value<'src>) -> Self {
        match value {
            Value::Function(function) => Self::Function(function),
            Value::Literal(string) => Self::Literal(string.into()),
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
    ///
    /// ```text
    /// error(code: String) -> Response<Page>
    /// ```
    Error(ParsedString<'src>, Span<'src>),

    /// Return a literal as the body with a 200 code
    ///
    /// ```text
    /// literal(content: String) -> Response<Text>
    /// ```
    Literal(ParsedString<'src>, Span<'src>),

    /// Render Markdown
    ///
    /// ```text
    /// markdown(input: Path|Response) -> Response<Metadata, HTML>
    /// ```
    Markdown(Box<Action<'src>>, Span<'src>),

    /// Render a page in a template
    ///
    /// ```text
    /// render(input: Path|Response) -> Response<Metadata?, HTML>
    /// ```
    Render(Box<Action<'src>>, Span<'src>),
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
        rparen: Option<&'src str>,
    ) -> ParseResult<'src, Self> {
        let actual = parameters.len();
        let mut parameters = parameters.into_iter();
        let span = (name, rparen).into();
        match (name, parameters.next(), parameters.next()) {
            ("error", Some(value), None) => {
                Ok(Self::Error(value.try_into()?, span))
            }
            ("literal", Some(value), None) => {
                Ok(Self::Literal(value.try_into()?, span))
            }
            ("markdown", Some(value), None) => {
                Ok(Self::Markdown(Box::new(value.into()), span))
            }
            ("render", Some(value), None) => {
                Ok(Self::Render(Box::new(value.into()), span))
            }
            ("error" | "literal" | "markdown" | "render", ..) => {
                Err(ParseError::WrongFunctionParameterCount {
                    name,
                    expected: 1,
                    actual,
                }
                .spanned_s(span))
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
                Self::Error(value, _) | Self::Literal(value, _) =>
                    value.canonical(),
                Self::Markdown(value, _) | Self::Render(value, _) =>
                    value.canonical(),
            },
        )
    }

    /// Get the span for the entire function call.
    #[must_use]
    pub const fn span(&self) -> &Span<'src> {
        match self {
            Self::Error(_, span)
            | Self::Literal(_, span)
            | Self::Markdown(_, span)
            | Self::Render(_, span) => span,
        }
    }

    /// Evaluate the function for a request
    ///
    /// # Errors
    ///
    /// Returns [`actions::Error`] if there is a problem evaluating a function
    /// or opening a path.
    pub fn evaluate<'vars, V: VariableMap<'vars>>(
        &self,
        context: &'vars Context<'vars, V>,
    ) -> actions::Result {
        match self {
            Self::Error(error, _) => {
                let code = error.content(&context.variables);
                // FIXME better error response
                Ok(ContentReturn::plain_text(format!("Error {code}"))
                    .with_status(code.as_str().try_into().map_err(|_| {
                        actions::Error::InternalString(format!(
                            "Invalid status code {code:?}"
                        ))
                    })?)
                    .into())
            }
            Self::Literal(string, _) => Ok(ContentReturn::plain_text(
                string.content(&context.variables),
            )
            .into()),
            Self::Markdown(action, _) => {
                let ret = action.evaluate(context)?;
                // FIXME better error
                actions::markdown_to_html(context, ret)?.into()
            }
            Self::Render(action, _) => {
                let ret = action.evaluate(context)?;
                // FIXME better error
                actions::render(context, ret)?.into()
            }
        }
    }
}
