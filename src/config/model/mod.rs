//! The types that represent the actual configuration

mod errors;
mod glob;
mod string;
pub use errors::*;
pub use glob::*;
pub use string::*;

use super::lexer::TokenType;
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::fmt;
use std::slice;

/// An entire configuration
#[expect(dead_code, reason = "wip")]
pub struct Configuration<'src> {
    /// Matcher to determine which rules to apply.
    globset: GlobSet,

    /// All the rules in the configuration.
    rules: Vec<ConfigRule<'src>>,
}

impl<'src> Configuration<'src> {
    /// Get all the rules.
    #[must_use]
    pub fn rules(&self) -> &[ConfigRule<'src>] {
        &self.rules
    }
}

/// Build a configuration from rules
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
    /// Returns an error if there is a problem creating a [`Glob`] from the
    /// rule’s matchers.
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
                ParseError::BuildingGlobSet(error)
                    .with_spans(Vec::new())
                    .plural()
            })?,
            rules,
        })
    }
}

/// A rule found in the configuration file.
#[derive(Clone, Debug)]
pub struct ConfigRule<'src> {
    /// Match a request
    pub matcher: MatcherStack<'src>,
    /// Action to take in response to a request
    pub action: Action<'src>,
}

impl ConfigRule<'_> {
    /// Return the canonical representation of this rule
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} {}", self.matcher.canonical(), self.action.canonical())
    }
}

/// A stack of matchers for a request.
#[derive(Clone, Debug, Default)]
pub struct MatcherStack<'src>(pub Vec<Matcher<'src>>);

impl<'src> MatcherStack<'src> {
    /// Get a new, empty matcher stack
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Create a matcher stack from a slice of bare glob strings
    #[must_use]
    pub fn from_glob_strs<I: IntoIterator<Item = &'src str>>(globs: I) -> Self {
        Self(
            globs
                .into_iter()
                .map(GlobString::from_glob_str)
                .map(Matcher)
                .collect(),
        )
    }

    /// Add a matcher to the stack
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Add a matcher to the stack
    pub fn push(&mut self, matcher: Matcher<'src>) {
        self.0.push(matcher);
    }

    /// Remove a matcher from the top of the stack
    pub fn pop(&mut self) -> Option<Matcher<'src>> {
        self.0.pop()
    }

    /// Get the spans for each matcher
    #[must_use]
    pub fn spans(&self) -> Vec<&'src str> {
        self.0.iter().map(Matcher::span).collect()
    }

    /// Get the matcher stack as a glob
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    ///
    /// # Errors
    ///
    /// May pass through errors from [`GlobBuilder::build()`].
    pub fn as_glob(&self) -> ParseResult<'src, Glob> {
        GlobBuilder::new(&self.as_glob_str())
            .literal_separator(true)
            .backslash_escape(true)
            .empty_alternates(true)
            .build()
            .map_err(|error| {
                ParseError::BuildingGlob(error)
                    .with_spans(self.spans())
                    .into()
            })
    }

    /// Get the matcher stack as a glob string
    ///
    /// This does not necessarily include every condition in the matcher.
    ///
    ///   * FIXME: allow other conditions; evaluate them.
    ///   * FIXME: Treat `"*", "*"` specially?
    ///
    /// ```
    /// use assert2::check;
    /// use riki::config::model::MatcherStack;
    ///
    /// fn combine<'a, I: IntoIterator<Item=&'a str>>(globs: I) -> String {
    ///     MatcherStack::from_glob_strs(globs).as_glob_str()
    /// }
    ///
    /// check!(combine([]) == "/**");
    /// check!(combine(["/"]) == "/**");
    /// check!(combine(["/", "/"]) == "/**");
    /// check!(combine(["/", "foobar", "/zz/"]) == "/**/foobar/zz/**");
    /// check!(combine(["/", "/", "/zz"]) == "/zz{,/**}");
    /// check!(combine(["abc", "/", "/zz"]) == "/**/abc/zz{,/**}");
    /// check!(combine(["abc", "def", "/zz"]) == "/**/abc/**/def/zz{,/**}");
    ///
    /// check!(combine(["*"]) == "/**/*{,/**}");
    /// check!(combine(["**/foo"]) == "/**/foo{,/**}");
    /// check!(combine(["**", "**", "foo"]) == "/**/**/foo{,/**}");
    /// check!(combine(["**", "a"]) == "/**/a{,/**}");
    /// check!(combine(["**", "/a"]) == "/**/a{,/**}");
    /// check!(combine(["**/", "a"]) == "/**/**/a{,/**}");
    /// check!(combine(["**/", "/a"]) == "/**/a{,/**}");
    /// check!(combine(["**/**", "foobar"]) == "/**/**/foobar{,/**}");
    /// check!(combine(["/", "[abc]?", "/zz"]) == "/**/[abc]?/zz{,/**}");
    /// check!(combine(["abc", "/{foo,**}"]) == "/**/abc/{foo,**}{,/**}");
    /// ```
    #[must_use]
    #[inline]
    pub fn as_glob_str(&self) -> String {
        let mut full_glob = "/".to_owned(); // FIXME capacity?
        for matcher in &self.0 {
            let glob_str = matcher.as_glob_str();
            #[expect(clippy::manual_strip, reason = "clarity, simplicity")]
            if glob_str.starts_with('/') {
                if full_glob.ends_with('/') {
                    full_glob.push_str(&glob_str[1..]);
                } else {
                    full_glob.push_str(&glob_str);
                }
            } else if glob_str.starts_with("**/") || glob_str == "**" {
                if !full_glob.ends_with('/') {
                    full_glob.push('/');
                }
                full_glob.push_str(&glob_str);
            } else {
                // Doesn’t start with / or **/
                full_glob.push_str(if full_glob.ends_with("/**") {
                    "/"
                } else if full_glob.ends_with('/') {
                    "**/"
                } else {
                    "/**/"
                });
                full_glob.push_str(&glob_str);
            }
        }

        // Match a path prefix, not just and exact path.
        if full_glob.ends_with('/') {
            full_glob.push_str("**");
        } else if !full_glob.ends_with("/**") {
            // It doesn’t actually matter if the full glob ends with `/**`,
            // since `{,/**}` can match nothing.
            full_glob.push_str("{,/**}");
        }

        full_glob
    }

    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self) -> String {
        self.as_glob_str()
    }

    /// Return an iterator over the matchers
    pub fn iter(&self) -> slice::Iter<'_, Matcher<'src>> {
        self.0.iter()
    }
}

