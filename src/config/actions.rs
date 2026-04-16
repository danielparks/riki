//! Perform actions defined in configuration file.

mod tests;

use super::errors::{ParseError, ParseResult};
use super::model::ParsedString;
use super::parser2::{Parameters, Span, Spanned};
use crate::actions::{
    self, ContentReturn, Context, Return, StringReturn, VariableMap,
};
use pastey::paste;
use std::fmt;

/// An action to take in response to an HTTP request.
///
/// The two `evaluate...()` methods have different return types and differ in
/// how they deal with literals.
///
///   * [`Action::evaluate()`]: evaluate literals as paths. This means that a
///     literal that starts with a variable is _always_ a relative path.
///   * [`Action::evaluate_as_string()`]: literals are treated as strings. This
///     means that a literal that starts with a variable might evaluate to a
///     string that starts with a slash.
#[derive(Clone, derive_more::From)]
pub enum Action<'src> {
    /// Function call.
    Function(Box<Spanned<'src, Function<'src>>>),

    /// Literal (a string value).
    Literal(ParsedString<'src>),
}

impl Action<'_> {
    /// Return the canonical representation of this action.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.value.canonical(),
            Self::Literal(string) => string.canonical(),
        }
    }

    /// Evaluate the action for a request as a path, file, or content.
    ///
    /// This will evaluate literals as paths. This means that a literal that
    /// starts with a variable is _always_ a relative path.
    ///
    /// # Errors
    ///
    /// Returns [`actions::Error`] if there is a problem evaluating a function
    /// or opening a path.
    pub fn evaluate<V: VariableMap>(
        &self,
        context: &Context<V>,
    ) -> actions::Result {
        match self {
            Self::Function(function) => function.value.evaluate(context),
            Self::Literal(string) => {
                StringReturn::from(string.path_content(&context.variables))
                    .into()
            }
        }
    }

    /// Evaluate the action for a request as a string.
    ///
    /// This will evaluate literals are as strings. A literal that starts with a
    /// variable might evaluate to a string that starts with a slash.
    ///
    /// # Errors
    ///
    /// Returns [`actions::Error`] if there is a problem evaluating a function
    /// or opening a path.
    pub fn evaluate_as_string<V: VariableMap>(
        &self,
        context: &Context<V>,
    ) -> actions::Result<String> {
        match self {
            Self::Function(function) => {
                function.value.evaluate(context)?.into_string()
            }
            Self::Literal(string) => Ok(string.content(&context.variables)),
        }
    }
}

impl fmt::Debug for Action<'_> {
    /// Hide `Action` in debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Function(function) => function.fmt(f),
            Self::Literal(literal) => literal.fmt(f),
        }
    }
}

impl<'src> From<&'src str> for Action<'src> {
    /// Convenience for producing actions in code.
    fn from(string: &'src str) -> Self {
        Self::Literal(ParsedString::from_literal(string))
    }
}

impl From<String> for Action<'_> {
    /// Convenience for producing actions in code.
    fn from(string: String) -> Self {
        Self::Literal(ParsedString::from(string))
    }
}

impl<'src> From<Function<'src>> for Action<'src> {
    /// Convenience for using [`Function`] in code.
    ///
    /// This use an empty, `'static` string for the span.
    #[inline]
    fn from(func: Function<'src>) -> Self {
        Spanned::new(func, Span::Slice("")).into()
    }
}

impl<'src> From<Spanned<'src, Function<'src>>> for Action<'src> {
    #[inline]
    fn from(func: Spanned<'src, Function<'src>>) -> Self {
        Self::Function(Box::new(func))
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
            pub fn evaluate<M: VariableMap>(
                &self,
                $ctx: &Context<M>,
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

functions! (
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
    /// FIXME what if the argument should not be returned to the user? Example:
    ///
    /// ```text
    /// Rule::new(
    ///     "**/index.html",
    ///     canonical(as_dir(dirname(parsed("$clean_path")))),
    /// ),
    /// ```
    ///
    /// Suppose we requested /a/b/c/index.html and /a/b/c is a file.
    ///
    /// ```text
    /// canonical(path) -> path
    /// ```
    Canonical(path) => {
        let input = path.evaluate(context)?;
        let url_path = input.url_path()?;
        tracing::trace!(
            "check canonical ({url_path:?}) == request ({:?})",
            context.variables.request_path(),
        );
        if *url_path == *context.variables.request_path() {
            input.into()
        } else {
            Err(actions::Error::RedirectCanonical(url_path.into_owned()))
        }
    },

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
    /// `error("404")` will return a 404 Not Found error to the client rather
    /// than falling through to the next rule. (It generates
    /// [`actions::Error::NotFound`] rather than [`actions::Error::Skip`].)
    ///
    /// ```text
    /// error(str) -> content
    /// ```
    Error(code) => Err(
        actions::Error::from_config_error(&code.evaluate_as_string(context)?)
    ),

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
        string.evaluate_as_string(context)?,
    ).into(),

    /// Convert the input from Markdown to HTML.
    ///
    /// ```text
    /// markdown(file) -> content
    /// ```
    Markdown(file) => {
        actions::markdown_to_html(context, file.evaluate(context)?)?
            .into()
    },

    /// Redact the input Markdown.
    ///
    /// ```text
    /// redact_source(file) -> content
    /// ```
    RedactSource(file) => {
        actions::redact_source(context, file.evaluate(context)?)?
            .into()
    },

    /// Render the input HTML in a template.
    ///
    /// ```text
    /// render(file) -> content
    /// ```
    Render(file) => {
        actions::render(context, file.evaluate(context)?)?
            .into()
    },
);
