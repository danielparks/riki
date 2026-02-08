//! A return of a short string. Generally an input to an action.

use super::{
    ActionReturn, ContentReturn, Context, PathReturn, RequestContext, Result,
    Return, VariableMap,
};
use actix_web::HttpResponse;
use std::path::PathBuf;

/// A short string.
#[derive(Debug)]
pub struct StringReturn(String);

impl StringReturn {
    /// Get the value as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Append `suffix` if this doesn’t already end with it.
    #[must_use]
    pub fn ensure_ends_with<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();
        if !self.0.ends_with(suffix) {
            self.0.push_str(suffix);
        }
        self
    }

    /// Append `suffix` to the value.
    #[must_use]
    pub fn append<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();
        self.0.push_str(suffix);
        self
    }

    /// Strip everything from the last `'/'` on.
    ///
    /// If there are no `'/'`s, this returns `""`.
    #[must_use]
    pub fn dirname(mut self) -> Self {
        self.0.truncate(self.0.rfind('/').unwrap_or_default());
        self
    }

    /// Append `suffix` after a `'/'`.
    ///
    /// Ensures there is only on `'/'`.
    ///
    /// Note that if `suffix` starts with `'/'`, this will still append it.
    #[must_use]
    pub fn join<S: AsRef<str>>(mut self, suffix: S) -> Self {
        let suffix = suffix.as_ref();

        if self.0.ends_with('/') {
            if let Some(stripped) = suffix.strip_prefix('/') {
                self.0.push_str(stripped);
            } else {
                self.0.push_str(suffix);
            }
        } else if suffix.starts_with('/') {
            self.0.push_str(suffix);
        } else {
            self.0.push('/');
            self.0.push_str(suffix);
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

impl From<&str> for StringReturn {
    fn from(string: &str) -> Self {
        Self(string.to_owned())
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

impl AsRef<str> for StringReturn {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
