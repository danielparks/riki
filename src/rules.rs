//! Code to represent rules about how data is processed and returned.

use crate::actions::{self, Context, Error, Return, Variable, VariableMap};
use globset::{Glob, GlobMatcher};
use std::fmt;

/// A rule for how to respond to HTTP requests.
#[derive(Clone)]
pub struct Rule<'src> {
    /// Matcher for URL path.
    pub path_matcher: GlobMatcher,

    /// The value to return if the URL matches (likely an [`Action`]).
    pub value: Value<'src>,
}

impl<'src> Rule<'src> {
    /// Create a new rule.
    ///
    /// # Panics
    ///
    /// Panics if there is a problem parsing the matcher.
    pub fn new<V: Into<Value<'src>>>(path_matcher: &str, value: V) -> Self {
        Self {
            path_matcher: Glob::new(path_matcher).unwrap().compile_matcher(),
            value: value.into(),
        }
    }

    /// Evaluate a rule.
    ///
    /// # Errors
    ///
    /// Most variants of [`Error`] should be returned as an HTTP response,
    /// except [`Error::NotFound`], which means that this rule should be skipped
    /// and the next rule evaluated.
    pub fn evaluate<'vars, V: VariableMap<'vars>>(
        &self,
        context: &'vars Context<'vars, V>,
    ) -> actions::Result {
        if self.path_matcher.is_match(context.variables.clean_path()) {
            self.value.evaluate(context)
        } else {
            Err(Error::NotFound)
        }
    }
}

impl fmt::Debug for Rule<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rule")
            // Output glob string instead of glob internals.
            .field("path_matcher", &self.path_matcher.glob())
            .field("value", &self.value)
            .finish()
    }
}

/// Something that can be passed to an action.
#[derive(Clone, derive_more::From)]
pub enum Value<'src> {
    /// Value comes from an action.
    Action(Box<Action<'src>>),

    /// Value is a string literal.
    String(&'src str),

    /// Value is a variable.
    Variable(Variable),
}

impl Value<'_> {
    /// Calculate the return to be processed by the action.
    ///
    /// # Errors
    ///
    /// Can return [`actions::Error`].
    pub fn evaluate<'vars, V: VariableMap<'vars>>(
        &self,
        context: &'vars Context<'vars, V>,
    ) -> actions::Result {
        match self {
            Value::Action(action) => action.evaluate(context),
            Value::String(string) => Ok((*string).into()),
            Value::Variable(variable) => Ok(variable.evaluate(context)),
        }
    }
}

impl<'src> From<Action<'src>> for Value<'src> {
    #[inline]
    fn from(action: Action<'src>) -> Self {
        Value::Action(Box::new(action))
    }
}

impl fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_tuple("Value");
        match self {
            Self::Action(v) => f.field(v),
            Self::String(v) => f.field(v),
            Self::Variable(v) => f.field(v),
        }
        .finish()
    }
}

