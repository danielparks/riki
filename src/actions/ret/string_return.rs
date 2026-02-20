//! A return of a short string. Generally an input to an action.

use super::{
    ActionReturn, ContentReturn, Context, RealFileReturn, RequestContext,
    Result, Return, VariableMap,
};
use actix_web::HttpResponse;
use os_str_bytes::OsStrBytesExt;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// A short string.
///
/// Internally this holds an [`OsString`], so it can contain non-UTF-8
/// characters on UNIX, at least. It is designed to be losslessly convertible to
/// and from [`PathBuf`].
#[derive(Debug)]
pub struct StringReturn(OsString);

impl StringReturn {
    /// Get the value as a `&str`.
    ///
    /// Returns `None` if this contains invalid UTF-8.
    #[must_use]
    pub fn to_str(&self) -> Option<&str> {
        self.0.to_str()
    }

    /// Check if this ends with the string `suffix`.
    ///
    /// ```
    /// use assert2::check;
    /// use riki::actions::StringReturn;
    /// check!(StringReturn::from("ab").ends_with("b"));
    /// check!(!StringReturn::from("ab").ends_with("c"));
    /// ```
    #[must_use]
    #[inline]
    pub fn ends_with<S: AsRef<OsStr>>(&mut self, suffix: S) -> bool {
        self.0.ends_with_os(suffix.as_ref())
    }

    /// Append `suffix` to the value.
    ///
    /// ```
    /// use assert2::check;
    /// use riki::actions::StringReturn;
    /// check!(StringReturn::from("/a/b").append(".md") == "/a/b.md");
    /// ```
    pub fn append<S: AsRef<OsStr>>(&mut self, suffix: S) -> &Self {
        self.0.push(suffix);
        self
    }

    /// Append `suffix` if this doesn’t already end with it.
    ///
    /// ```
    /// use assert2::check;
    /// use riki::actions::StringReturn;
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
    pub fn into_appended<S: AsRef<OsStr>>(mut self, suffix: S) -> Self {
        self.append(suffix);
        self
    }

    /// Get the directory of `self` if it were a path.
    ///
    /// ```
    /// use assert2::check;
    /// use riki::actions::StringReturn;
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

        // FIXME avoid allocating when OsString::truncate() stabilizes.
        let trimmed_dir = self.0.split_at(last).0.trim_end_matches('/');
        if trimmed_dir.is_empty() {
            // self started with at least one slash, then had a single segment.
            self.0 = OsString::from("/");
            return self;
        }

        self.0 = trimmed_dir.to_owned();
        self
    }

    /// Append `suffix` after exactly one `'/'`.
    ///
    /// Note that if `suffix` starts with `'/'`, this will still append it.
    ///
    /// ```
    /// use assert2::check;
    /// use riki::actions::StringReturn;
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
    pub fn into_joined<S: AsRef<OsStr>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();

        // FIXME avoid allocating when OsString::truncate() stabilizes.
        if self.ends_with("//") {
            self.0 = self.0.trim_end_matches('/').to_os_string();
            self.append("/");
            self.append(suffix.trim_start_matches('/'));
        } else if self.ends_with("/") {
            // Ends with single slash.
            self.append(suffix.trim_start_matches('/'));
        } else if suffix.starts_with("//") {
            // Ends with no slash and suffix starts with multiple slash.
            self.append("/");
            self.append(suffix.trim_start_matches('/'));
        } else if suffix.starts_with("/") {
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
    fn into_real_file_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<RealFileReturn> {
        Ok(RealFileReturn::new(self.into(), context)?)
    }
}

impl Return for StringReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        self.into_real_file_return(context)?.into()
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(self)
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        self.into_real_file_return(context)?
            .into_content_return(context)
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        self.into_real_file_return(context)?.into_response(context)
    }
}

impl AsRef<OsStr> for StringReturn {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
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
        Self(string.into())
    }
}

impl From<OsString> for StringReturn {
    fn from(string: OsString) -> Self {
        Self(string)
    }
}

impl From<PathBuf> for StringReturn {
    fn from(path: PathBuf) -> Self {
        Self(path.into_os_string())
    }
}

impl TryFrom<StringReturn> for String {
    type Error = crate::NotUtf8;

    fn try_from(ret: StringReturn) -> Result<Self, Self::Error> {
        ret.0.into_string().map_err(crate::NotUtf8::OsString)
    }
}

impl From<StringReturn> for OsString {
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
