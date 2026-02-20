//! # Context
//!
//! The context in which actions run — the working directory, the
//! [variable map][VariableMap], and the current templates.
//!
//! [`Context`] has two type aliases:
//!
//!   * [`RequestContext`] is a context that uses [`RequestVariables`] to
//!     extract variables from an HTTP request. It is used by
//!     [`Router`][crate::http::Router].
//!   * [`StaticContext`] is a context that has static variable values stored in
//!     [`StaticVariables`]. It’s useful for testing.

use crate::Error;
use actix_web::HttpRequest;
use handlebars::Handlebars;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Variable names available to be used in configuration.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum Variable {
    /// The cleaned path of the request, not including a query string.
    ///
    /// See [`clean_path()`].
    CleanPath,

    /// The raw path from the request.
    RequestPath,

    /// The HTTP verb used, e.g. `"GET"`.
    Verb,

    /// The `Host` header, or `""` if its invalid or not set.
    Host,
}

/// Access variables containing request information used in configuration.
///
/// These can be interpolated into the configuration, e.g. `/srv/$clean_path`.
pub trait VariableMap<'vars> {
    /// Get a variable value by name.
    ///
    /// ```
    /// use riki::actions::{StaticVariables, Variable, VariableMap};
    ///
    /// let vars = StaticVariables {
    ///     request_path: "/example/",
    ///     clean_path: "/example",
    ///     verb: "POST",
    ///     ..StaticVariables::default()
    /// };
    /// assert_eq!(vars.get("clean_path".try_into().unwrap()), "/example");
    /// assert_eq!(vars.get(Variable::Verb), "POST");
    /// ```
    fn get(&'vars self, variable: Variable) -> &'vars str;

    /// Convenience function to get the clean request path.
    #[inline]
    fn clean_path(&'vars self) -> &'vars str {
        self.get(Variable::CleanPath)
    }

    /// Convenience function to get the raw request path.
    #[inline]
    fn request_path(&'vars self) -> &'vars str {
        self.get(Variable::RequestPath)
    }
}

/// Static variable values (for testing).
#[derive(Clone, Debug)]
pub struct StaticVariables<'vars> {
    /// Cleaned request path
    ///
    /// This should always be clean. See [`clean_path()`].
    pub clean_path: &'vars str,

    /// Raw request path
    pub request_path: &'vars str,

    /// Request verb
    pub verb: &'vars str,

    /// Request host
    pub host: &'vars str,
}

impl<'vars> VariableMap<'vars> for StaticVariables<'vars> {
    fn get(&'vars self, variable: Variable) -> &'vars str {
        match variable {
            Variable::CleanPath => self.clean_path,
            Variable::RequestPath => self.request_path,
            Variable::Verb => self.verb,
            Variable::Host => self.host,
        }
    }
}

impl Default for StaticVariables<'static> {
    /// Default values for [`StaticVariables`].
    ///
    ///   * `clean_path`: `"/"`
    ///   * `request_path`: `"/"`
    ///   * `verb`: `"GET"`
    ///   * `host`: `"localhost"`
    fn default() -> Self {
        Self {
            clean_path: "/",
            request_path: "/",
            verb: "GET",
            host: "localhost",
        }
    }
}

/// Variables from a request object.
#[derive(Clone, Debug)]
pub struct RequestVariables<'vars> {
    /// The request object.
    pub request: &'vars HttpRequest,

    /// The cleaned request path.
    pub path: String,
}

impl<'vars> RequestVariables<'vars> {
    /// Create from an [`HttpRequest`].
    ///
    /// # Errors
    ///
    /// See [`clean_path()`].
    pub fn new(request: &'vars HttpRequest) -> crate::Result<Self> {
        Ok(Self { request, path: clean_path(request.path())? })
    }
}

impl<'vars> VariableMap<'vars> for RequestVariables<'vars> {
    fn get(&'vars self, variable: Variable) -> &'vars str {
        match variable {
            Variable::CleanPath => &self.path,
            Variable::RequestPath => self.request.path(),
            Variable::Verb => self.request.method().as_str(),
            Variable::Host => self
                .request
                .headers()
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
        }
    }
}

/// Static context for actions (for testing).
#[derive(Clone, Debug)]
pub struct Context<'a, V: VariableMap<'a>> {
    /// Working directory.
    pub working_path: PathBuf,

    /// Templates for rendering pages.
    pub tpls: Arc<Handlebars<'a>>,

    /// Variables for interpolation into strings.
    pub variables: V,
}

