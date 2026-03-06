//! A return of a short string. Generally an input to an action.

use super::{
    ActionReturn, ContentReturn, Context, RealFileReturn, RequestContext,
    Result, Return, VariableMap,
};
use axum::response::Response;
use std::path::{Path, PathBuf};

/// A short string.
///
/// We don’t handle non-UTF-8 URLs and only support Unicode paths, so this uses
/// a [`String`] internally.
#[derive(Debug)]
pub struct StringReturn(String);

impl StringReturn {
    /// Check if this ends with the string `suffix`.
    ///
    /// ```
    /// use assert2::check;
    /// use riki_actions::StringReturn;
    /// check!(StringReturn::from("ab").ends_with("b"));
    /// check!(!StringReturn::from("ab").ends_with("c"));
    /// ```
    #[must_use]
    #[inline]
    pub fn ends_with<S: AsRef<str>>(&mut self, suffix: S) -> bool {
        self.0.ends_with(suffix.as_ref())
    }

    /// Append `suffix` to the value.
    ///
    /// ```
    /// use assert2::check;
    /// use riki_actions::StringReturn;
    /// check!(StringReturn::from("/a/b").append(".md") == "/a/b.md");
    /// ```
    #[inline]
    pub fn append<S: AsRef<str>>(&mut self, suffix: S) -> &Self {
        self.0.push_str(suffix.as_ref());
        self
    }

    /// Append `suffix` if this doesn’t already end with it.
    ///
    /// ```
    /// use assert2::check;
    /// use riki_actions::StringReturn;
    /// check!(StringReturn::from("a").into_ending_with(".md") == "a.md");
    /// check!(StringReturn::from("a.md").into_ending_with(".md") == "a.md");
    /// check!(StringReturn::from("a.").into_ending_with(".md") == "a..md");
    ///
    /// check!(StringReturn::from("a").into_ending_with("/") == "a/");
    /// ```
    #[must_use]
    pub fn into_ending_with<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();
        if !self.ends_with(suffix) {
            self.append(suffix);
        }
        self
    }

    /// Append `suffix` to the value.
    #[must_use]
    #[inline]
    pub fn into_appended<S: AsRef<str>>(mut self, suffix: S) -> Self {
        self.append(suffix);
        self
    }

    /// Get the directory of `self` if it were a path.
    ///
    /// ```
    /// use assert2::check;
    /// use riki_actions::StringReturn;
    ///
    /// check!(StringReturn::from("/").into_dirname() == "/");
    /// check!(StringReturn::from("//").into_dirname() == "/");
    /// check!(StringReturn::from("///").into_dirname() == "/");
    /// check!(StringReturn::from("a").into_dirname() == "");
    /// check!(StringReturn::from("/a").into_dirname() == "/");
    /// check!(StringReturn::from("a/b").into_dirname() == "a");
    /// check!(StringReturn::from("a//b").into_dirname() == "a");
    /// check!(StringReturn::from("a/").into_dirname() == "a");
    /// check!(StringReturn::from("a//").into_dirname() == "a");
    /// ```
    #[must_use]
    pub fn into_dirname(mut self) -> Self {
        let Some(last) = self.0.rfind('/') else {
            self.0.clear();
            return self;
        };

        let trimmed_dir = self.0.split_at(last).0.trim_end_matches('/');
        if trimmed_dir.is_empty() {
            // self started with at least one slash, then had a single segment.
            self.0.truncate(1);
            return self;
        }

        self.0.truncate(trimmed_dir.len());
        self
    }

    /// Append `suffix` after exactly one `'/'`.
    ///
    /// Note that if `suffix` starts with `'/'`, this will still append it.
    ///
    /// ```
    /// use assert2::check;
    /// use riki_actions::StringReturn;
    ///
    /// check!(StringReturn::from("a").into_joined("b") == "a/b");
    /// check!(StringReturn::from("a").into_joined("/b") == "a/b");
    /// check!(StringReturn::from("a").into_joined("//b") == "a/b");
    ///
    /// check!(StringReturn::from("a/").into_joined("b") == "a/b");
    /// check!(StringReturn::from("a/").into_joined("/b") == "a/b");
    /// check!(StringReturn::from("a/").into_joined("//b") == "a/b");
    ///
    /// check!(StringReturn::from("a//").into_joined("b") == "a/b");
    /// check!(StringReturn::from("a//").into_joined("/b") == "a/b");
    /// check!(StringReturn::from("a//").into_joined("//b") == "a/b");
    ///
    /// check!(StringReturn::from("a").into_joined("b/") == "a/b/");
    /// ```
    #[must_use]
    pub fn into_joined<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();

        if self.ends_with("//") {
            self.0.truncate(self.0.trim_end_matches('/').len());
            self.append("/");
            self.append(suffix.trim_start_matches('/'));
        } else if self.ends_with("/") {
            // Ends with single slash.
            self.append(suffix.trim_start_matches('/'));
        } else if suffix.starts_with("//") {
            // Ends with no slash and suffix starts with multiple slash.
            self.append("/");
            self.append(suffix.trim_start_matches('/'));
        } else if suffix.starts_with('/') {
            // Ends with no slash; suffix starts with single slash.
            self.append(suffix);
        } else {
            self.append("/");
            self.append(suffix);
        }
        self
    }

    /// Convert the return to a [`RealFileReturn`].
    ///
    /// # Errors
    ///
    /// Returns an [`Error`][super::Error] if the path represented by `self` was
    /// not a file that could be read.
    fn into_real_file_return<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<RealFileReturn> {
        let ret = RealFileReturn::from_inner_path(self.into(), context)?;
        tracing::trace!("into_real_file_return() -> {ret:?}");
        Ok(ret)
    }
}

impl Return for StringReturn {
    fn inner_path(&self) -> Result<&str> {
        Ok(&self.0)
    }

    fn ensure_file<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<ActionReturn> {
        self.into_real_file_return(context)?.into()
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(self)
    }

    fn into_content_return<V: VariableMap>(
        self,
        context: &Context<V>,
    ) -> Result<ContentReturn> {
        self.into_real_file_return(context)?
            .into_content_return(context)
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<Response> {
        self.into_real_file_return(context)?.into_response(context)
    }
}

impl AsRef<str> for StringReturn {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for StringReturn {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl From<&str> for StringReturn {
    fn from(string: &str) -> Self {
        Self(string.into())
    }
}

impl From<String> for StringReturn {
    fn from(string: String) -> Self {
        Self(string)
    }
}

impl From<StringReturn> for String {
    fn from(ret: StringReturn) -> Self {
        ret.0
    }
}

impl From<StringReturn> for PathBuf {
    fn from(ret: StringReturn) -> Self {
        ret.0.into()
    }
}

impl From<StringReturn> for Result {
    fn from(ret: StringReturn) -> Self {
        Ok(ret.into())
    }
}

impl PartialEq<&str> for StringReturn {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        *self.0 == **other
    }
}

impl PartialEq<str> for StringReturn {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}