impl<'a, 'src> IntoIterator for &'a MatcherStack<'src>
where
    'src: 'a,
{
    type Item = &'a Matcher<'src>;
    type IntoIter = slice::Iter<'a, Matcher<'src>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A matcher for a request.
#[derive(Clone, Debug)]
pub struct Matcher<'src>(GlobString<'src>);

impl<'src> Matcher<'src> {
    /// Return the canonical representation of this matcher
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.canonical()
    }

    /// Get the matcher as a glob string
    ///
    /// This does not necessarily include every condition in the matcher.
    /// FIXME: allow other conditions; evaluate them.
    #[must_use]
    pub fn as_glob_str(&self) -> String {
        self.0.as_glob_str()
    }

    /// Get the span of the original source for this matcher.
    #[must_use]
    pub const fn span(&self) -> &'src str {
        self.0.span()
    }
}

impl<'src> TryFrom<StringToken<'src>> for Matcher<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        // FIXME validate glob
        Ok(Matcher(token.try_into()?))
    }
}

/// The action corresponding to a rule.
#[derive(Clone, Debug)]
pub enum Action<'src> {
    /// Set an option for matching requests.
    Setting(Setting<'src>),

    /// Value to return for matching requests.
    Value(Value<'src>),
}

impl Action<'_> {
    /// Return the canonical representation of this action
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Setting(setting) => setting.canonical(),
            Self::Value(value) => value.canonical(),
        }
    }
}

/// A configuration setting
#[derive(Clone, Debug)]
pub struct Setting<'src> {
    /// The variable being set
    name: Identifier<'src>,

    /// The value
    value: Value<'src>,
}

impl Setting<'_> {
    /// Return the canonical representation of this setting
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{} = {}", self.name.canonical(), self.value.canonical())
    }
}

