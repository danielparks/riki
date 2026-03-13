//! The types that represent the actual configuration

mod glob;
mod string;
pub use glob::*;
pub use string::*;

use super::actions::Action;
use super::errors::{ParseError, ParseResult, SpannedErrors};
use super::lexer::Diagnostic;
use super::parser::parse_to_cst;
use super::parser2::{ContentSource, Identifier, MatcherStack, process_cst};
use crate::actions::{self, RequestVariables, Return, VariableMap};
use crate::render::TemplatesManager;
use globset::{GlobSet, GlobSetBuilder};

/// An entire configuration
#[derive(Clone, Debug)]
pub struct Configuration<'src> {
    /// Matcher to determine which rules to apply.
    globset: GlobSet,

    /// All the rules in the configuration.
    rules: Vec<ConfigRule<'src>>,
}

impl<'src> Configuration<'src> {
    /// Parse a configuration string.
    ///
    /// # Errors
    ///
    /// Returns <code>Vec<[Diagnostic]></code> for parse errors.
    pub fn parse<S: ContentSource>(
        source: &'src S,
    ) -> Result<Self, Vec<Diagnostic>> {
        process_cst(&parse_to_cst(source)?).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.into_diagnostic(source))
                .collect()
        })
    }

    /// Get all the rules.
    #[must_use]
    pub fn rules(&self) -> &[ConfigRule<'src>] {
        &self.rules
    }

    /// Get a canonical representation of the rules.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = Vec::new();
        let mut settings = &ConfigSettings::default();
        for rule in self.rules() {
            if &rule.settings != settings {
                settings = &rule.settings;
                out.push(settings.canonical("/**").join("\n"));
            }
            out.push(rule.canonical());
        }
        out.join("\n")
    }

    /// Get matching rules for a path.
    ///
    /// This always returns rules in the order they were defined.
    #[must_use]
    pub fn matches(&self, path: &str) -> Vec<&ConfigRule<'src>> {
        // globset.matches() always returns a Vec of indices sorted by value.
        self.globset
            .matches(path)
            .into_iter()
            .map(|i| &self.rules[i])
            .collect()
    }

    /// Get matching rules for a path.
    #[must_use]
    pub fn last_matching(&self, path: &str) -> Option<&ConfigRule<'src>> {
        // globset.matches() always returns a Vec of indices sorted by value.
        self.globset.matches(path).last().map(|&i| &self.rules[i])
    }

    /// Evaluate a request through the configuration rules.
    ///
    /// # Errors
    ///
    /// This tries to return errors as rendered responses, but if it fails it
    /// may return an error to be rendered by the fallback.
    pub fn evaluate(
        &self,
        manager: &TemplatesManager,
        request: &axum::extract::Request,
    ) -> actions::Result<axum::response::Response> {
        let path = request.uri().path();

        // Returns an error if `clean_path()` fails, which should only happen if
        // the client makes a bad request.
        let variables = match RequestVariables::new(request) {
            Ok(variables) => variables,
            Err(error) => {
                // The path was invalid, so try to get templates for /. Errors
                // in this handler get passed to the fallback renderer.
                tracing::warn!("{error:?}");
                if let Some(tpls_path) =
                    self.last_matching("/").and_then(|rule| {
                        rule.settings.templates.no_variable_path_content()
                    })
                {
                    let tpls = manager.templates_for_directory(tpls_path)?;
                    return Ok(error.render(path, &tpls));
                }
                return Err(error);
            }
        };

        match (|| {
            for rule in self.matches(&variables.clean_path()) {
                // FIXME &variables instead of clone()
                match rule.evaluate(manager, variables.clone()) {
                    Err(actions::Error::Skip) => (),
                    other => return other,
                }
            }
            Err(actions::Error::NotFound)
        })() {
            Ok(response) => Ok(response),
            Err(error) => {
                // Errors in this handler get passed to the fallback renderer.
                tracing::trace!("error returned from rules: {error:?}");
                if let Some(rule) = self.last_matching(&variables.clean_path())
                {
                    let tpls = manager.templates_for_directory(
                        rule.settings.templates.path_content(&variables),
                    )?;
                    Ok(error.render(path, &tpls))
                } else {
                    Err(error)
                }
            }
        }
    }
}