impl<'a, V: VariableMap<'a>> Context<'a, V> {
    /// Get the file system path for a request path (`path`).
    ///
    /// Request paths always start with `'/'`, but in this case they are
    /// evaluated as relative paths.
    ///
    /// This assumes that `path` does not have any `/../` components.
    pub fn real_path<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path: &Path = path.as_ref();
        self.working_path
            .join(path.strip_prefix("/").unwrap_or(path))
    }
}

impl<'a, V: VariableMap<'a> + Default> Default for Context<'a, V> {
    /// Quick default context.
    ///
    ///   * `working_path` will be `""`.
    ///   * `tpls` will be empty.
    ///   * `variables` will be the type default — likely
    ///     [`StaticVariables::default()`].
    fn default() -> Self {
        Self {
            working_path: PathBuf::new(),
            tpls: Arc::new(Handlebars::default()),
            variables: Default::default(),
        }
    }
}

impl<'a> Context<'a, RequestVariables<'a>> {
    /// Get a new context for a request.
    ///
    /// # Errors
    ///
    /// See [`clean_path()`].
    pub fn new(
        working_path: PathBuf,
        tpls: Arc<Handlebars<'a>>,
        request: &'a HttpRequest,
    ) -> crate::Result<Self> {
        Ok(Self {
            working_path,
            tpls,
            variables: RequestVariables::new(request)?,
        })
    }
}

/// Convenient alias for a context with static variables.
pub type StaticContext = Context<'static, StaticVariables<'static>>;

/// Convenient alias for a context with variables from a request.
pub type RequestContext<'a> = Context<'a, RequestVariables<'a>>;

/// Get a clean path from the request path.
///
/// # Errors
///
///   * [`Error::RequestPathNotAbsolute`] if the path doesn’t start with '/'.
///   * [`Error::RequestPathContainsDotDot`] if the path contains a .. segment.
pub fn clean_path(path: &str) -> crate::Result<String> {
    // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
    if !path.starts_with('/') {
        Err(Error::RequestPathNotAbsolute(path.to_owned()))
    } else if path.split('/').any(|v| v == "..") {
        Err(Error::RequestPathContainsDotDot(path.to_owned()))
    } else {
        // This guarantees the returned path:
        //   * either is "/" or doesn’t end with '/'
        //   * doesn’t contain any "" or "." segments
        #[expect(clippy::comparison_to_empty, reason = "clarity")]
        Ok(format!(
            "/{}",
            path.split('/')
                .filter(|part| *part != "." && *part != "")
                .collect::<Vec<_>>()
                .join("/")
        ))
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    use assert2::check;

    /// For easier comparisons.
    fn wrapped_clean_path(path: &str) -> Result<String, String> {
        clean_path(path).map_err(|error| error.to_string())
    }

    /// Convenience; for easier comparisons.
    #[expect(clippy::unnecessary_wraps, reason = "convenient comparisons")]
    fn ok(value: &str) -> Result<String, String> {
        Ok(value.to_owned())
    }

    /// Convenience; for easier comparisons.
    fn err(value: &str) -> Result<String, String> {
        Err(value.to_owned())
    }

    #[test_log::test]
    fn clean_path_file() {
        check!(wrapped_clean_path("/foo") == ok("/foo"));
        check!(wrapped_clean_path("/a/b") == ok("/a/b"));
    }

    #[test_log::test]
    fn clean_path_dir() {
        check!(wrapped_clean_path("/dir/") == ok("/dir"));
        check!(wrapped_clean_path("/a/b/") == ok("/a/b"));
    }

    #[test_log::test]
    fn clean_path_root_self() {
        check!(wrapped_clean_path("/") == ok("/"));
        check!(wrapped_clean_path("/.") == ok("/"));
        check!(wrapped_clean_path("/./") == ok("/"));
        check!(wrapped_clean_path("/./.") == ok("/"));
        check!(wrapped_clean_path("/././") == ok("/"));
    }

    #[test_log::test]
    fn clean_path_root_multi_slash() {
        check!(wrapped_clean_path("//") == ok("/"));
        check!(wrapped_clean_path("/.//") == ok("/"));
        check!(wrapped_clean_path("//./") == ok("/"));
        check!(wrapped_clean_path("///") == ok("/"));
    }

    #[test_log::test]
    fn clean_path_errors() {
        check!(
            wrapped_clean_path("/../a")
                == err("Request path \"/../a\" contained \"..\" segment")
        );
        check!(
            wrapped_clean_path("a")
                == err("Request path \"a\" did not start with '/'")
        );
        check!(
            wrapped_clean_path("")
                == err("Request path \"\" did not start with '/'")
        );
    }
}
