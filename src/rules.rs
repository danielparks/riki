//! Code to represent rules about how data is processed and returned.

use crate::actions::{self, Context, Error, VariableMap};
use crate::config::actions::{Action, functions};
use crate::config::model::ParsedString;
use globset::{Glob, GlobMatcher};
use std::fmt;

/// A rule for how to respond to HTTP requests.
#[derive(Clone)]
pub struct Rule<'src> {
    /// Matcher for URL path.
    pub path_matcher: GlobMatcher,

    /// The action to take if the URL matches.
    pub value: Action<'src>,
}

impl<'src> Rule<'src> {
    /// Create a new rule.
    ///
    /// # Panics
    ///
    /// Panics if there is a problem parsing the matcher.
    pub fn new<V: Into<Action<'src>>>(path_matcher: &str, value: V) -> Self {
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

/// Create a [`ParsedString`] from string content with variable interpolation.
///
/// # Panics
///
/// Panics if the string cannot be parsed.
fn parsed(s: &str) -> ParsedString<'_> {
    use crate::config::parser2::StringType;
    ParsedString::from_string_content(s, StringType::QuotedDouble).unwrap()
}

/// Get the default rules for Riki.
#[must_use]
pub fn default_rules() -> Vec<Rule<'static>> {
    vec![
        // *.md redact_source(canonical($clean_path))
        Rule::new(
            "**/*.md",
            functions::redact_source(functions::canonical(parsed(
                "$clean_path",
            ))),
        ),
        // index.html canonical("${dirname($clean_path)}/")
        Rule::new(
            "**/index.html",
            functions::canonical(functions::as_dir(functions::dirname(
                parsed("$clean_path"),
            ))),
        ),
        // if file_exists("$clean_path") {
        //     canonical($clean_path) // returns $clean_path as a file if it
        // matches. }
        Rule::new(
            "**",
            functions::canonical(functions::if_file(parsed("$clean_path"))),
        ),
        // if file_exists("$clean_path/index.html") {
        //     if canonical("${clean_path}/") {
        //         $clean_path/index.html
        //     }
        // }
        Rule::new(
            "**",
            functions::condition(
                functions::canonical(functions::condition(
                    functions::if_file(functions::join(
                        parsed("$clean_path"),
                        ParsedString::from_literal("index.html"),
                    )),
                    functions::as_dir(parsed("$clean_path")),
                )),
                functions::join(
                    parsed("$clean_path"),
                    ParsedString::from_literal("index.html"),
                ),
            ),
        ),
        // index canonical("${dirname($clean_path)}/")
        Rule::new(
            "**/index",
            functions::canonical(functions::as_dir(functions::dirname(
                parsed("$clean_path"),
            ))),
        ),
        // if file_exists("${clean_path}.md") {
        //     if canonical($clean_path) {
        //         render(markdown("${clean_path}.md"))
        //     }
        // }
        Rule::new(
            "**",
            functions::condition(
                functions::canonical(functions::condition(
                    functions::if_file(parsed("${clean_path}.md")),
                    parsed("$clean_path"),
                )),
                functions::render(functions::markdown(functions::if_file(
                    parsed("${clean_path}.md"),
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
            functions::condition(
                functions::canonical(functions::condition(
                    functions::if_file(functions::join(
                        parsed("$clean_path"),
                        ParsedString::from_literal("index.md"),
                    )),
                    functions::as_dir(parsed("$clean_path")),
                )),
                functions::render(functions::markdown(functions::if_file(
                    functions::join(
                        parsed("$clean_path"),
                        ParsedString::from_literal("index.md"),
                    ),
                ))),
            ),
        ),
    ]
}
