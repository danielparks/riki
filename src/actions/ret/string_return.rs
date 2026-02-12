//! A return of a short string. Generally an input to an action.

use super::{
    ActionReturn, ContentReturn, Context, PathReturn, RequestContext, Result,
    Return, VariableMap,
};
use actix_web::HttpResponse;
use os_str_bytes::OsStrBytesExt;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// A short string.
///
/// This can contain non-Unicode characters on UNIX, at least. It is designed to
/// be convertible to and from [`PathBuf`].
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
    #[must_use]
    #[inline]
    pub fn ends_with<S: AsRef<OsStr>>(&mut self, suffix: S) -> bool {
        let suffix = suffix.as_ref();
        self.0
            .as_encoded_bytes()
            .ends_with(suffix.as_encoded_bytes())
    }

    /// Append `suffix` to the value.
    pub fn append<S: AsRef<OsStr>>(&mut self, suffix: S) -> &Self {
        let suffix = suffix.as_ref();
        self.0.push(suffix);
        self
    }

    /// Append `suffix` if this doesn’t already end with it.
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

    /// Strip everything from the last `'/'` on.
    ///
    /// If there are no `'/'`s, this returns `""`.
    #[must_use]
    pub fn into_dirname(mut self) -> Self {
        self.0 = PathBuf::from(self.0)
            .parent()
            .map(OsString::from)
            .unwrap_or_default();
        self
    }

    /// Append `suffix` after a `'/'`.
    ///
    /// Ensures there is only on `'/'`.
    ///
    /// Note that if `suffix` starts with `'/'`, this will still append it.
    #[must_use]
    pub fn into_joined<S: AsRef<OsStr>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();

        if self.ends_with("/") {
            self.append(suffix.strip_prefix("/").unwrap_or(suffix));
        } else if suffix.starts_with("/") {
            self.append(suffix);
        } else {
            self.append("/");
            self.append(suffix);
        }
        self
    }
}

impl Return for StringReturn {
    fn ensure_file<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ActionReturn> {
        Ok(PathReturn::new(self.into(), context)?.into())
    }

    fn into_string_return(self) -> Result<StringReturn> {
        Ok(self)
    }

    fn into_content_return<'a, V: VariableMap<'a>>(
        self,
        context: &'a Context<'a, V>,
    ) -> Result<ContentReturn> {
        PathReturn::new(self.into(), context)?.into_content_return(context)
    }

    fn into_response<'a>(
        self,
        context: &'a RequestContext<'a>,
    ) -> Result<HttpResponse> {
        PathReturn::new(self.into(), context)?.into_response(context)
    }
}

impl AsRef<OsStr> for StringReturn {
    fn as_ref(&self) -> &OsStr {
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

impl PartialEq<&str> for StringReturn {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        *self.0 == **other
    }
}