/// An action to run in response to an HTTP request.
#[derive(Clone, Debug)]
pub enum Action<'src> {
    /// Add a `'/'` to the end of the input.
    ///
    /// ```text
    /// as_dir(path) -> path
    /// ```
    AsDir(Value<'src>),

    /// Redirect to the passed path if the requested path is not identical.
    ///
    /// Otherwise, return its argument.
    ///
    /// ```text
    /// canonical(path) -> path
    /// ```
    Canonical(Value<'src>),

    /// Concatenate to values together
    ///
    /// ```text
    /// concat(str, str) -> str
    /// ```
    Concat(Value<'src>, Value<'src>),

    /// If the first value succeeds, return the second.
    ///
    /// ```text
    /// condition(file, any) -> any
    /// ```
    Condition(Value<'src>, Value<'src>),

    /// Strip the last path component and the last slash.
    ///
    /// ```text
    /// dirname(path) -> path
    /// ```
    Dirname(Value<'src>),

    /// Return an error with the passed code.
    ///
    /// ```text
    /// error(str) -> content
    /// ```
    Error(Value<'src>),

    /// If the passed path is a file, succeed.
    ///
    /// ```text
    /// if_file(path) -> file
    /// ```
    IfFile(Value<'src>),

    /// Join two paths.
    ///
    /// ```text
    /// join(path, path) -> path
    /// ```
    Join(Value<'src>, Value<'src>),

    /// Convert the input from Markdown to HTML.
    ///
    /// ```text
    /// markdown(file) -> content
    /// ```
    Markdown(Value<'src>),

    /// Redact the input Markdown.
    ///
    /// ```text
    /// redact_source(file) -> content
    /// ```
    RedactSource(Value<'src>),

    /// Render the input HTML in a template.
    ///
    /// ```text
    /// render(file) -> content
    /// ```
    Render(Value<'src>),
}

impl<'src> Action<'src> {
    /// Convenience function to construct [`Action::AsDir`].
    pub fn as_dir<V: Into<Value<'src>>>(value: V) -> Self {
        Self::AsDir(value.into())
    }

    /// Convenience function to construct [`Action::Canonical`].
    pub fn canonical<V: Into<Value<'src>>>(value: V) -> Self {
        Self::Canonical(value.into())
    }

    /// Convenience function to construct [`Action::Concat`].
    pub fn concat<V: Into<Value<'src>>, V2: Into<Value<'src>>>(
        value: V,
        value2: V2,
    ) -> Self {
        Self::Concat(value.into(), value2.into())
    }

    /// Convenience function to construct [`Action::Condition`].
    pub fn condition<V: Into<Value<'src>>, V2: Into<Value<'src>>>(
        value: V,
        value2: V2,
    ) -> Self {
        Self::Condition(value.into(), value2.into())
    }

    /// Convenience function to construct [`Action::Dirname`].
    pub fn dirname<V: Into<Value<'src>>>(value: V) -> Self {
        Self::Dirname(value.into())
    }

    /// Convenience function to construct [`Action::Error`].
    pub fn error<V: Into<Value<'src>>>(value: V) -> Self {
        Self::Error(value.into())
    }

    /// Convenience function to construct [`Action::IfFile`].
    pub fn if_file<V: Into<Value<'src>>>(value: V) -> Self {
        Self::IfFile(value.into())
    }

    /// Convenience function to construct [`Action::Join`].
    pub fn join<V: Into<Value<'src>>, V2: Into<Value<'src>>>(
        value: V,
        value2: V2,
    ) -> Self {
        Self::Join(value.into(), value2.into())
    }

    /// Convenience function to construct [`Action::Markdown`].
    pub fn markdown<V: Into<Value<'src>>>(value: V) -> Self {
        Self::Markdown(value.into())
    }

    /// Convenience function to construct [`Action::RedactSource`].
    pub fn redact_source<V: Into<Value<'src>>>(value: V) -> Self {
        Self::RedactSource(value.into())
    }

    /// Convenience function to construct [`Action::Render`].
    pub fn render<V: Into<Value<'src>>>(value: V) -> Self {
        Self::Render(value.into())
    }
}

