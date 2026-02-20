//! Handle path literals in the configuration

use super::{Interpolation, ParsedString};
use crate::actions::VariableMap;
use std::path::PathBuf;

/// A value that will evaluate to a path.
///
/// This allows things like joining `ParsedPath`s together.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedPath<'src>(ParsedString<'src>);

impl<'src> ParsedPath<'src> {
    /// Canonical version of path.
    #[must_use]
    #[inline]
    pub fn canonical(&self) -> String {
        self.0.canonical()
    }

    /// Return the content of this path.
    ///
    /// If this starts with a variable then leading `'/'`s will be removed.
    #[inline]
    pub fn content<'vars, V: VariableMap<'vars>>(
        &self,
        variables: &'vars V,
    ) -> String {
        let path = self.0.content(variables);
        if self.starts_with_variable() {
            // FIXME handle Windows absolute/root paths
            let new_path = path.trim_start_matches('/');
            if new_path == path {
                path
            } else {
                new_path.to_owned()
            }
        } else {
            path
        }
    }

    /// Join another path onto this one.
    ///
    /// If `other` starts with `'/'`, then it will replace `self`. If `other`
    /// starts with a variable, then we consider it a relative path.
    pub fn push(&mut self, other: &Self) {
        if other.starts_with('/') == Some(true) {
            self.0 = other.0.clone();
        } else if self.ends_with('/') == Some(true) {
            self.0.push_string(&other.0);
        } else {
            // A double or triple / doesn’t matter.
            self.0.push('/');
            self.0.push_string(&other.0);
        }
    }

    /// Join two paths together.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        // If other is absolute, and we moved other, this could just return it.
        let mut new = self.clone();
        new.push(other);
        new
    }

    /// Does this start with a variable?
    #[must_use]
    #[inline]
    pub fn starts_with_variable(&self) -> bool {
        self.0.starts_with_variable()
    }

    /// Does this end with a variable?
    #[must_use]
    #[inline]
    pub fn ends_with_variable(&self) -> bool {
        self.0.ends_with_variable()
    }

    /// Does this start with `c`?
    ///
    /// Returns `None` if this starts with a variable, and thus the final start
    /// is unknown.
    #[must_use]
    #[inline]
    fn starts_with(&self, c: char) -> Option<bool> {
        self.0.starts_with(c)
    }

    /// Does this end with `c`?
    ///
    /// Returns `None` if this ends with a variable, and thus the final ending
    /// is unknown.
    #[must_use]
    #[inline]
    fn ends_with(&self, c: char) -> Option<bool> {
        self.0.ends_with(c)
    }

    /// Get the variables used in the path.
    #[must_use]
    pub fn variables(&self) -> &[Interpolation<'src>] {
        self.0.variables()
    }
}

impl<'src> From<ParsedString<'src>> for ParsedPath<'src> {
    fn from(value: ParsedString<'src>) -> Self {
        Self(value)
    }
}

impl<'src> From<&'src str> for ParsedPath<'src> {
    fn from(value: &'src str) -> Self {
        Self(value.into())
    }
}

impl From<PathBuf> for ParsedPath<'_> {
    fn from(value: PathBuf) -> Self {
        Self(value.into())
    }
}

