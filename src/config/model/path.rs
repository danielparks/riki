//! Handle path literals in the configuration

use super::ParsedString;
use std::path::PathBuf;

/// A value that will evaluate to a path.
///
/// This allows things like joining `ParsedPath`s together.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedPath<'src>(ParsedString<'src>);

impl ParsedPath<'_> {
    /// Canonical version of path.
    #[must_use]
    #[inline]
    pub fn canonical(&self) -> String {
        self.0.canonical()
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