impl<'src> TryFrom<(Identifier<'src>, Value<'src>)> for Setting<'src> {
    type Error = ParseError<'src>;

    fn try_from(
        (name, value): (Identifier<'src>, Value<'src>),
    ) -> Result<Self, Self::Error> {
        super::validate_setting(name.0, &value)?;
        Ok(Self { name, value })
    }
}

/// A value for a configuration setting or to return as a response.
#[derive(Clone, Debug)]
pub enum Value<'src> {
    /// Call a function.
    Function(FunctionCall<'src>),

    /// A string of some kind.
    Literal(ParsedString<'src>),
}

impl Value<'_> {
    /// Return the canonical representation of this value
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Function(function) => function.canonical(),
            Self::Literal(string) => string.canonical(),
        }
    }
}

impl<'src> TryFrom<StringToken<'src>> for Value<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        Ok(Self::Literal(token.try_into()?))
    }
}

/// A function call
#[derive(Clone, Debug)]
pub struct FunctionCall<'src> {
    /// The function name
    name: Identifier<'src>,

    /// The parameters
    parameters: Parameters<'src>,
}

impl FunctionCall<'_> {
    /// Return the canonical representation of this function call
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}({})",
            self.name.canonical(),
            self.parameters
                .iter()
                .map(Value::canonical)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl<'src> TryFrom<(Identifier<'src>, Parameters<'src>)>
    for FunctionCall<'src>
{
    type Error = ParseError<'src>;

    fn try_from(
        (name, parameters): (Identifier<'src>, Parameters<'src>),
    ) -> Result<Self, Self::Error> {
        super::validate_function_call(
            name.0,
            parameters
                .len()
                .try_into()
                .map_err(|_| ParseError::TooManyParameters)?,
        )?;
        Ok(Self { name, parameters })
    }
}

/// Parameters to a function call.
pub type Parameters<'src> = Vec<Value<'src>>;

/// A reference to an identifier in the config file
#[derive(Clone, Debug)]
pub struct Identifier<'src>(pub &'src str);

impl Identifier<'_> {
    /// Return the canonical representation of this identifier
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.to_owned()
    }
}

impl<'src> TryFrom<StringToken<'src>> for Identifier<'src> {
    type Error = SpannedErrors<'src>;

    fn try_from(token: StringToken<'src>) -> Result<Self, Self::Error> {
        match token.string_type {
            StringType::Identifier => Ok(Self(token.src)),
            other => Err(ParseError::ExpectedIdentifierToken(other.into())
                .spanned_s(token.src)),
        }
    }
}

/// The type of a string
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringType {
    /// Identifier
    Identifier,
    /// Bare path
    Path,
    /// Bare glob
    BareGlob,
    /// Single quoted string
    QuotedSingle,
    /// Double quoted string
    QuotedDouble,
}

impl fmt::Display for StringType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identifier => "identifier",
            Self::Path => "path",
            Self::BareGlob => "glob",
            Self::QuotedSingle | Self::QuotedDouble => "string",
        })
    }
}

impl TryFrom<TokenType> for StringType {
    type Error = ParseError<'static>;

    fn try_from(value: TokenType) -> Result<Self, Self::Error> {
        match value {
            TokenType::Identifier => Ok(Self::Identifier),
            TokenType::Path => Ok(Self::Path),
            TokenType::BareGlob => Ok(Self::BareGlob),
            TokenType::QuotedSingle => Ok(Self::QuotedSingle),
            TokenType::QuotedDouble => Ok(Self::QuotedDouble),
            other => Err(ParseError::ExpectedStringToken(other)),
        }
    }
}

impl From<StringType> for TokenType {
    fn from(string_type: StringType) -> Self {
        match string_type {
            StringType::Identifier => Self::Identifier,
            StringType::Path => Self::Path,
            StringType::BareGlob => Self::BareGlob,
            StringType::QuotedSingle => Self::QuotedSingle,
            StringType::QuotedDouble => Self::QuotedDouble,
        }
    }
}

/// A reference to a string (string, identifier, path, or glob) in the source
#[derive(Clone, Debug)]
pub struct StringToken<'src> {
    /// The type of string
    pub string_type: StringType,

    /// The slice of the source representing this string
    pub src: &'src str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

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
}