/// Build a configuration from rules
#[derive(Clone, Debug)]
pub struct ConfigurationBuilder<'src> {
    /// Builder for the final `GlobSet`.
    globset_builder: GlobSetBuilder,

    /// All the rules in the configuration.
    rules: Vec<ConfigRule<'src>>,
}

impl Default for ConfigurationBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'src> ConfigurationBuilder<'src> {
    /// Create an empty `ConfigurationBuilder`.
    #[must_use]
    pub fn new() -> Self {
        Self { globset_builder: GlobSetBuilder::new(), rules: Vec::new() }
    }

    /// Add a rule.
    ///
    /// # Errors
    ///
    /// Returns an error if there is a problem creating a [`globset::Glob`] from
    /// the rule’s matchers.
    pub fn add(&mut self, rule: ConfigRule<'src>) -> ParseResult<'src, ()> {
        self.globset_builder.add(rule.matcher.as_glob()?);
        self.rules.push(rule);
        Ok(())
    }

    /// Build a [`Configuration`].
    ///
    /// # Errors
    ///
    /// Returns an error if there is a problem creating a [`GlobSet`] from all
    /// the rule matchers.
    pub fn build(self) -> ParseResult<'src, Configuration<'src>> {
        let Self { globset_builder, rules } = self;
        Ok(Configuration {
            globset: globset_builder.build().map_err(|error| {
                ParseError::BuildingGlobSet(error).without_spans().plural()
            })?,
            rules,
        })
    }
}

/// A rule generated by a configuration file
#[derive(Clone, Debug)]
pub struct ConfigRule<'src> {
    /// Match a request.
    pub matcher: MatcherStack<'src>,

    /// Settings from configuration, e.g. the root directory.
    pub settings: ConfigSettings<'src>,

    /// Action to take in response to a request.
    pub action: Action<'src>,
}

impl<'src> ConfigRule<'src> {
    /// Convenience method to create a new rule.
    #[must_use]
    pub fn new(
        glob: &'src str,
        settings: ConfigSettings<'src>,
        action: Action<'src>,
    ) -> Self {
        Self {
            matcher: MatcherStack::from_glob_strs([glob]),
            settings,
            action,
        }
    }

    /// Return the canonical representation of this rule
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} {}", self.matcher.canonical(), self.action.canonical())
    }

    /// Evaluate a rule, ignoring `self.matcher`.
    ///
    /// The `matcher` must be handled separately.
    ///
    /// # Errors
    ///
    /// Most variants of [`actions::Error`] should be returned as an HTTP
    /// response, except [`actions::Error::NotFound`], which means that this
    /// rule should be skipped and the next rule evaluated.
    fn evaluate(
        &self,
        manager: &TemplatesManager,
        variables: RequestVariables<'_>,
    ) -> actions::Result<axum::response::Response> {
        let templates_path = self.settings.templates.path_content(&variables);
        let context = actions::Context {
            working_path: self.settings.root.path_content(&variables).into(),
            // FIXME? might not need to load templates
            tpls: manager.templates_for_directory(templates_path)?,
            variables,
        };

        match self
            .action
            .evaluate(&context)
            .and_then(|ret| ret.into_response(&context))
        {
            Ok(response) => {
                tracing::trace!("success {}: {response:?}", self.canonical());
                Ok(response)
            }
            Err(actions::Error::Skip) => {
                tracing::trace!("skip {}", self.canonical());
                Err(actions::Error::Skip)
            }
            Err(error) => {
                tracing::trace!("error {}: {error:?}", self.canonical());
                Ok(error
                    .render(&context.variables.request_path(), &context.tpls))
            }
        }
    }
}

/// Settings from configuration.
///
/// For example, `root = /srv`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigSettings<'src> {
    /// The root directory to search relative to.
    pub root: ParsedString<'src>,

    /// Template directory.
    pub templates: ParsedString<'src>,
}

impl<'src> ConfigSettings<'src> {
    /// Apply a setting.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`]s if there are problems joining paths or if `name`
    /// or `value` are invalid.
    pub fn apply(
        &mut self,
        name: &Identifier<'src>,
        value: Action<'src>,
    ) -> Result<(), SpannedErrors<'src>> {
        match (name.0, value) {
            ("root", Action::Literal(value)) => {
                self.root.push_path(&value);
                Ok(())
            }
            ("templates", Action::Literal(value)) => {
                self.templates = self.root.join_path(&value);
                Ok(())
            }
            ("root" | "templates", Action::Function(function)) => {
                Err(ParseError::SettingDoesNotAcceptFunction(name.0)
                    .spanned_s((name.0, function.span.clone())))
            }
            (_, _) => {
                Err(ParseError::UnknownSettingName(name.0).spanned_s(name.0))
            }
        }
    }

    /// Generate a canonical version of these settings.
    #[must_use]
    pub fn canonical(&self, matcher: &str) -> Vec<String> {
        vec![
            format!(r"{matcher} root = {}", self.root.canonical()),
            format!(r"{matcher} templates = {}", self.templates.canonical()),
        ]
    }
}