impl Action<'_> {
    /// Calculate the return to be processed by the action.
    ///
    /// # Errors
    ///
    /// Can return [`actions::Error`].
    pub fn evaluate<'vars, M: VariableMap<'vars>>(
        &self,
        context: &'vars Context<'vars, M>,
    ) -> actions::Result {
        match self {
            Action::AsDir(path) => path
                .evaluate(context)?
                .into_string_return()?
                .into_ending_with("/")
                .into(),
            Action::Canonical(path) => {
                let path = path.evaluate(context)?.into_string_return()?;
                tracing::trace!(
                    "check canonical ({path:?}) == request ({:?})",
                    context.variables.request_path(),
                );
                if path == context.variables.request_path() {
                    path.into()
                } else {
                    Err(actions::Error::RedirectCanonical(path.try_into()?))
                }
            }
            Action::Concat(string1, string2) => string1
                .evaluate(context)?
                .into_string_return()?
                .into_appended(string2.evaluate(context)?.into_string_return()?)
                .into(),
            Action::Condition(condition, value) => {
                // Returns `Error::NotFound` and other errors:
                let _ = condition.evaluate(context)?;
                value.evaluate(context)
            }
            Action::Dirname(path) => path
                .evaluate(context)?
                .into_string_return()?
                .into_dirname()
                .into(),
            Action::Error(code) => {
                // FIXME use error code; show error page.
                Err(actions::Error::InternalString(
                    code.evaluate(context)?.into_string_return()?.try_into()?,
                ))
            }
            Action::IfFile(path) => {
                path.evaluate(context)?.ensure_file(context)
            }
            Action::Join(path1, path2) => path1
                .evaluate(context)?
                .into_string_return()?
                .into_joined(path2.evaluate(context)?.into_string_return()?)
                .into(),
            Action::Markdown(markdown) => {
                actions::markdown_to_html(context, markdown.evaluate(context)?)?
                    .ok_or(actions::Error::NotFound)?
                    .into()
            }
            Action::RedactSource(markdown) => {
                actions::redact_source(context, markdown.evaluate(context)?)?
                    .ok_or(actions::Error::NotFound)?
                    .into()
            }
            Action::Render(html) => {
                actions::render(context, html.evaluate(context)?)?
                    .ok_or(actions::Error::NotFound)?
                    .into()
            }
        }
    }
}

/// Get the default rules for Riki.
#[must_use]
pub fn default_rules() -> Vec<Rule<'static>> {
    vec![
        // *.md redact_source(canonical($clean_path))
        Rule::new(
            "**/*.md",
            Action::redact_source(Action::canonical(Variable::CleanPath)),
        ),
        // index.html canonical("${dirname($clean_path)}/")
        Rule::new(
            "**/index.html",
            Action::canonical(Action::as_dir(Action::dirname(
                Variable::CleanPath,
            ))),
        ),
        // if file_exists("$clean_path") {
        //     canonical($clean_path) // returns $clean_path as a file if it
        // matches. }
        Rule::new(
            "**",
            Action::canonical(Action::if_file(Variable::CleanPath)),
        ),
        // if file_exists("$clean_path/index.html") {
        //     if canonical("${clean_path}/") {
        //         $clean_path/index.html
        //     }
        // }
        Rule::new(
            "**",
            Action::condition(
                Action::canonical(Action::condition(
                    Action::if_file(Action::join(
                        Variable::CleanPath,
                        "index.html",
                    )),
                    Action::as_dir(Variable::CleanPath),
                )),
                Action::join(Variable::CleanPath, "index.html"),
            ),
        ),
        // index canonical("${dirname($clean_path)}/")
        Rule::new(
            "**/index",
            Action::canonical(Action::as_dir(Action::dirname(
                Variable::CleanPath,
            ))),
        ),
        // if file_exists("${clean_path}.md") {
        //     if canonical($clean_path) {
        //         render(markdown("${clean_path}.md"))
        //     }
        // }
        Rule::new(
            "**",
            Action::condition(
                Action::canonical(Action::condition(
                    Action::if_file(Action::concat(Variable::CleanPath, ".md")),
                    Variable::CleanPath,
                )),
                Action::render(Action::markdown(Action::if_file(
                    Action::concat(Variable::CleanPath, ".md"),
                ))),
            ),
        ),
        // if file_exists("$clean_path/index.md") {
        //     if canonical("${clean_path}/") {
        //         render(markdown("$clean_path/index.md"))
        //     }
        // }
        Rule::new(
            "**",
            Action::condition(
                Action::canonical(Action::condition(
                    Action::if_file(Action::join(
                        Variable::CleanPath,
                        "index.md",
                    )),
                    Action::as_dir(Variable::CleanPath),
                )),
                Action::render(Action::markdown(Action::if_file(
                    Action::join(Variable::CleanPath, "index.md"),
                ))),
            ),
        ),
    ]
}
