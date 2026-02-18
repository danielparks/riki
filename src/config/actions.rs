//! Perform actions defined in configuration file

use super::errors::{ParseError, ParseResult};
use super::model::ParsedPath;
use super::parser2::{Parameters, Spanned, Value};
use crate::actions::{
    self, ContentReturn, Context, RealFileReturn, Return, VariableMap,
};
use pastey::paste;

/// An action
#[derive(Clone, Debug, derive_more::From)]
pub enum Action<'src> {
    /// Function call
    Function(Box<Spanned<'src, Function<'src>>>),

    /// Literal (a path)
    Literal(ParsedPath<'src>),
}

impl Action<'_> {
    /// Return the canonical representation of this action
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.value.canonical(),
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
            Self::Function(function) => function.value.evaluate(context),
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

/// Macro to define [`Function`] variants with co-located evaluation logic.
///
/// For each entry, generates:
/// - An enum variant on [`Function`]
/// - A snake\_case convenience constructor on `Function` (derived from the
///   variant name via [`pastey`])
/// - A match arm in [`Function::evaluate()`]
macro_rules! functions {
    (
        $ctx:ident;
        $(
            $(#[$meta:meta])*
            $variant:ident($($param:ident),*) => $body:expr
        ),* $(,)?
    ) => { paste! {
        /// An function to run in response to an HTTP request.
        #[derive(Clone, Debug)]
        pub enum Function<'src> {
            $(
                $(#[$meta])*
                $variant($(functions!(@value_type $param)),*),
            )*
        }

        /// Convenience functions to create variants of [`Function`]
        pub mod functions {
            use super::{Function, Action};
            $(
                #[doc = concat!("Convenience function to construct ",
                    "[`Function::", stringify!($variant), "`].")]
                pub fn [<$variant:snake>]<
                    'src,
                    $([<V $param>]: Into<Action<'src>>),*
                >(
                    $($param: [<V $param>]),*
                ) -> Function<'src> {
                    Function::$variant($($param.into()),*)
                }
            )*
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
            ) -> ParseResult<'src, Spanned<'src, Self>> {
                let actual = parameters.len();
                let mut parameters = parameters.into_iter();
                let span = (name, rparen).into();
                match name {
                    $(
                    stringify!([< $variant:snake >]) => {
                        // Count expected parameters.
                        let expected = 0 $(+ functions!(@count $param))*;
                        $(
                        let $param = match parameters.next() {
                            Some(param) => param.into(),
                            None => return Err(
                                ParseError::WrongFunctionParameterCount {
                                    name,
                                    expected,
                                    actual,
                                }
                                .spanned_s(span)
                            ),
                        };
                        )*
                        if let Some(_) = parameters.next() {
                            return Err(
                                ParseError::WrongFunctionParameterCount {
                                    name,
                                    expected,
                                    actual,
                                }
                                .spanned_s(span)
                            );
                        }
                        Ok(Spanned::new(Self::$variant($($param),*), span))
                    }
                    )*
                    other => {
                        Err(
                            ParseError::UnknownFunctionName(other)
                                .spanned_s(other)
                        )
                    }
                }
            }

            /// Get the function’s name and parameters.
            #[must_use]
            #[inline]
            pub fn name_params<'a>(&'a self)
                -> (&'static str, Vec<&'a Action<'src>>)
            {
                match self {
                    $(
                        Self::$variant($($param),*) => (
                            stringify!([< $variant:snake >]),
                            vec![$($param),*],
                        ),
                    )*
                }
            }

            /// Get the canonical representation of this function call.
            #[must_use]
            pub fn canonical(&self) -> String {
                let (name, params) = self.name_params();
                format!(
                    "{name}({})",
                    params
                        .into_iter()
                        .map(Action::canonical)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }

            /// Calculate the return to be processed by the action.
            ///
            /// # Errors
            ///
            /// Can return [`actions::Error`].
            pub fn evaluate<'vars, M: VariableMap<'vars>>(
                &self,
                $ctx: &'vars Context<'vars, M>,
            ) -> actions::Result {
                match self {
                    $(
                        Function::$variant($($param),*) => $body,
                    )*
                }
            }
        }
    }};

    // Map each parameter name to `Action<'src>`.
    (@value_type $param:ident) => { Action<'src> };

    // Map each parameter name to `1`.
    (@count $param:ident) => { 1 };
}

functions! {
    context;

    /// Add a `'/'` to the end of the input.
    ///
    /// ```text
    /// as_dir(path) -> path
    /// ```
    AsDir(path) => path
        .evaluate(context)?
        .into_string_return()?
        .into_ending_with("/")
        .into(),

    /// Redirect to the passed path if the requested path is not identical.
    ///
    /// Otherwise, return its argument.
    ///
    /// ```text
    /// canonical(path) -> path
    /// ```
    Canonical(path) => {
        let path = path.evaluate(context)?.into_string_return()?;
        tracing::trace!(
            "check canonical ({path:?}) == request ({:?})",
            context.variables.request_path(),
        );
        if path == context.variables.request_path() {
            path.into()
        } else {
            Err(actions::Error::RedirectCanonical(path.into()))
        }
    },

    /// Concatenate two values together.
    ///
    /// ```text
    /// concat(str, str) -> str
    /// ```
    Concat(string1, string2) => string1
        .evaluate(context)?
        .into_string_return()?
        .into_appended(string2.evaluate(context)?.into_string_return()?)
        .into(),

    /// If the first value succeeds, return the second.
    ///
    /// ```text
    /// condition(file, any) -> any
    /// ```
    Condition(condition, value) => {
        // Returns `Error::NotFound` and other errors:
        let _ = condition.evaluate(context)?;
        value.evaluate(context)
    },

    /// Strip the last path component and the last slash.
    ///
    /// ```text
    /// dirname(path) -> path
    /// ```
    Dirname(path) => path
        .evaluate(context)?
        .into_string_return()?
        .into_dirname()
        .into(),

    /// Return an error with the passed code.
    ///
    /// ```text
    /// error(str) -> content
    /// ```
    Error(code) => {
        // FIXME use error code; show error page.
        Err(actions::Error::InternalString(
            code.evaluate(context)?.into_string()?,
        ))
    },

    /// If the passed path is a file, succeed.
    ///
    /// ```text
    /// if_file(path) -> file
    /// ```
    IfFile(path) => {
        path.evaluate(context)?.ensure_file(context)
    },

    /// Join two paths.
    ///
    /// ```text
    /// join(path, path) -> path
    /// ```
    Join(path1, path2) => path1
        .evaluate(context)?
        .into_string_return()?
        .into_joined(path2.evaluate(context)?.into_string_return()?)
        .into(),

    /// Return plain text string.
    ///
    /// ```texts
    /// literal(string) -> content
    /// ```
    Literal(string) => ContentReturn::plain_text(
        string.evaluate(context)?.into_string()?
    ).into(),

    /// Convert the input from Markdown to HTML.
    ///
    /// ```text
    /// markdown(file) -> content
    /// ```
    Markdown(markdown) => {
        actions::markdown_to_html(context, markdown.evaluate(context)?)?
            .into()
    },

    /// Redact the input Markdown.
    ///
    /// ```text
    /// redact_source(file) -> content
    /// ```
    RedactSource(markdown) => {
        actions::redact_source(context, markdown.evaluate(context)?)?
            .into()
    },

    /// Render the input HTML in a template.
    ///
    /// ```text
    /// render(file) -> content
    /// ```
    Render(html) => {
        actions::render(context, html.evaluate(context)?)?
            .into()
    },
}