impl Default for ConfigSettings<'_> {
    /// Default settings.
    ///
    ///   * `root`: `.`
    ///   * `templates`: `./templates`
    fn default() -> Self {
        Self { root: ".".into(), templates: "templates".into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::StaticVariables;
    use assert2::check;
    use globset::Glob;

    fn into_glob<'a, I: IntoIterator<Item = &'a str>>(
        globs: I,
    ) -> ParseResult<'a, Glob> {
        let stack = MatcherStack::from_glob_strs(globs);
        println!("glob str: {}", stack.as_glob_str());
        stack.as_glob()
    }

    #[test_log::test]
    fn matcher_stack_paths_makes_valid_globs() {
        check!(into_glob([]).is_ok());
        check!(into_glob(["/"]).is_ok());
        check!(into_glob(["/", "/"]).is_ok());
        check!(into_glob(["/", "foobar", "/zz/"]).is_ok());
        check!(into_glob(["/", "/", "/zz"]).is_ok());
        check!(into_glob(["abc", "/", "/zz"]).is_ok());
        check!(into_glob(["abc", "def", "/zz"]).is_ok());
    }

    #[test_log::test]
    fn matcher_stack_globs_makes_valid_globs() {
        check!(into_glob(["*"]).is_ok());
        check!(into_glob(["**/foo"]).is_ok());
        check!(into_glob(["**/**", "foobar"]).is_ok());
        check!(into_glob(["/", "[abc]?", "/zz"]).is_ok());
    }

    #[test_log::test]
    fn match_file_glob() {
        let matcher = into_glob(["/foo/bar.md"]).unwrap().compile_matcher();
        check!(matcher.is_match("/foo/bar.md"));
        check!(matcher.is_match("/foo/bar.md/"));
        check!(matcher.is_match("/foo/bar.md/test"));
    }

    #[test_log::test]
    fn match_dir_glob() {
        let matcher = into_glob(["/foo/"]).unwrap().compile_matcher();
        check!(!matcher.is_match("/foo"));
        check!(matcher.is_match("/foo/"));
        check!(matcher.is_match("/foo/bar.md/test"));
    }

    /// Test variables
    const VARS: StaticVariables = StaticVariables {
        request_path: "/abc/",
        verb: "GET",
        host: "example.com",
    };

    /// Helper to create a `ParsedString` literal.
    fn literal(s: &str) -> Action<'_> {
        Action::Literal(ParsedString::from_literal(s))
    }

    #[test_log::test]
    fn config_settings_apply_relative() {
        let mut settings = ConfigSettings::default();
        settings
            .apply(&Identifier("root"), literal("new/root"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "./new/root");
        check!(settings.templates.path_content(&VARS) == "templates");

        settings
            .apply(&Identifier("templates"), literal("tpls"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "./new/root");
        check!(settings.templates.path_content(&VARS) == "./new/root/tpls");
    }

    #[test_log::test]
    fn config_settings_apply_relative_parent() {
        let mut settings = ConfigSettings::default();
        settings
            .apply(&Identifier("root"), literal("/root/a/b"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "/root/a/b");
        settings
            .apply(&Identifier("root"), literal("../c"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "/root/a/b/../c");
    }

    #[test_log::test]
    fn config_settings_apply_absolute() {
        let mut settings = ConfigSettings::default();
        settings
            .apply(&Identifier("root"), literal("/a/b"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "/a/b");
        check!(settings.templates.path_content(&VARS) == "templates");

        settings
            .apply(&Identifier("templates"), literal("templates"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "/a/b");
        check!(settings.templates.path_content(&VARS) == "/a/b/templates");

        settings
            .apply(&Identifier("templates"), literal("/templates"))
            .unwrap();
        check!(settings.root.path_content(&VARS) == "/a/b");
        check!(settings.templates.path_content(&VARS) == "/templates");
    }
}