impl<'src> From<ParsedPath<'src>> for ParsedString<'src> {
    fn from(path: ParsedPath<'src>) -> Self {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{StaticVariables, Variable};
    use crate::config::errors::ParseResult;
    use crate::config::parser2::StringType;
    use assert2::{check, let_assert};

    /// Test variables
    const VARS: StaticVariables = StaticVariables {
        request_path: "/abc/",
        verb: "GET",
        host: "example.com",
    };

    /// Create a [`ParsedPath`] easily.
    fn parse_path(s: &str) -> ParseResult<'_, ParsedPath<'_>> {
        ParsedString::from_string_content(s, StringType::QuotedDouble)
            .map(Into::into)
    }

    #[test_log::test]
    fn parse_path_literal_escape() {
        let_assert!(Ok(s) = parse_path(r"\z"));
        check!(s.content(&VARS) == PathBuf::from("z"));

        let_assert!(Ok(s) = parse_path(r"a\zb"));
        check!(s.content(&VARS) == PathBuf::from("azb"));
    }

    #[test_log::test]
    fn parse_path_newline_escape() {
        let_assert!(Ok(s) = parse_path(r"\n"));
        check!(s.content(&VARS) == PathBuf::from("\n"));

        let_assert!(Ok(s) = parse_path(r"a\nb"));
        check!(s.content(&VARS) == PathBuf::from("a\nb"));
    }

    #[test_log::test]
    fn parse_path_just_var_no_braces() {
        let_assert!(Ok(s) = parse_path("$clean_path"));
        check!(s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_just_var_braces() {
        let_assert!(Ok(s) = parse_path("${clean_path}"));
        check!(s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_no_braces() {
        let_assert!(Ok(s) = parse_path("$clean_path/foo"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('o') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_braces() {
        let_assert!(Ok(s) = parse_path("${clean_path}bar"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('r') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_no_braces_c() {
        let_assert!(Ok(s) = parse_path("$clean_path/"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(true));
        check!(s.ends_with('o') == Some(false));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_braces_c() {
        let_assert!(Ok(s) = parse_path("${clean_path}/"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(true));
        check!(s.ends_with('o') == Some(false));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_no_braces_escape() {
        let_assert!(Ok(s) = parse_path(r"$clean_path\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_no_braces_escape2() {
        let_assert!(Ok(s) = parse_path(r"$clean_path\z\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_braces_escape() {
        let_assert!(Ok(s) = parse_path(r"${clean_path}\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_start_var_braces_escape2() {
        let_assert!(Ok(s) = parse_path(r"${clean_path}\z\z"));
        check!(s.starts_with_variable());
        check!(!s.ends_with_variable());
        check!(s.starts_with('/') == None);
        check!(s.ends_with('/') == Some(false));
        check!(s.ends_with('z') == Some(true));
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_end_var_no_braces() {
        let_assert!(Ok(s) = parse_path("/foo/$clean_path"));
        check!(!s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == Some(true));
        check!(s.starts_with('o') == Some(false));
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn parse_path_end_var_braces() {
        let_assert!(Ok(s) = parse_path("/foo/${clean_path}"));
        check!(!s.starts_with_variable());
        check!(s.ends_with_variable());
        check!(s.starts_with('/') == Some(true));
        check!(s.starts_with('o') == Some(false));
        check!(s.ends_with('/') == None);
        check!(s.variables().len() == 1);
        check!(s.variables()[0].variable == Variable::CleanPath);
    }

    #[test_log::test]
    fn push_c() {
        let mut s = parse_path("/foo/${clean_path}").unwrap();
        s.push(&parse_path("Z").unwrap());
        check!(s.ends_with('Z') == Some(true));
    }

    #[test_log::test]
    fn push_no_vars() {
        let mut a = parse_path("abc").unwrap();
        a.push(&parse_path("def").unwrap());
        check!(a.content(&VARS) == PathBuf::from("abc/def"));
        check!(a.variables().is_empty());
    }

    #[test_log::test]
    fn push_both_vars() {
        let mut a = parse_path("/foo/${clean_path}").unwrap();
        a.push(&parse_path("${clean_path}/foo").unwrap());
        check!(a.canonical() == r#""/foo/${clean_path}/${clean_path}/foo""#);
        check!(
            a.variables().iter().map(|i| i.variable).collect::<Vec<_>>()
                == [Variable::CleanPath, Variable::CleanPath]
        );
    }

    #[test_log::test]
    fn push_b_var() {
        let mut a = parse_path("/foo/").unwrap();
        a.push(&parse_path("${clean_path}/foo").unwrap());
        check!(a.canonical() == r#""/foo/${clean_path}/foo""#);
        check!(
            a.variables().iter().map(|i| i.variable).collect::<Vec<_>>()
                == [Variable::CleanPath]
        );
    }

    #[test_log::test]
    fn push_absolute() {
        let mut a = parse_path("/foo/").unwrap();
        a.push(&parse_path("/bar").unwrap());
        check!(a.canonical() == r#""/bar""#);
        check!(a.variables().is_empty());
    }

    #[test_log::test]
    fn path_content() {
        let path = parse_path("/foo/").unwrap();
        check!(path.content(&VARS) == PathBuf::from("/foo/"));
    }

    #[test_log::test]
    fn path_content_1_var() {
        let string = parse_path("/foo/${clean_path}/").unwrap();
        check!(string.content(&VARS) == PathBuf::from("/foo//abc/"));
    }

    #[test_log::test]
    fn path_content_2_vars() {
        let string = parse_path("/foo/$verb${clean_path}").unwrap();
        check!(string.content(&VARS) == PathBuf::from("/foo/GET/abc"));
    }

    #[test_log::test]
    fn path_content_3_vars() {
        let string = parse_path("${host}/$verb/$clean_path").unwrap();
        check!(string.content(&VARS) == PathBuf::from("example.com/GET//abc"));
    }

    #[test_log::test]
    fn path_content_start_path_var() {
        let string = parse_path("$clean_path/foo").unwrap();
        check!(string.content(&VARS) == PathBuf::from("abc/foo"));
    }
}
