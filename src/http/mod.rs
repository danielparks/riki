//! # Serve pages over HTTP
//!
//! [`path_handler()`][path_handler] first checks if the URL path corresponds to
//! something in the static directory, then it checks the pages. If nothing is
//! found it renders the error404 template.
//!
//! ## Canonical URLs and redirects
//!
//! Riki redirects to the canonical URL of a page when possible.
//!
//! The canonical URL will end with a / if (and only if) it corresponds to a
//! `index.html`-like page or static file.
//!
//! | Source path             | Canonical path   |
//! |-------------------------|------------------|
//! | `pages/page.md`         | `/page`          |
//! | `pages/dir/index.md`    | `/dir/`          |
//! | `static/static.html`    | `/static.html`   |
//! | `static/dir/index.html` | `/dir/`          |

pub mod functions;
mod tests;
pub mod util;

use crate::actions::{self, Context, PathReturn, Return, StaticContext};
use crate::render::templates_from_directory;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use functions::{markdown_to_html, redact_source, render};
use handlebars::Handlebars;
use std::fmt;
use std::path::PathBuf;
use tracing;
use tracing_actix_web::TracingLogger;

// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode

/// Main entry point for serving over HTTP
///
/// # Errors
///
/// May return an error if the server could not start correctly.
#[actix_web::main]
pub async fn serve<S: AsRef<str>>(
    config: Configuration,
    address: S,
) -> crate::Result<()> {
    let address = address.as_ref();

    util::check_dir(&config.root_path)?;

    let router = Data::new(Router::try_from_configuration(config)?);

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&router))
            .wrap(TracingLogger::default())
            .service(path_handler)
    })
    .bind(address)
    .map_err(|error| crate::Error::BindError {
        source: error,
        address: String::from(address),
    })?
    .run()
    .await
    .map_err(crate::Error::Io)
}

/// Handle all GET requests
#[expect(clippy::future_not_send, reason = "Required by Actix")]
#[get("/{path:.*}")]
pub async fn path_handler(
    req: HttpRequest,
    router: Data<Router<StaticContext<'_>>>,
) -> impl Responder {
    match RequestPath::new(req.path(), &req) {
        Ok(path) => router.route(path).await,
        Err(error) => Err(error),
    }
    .unwrap_or_else(|error: actions::Error| {
        tracing::error!("{}: {error:?}", req.path());
        error.render(&req, router.context().tpls())
    })
}

/// Route requests to the right actions
pub struct Router<C: Context> {
    /// Context for actions
    context: C,
}

impl<'a> Router<StaticContext<'a>> {
    /// Create a new router.
    #[must_use]
    pub const fn new(tpls: Handlebars<'a>, working_path: PathBuf) -> Self {
        Self { context: StaticContext { working_path, tpls } }
    }

    /// Create a router from a [`Configuration`].
    ///
    /// # Errors
    ///
    /// Returns an error if there is a problem loading templates.
    pub fn try_from_configuration(
        config: Configuration,
    ) -> crate::Result<Self> {
        let tpls = templates_from_directory(&config.templates_path)?;
        Ok(Self::new(tpls, config.root_path))
    }
}

impl<C: Context> Router<C> {
    /// Get the context
    #[must_use]
    pub const fn context(&self) -> &C {
        &self.context
    }

    /// Route a request
    ///
    /// # Hard coded rules
    ///
    ///   * `$path` always starts with `'/'` and never ends with `'/'` unless it
    ///     is literally `"/"`.
    ///   * `canonical(canonical_path)`: redirects to `canonical_path` if
    ///     `req.path` doesn’t exactly match `canonical_path`.
    ///
    /// ```text
    /// *.md {
    ///     canonical($path) # canonical if path doesn't end with /
    ///     redact_source($path)
    /// }
    ///
    /// index.html canonical("${dirname($path)}/") # FIXME interpolation
    /// if file_exists("$path") {
    ///     canonical($path)
    ///     $path
    /// }
    /// if file_exists("$path/index.html") {
    ///     canonical("${path}/")
    ///     $path/index.html
    /// }
    ///
    /// index canonical("${dirname($path)}/") # FIXME interpolation
    /// if file_exists("${path}.md") {
    ///     canonical($path)
    ///     render(markdown("${path}.md"))
    /// }
    /// if file_exists("$path/index.md") {
    ///     canonical("${path}/")
    ///     render(markdown("$path/index.md"))
    /// }
    ///
    /// error(404)
    /// ```
    ///
    /// # Errors
    ///
    /// Returned errors will be converted to appropriate HTTP responses.
    #[expect(clippy::future_not_send, reason = "Actix doesn’t require Send")]
    #[expect(clippy::unused_async, reason = "Required by Actix")]
    pub async fn route(
        &self,
        path: RequestPath<'_>,
    ) -> actions::Result<HttpResponse> {
        if let Some(ret) = path.open(&self.context)? {
            // RULE: *.md {
            //     canonical($path)
            //     redact_source($path)
            // }
            if path.ends_with_ignore_case(".md") {
                path.check_canonical(path.path())?;

                if let Some(ret) = redact_source(&self.context, ret)? {
                    return ret.into_response(path.req);
                }
                // else, fall through.
            } else {
                // RULE: index.html canonical("${dirname($path)}/")
                // if file_exists("$path") {
                //     canonical($path)
                //     $path
                // }
                if path.file_name() == Some("index.html") {
                    // FIXME? this should *always* redirect.
                    path.check_canonical(path.parent_with_slash())?;
                } else {
                    path.check_canonical(path.path())?;
                }
                return ret.into_response(path.req);
            }
        }

        if let Some(ret) = path.join("index.html").open(&self.context)? {
            // RULE: if file_exists("$path/index.html") {
            //     canonical("${path}/")
            //     $path/index.html
            // }
            path.check_canonical(&path.path_with_slash())?;
            return ret.into_response(path.req);
        }

        if let Some(md_path) = path.with_extension(".md")
            && let Some(ret) = md_path.open(&self.context)?
        {
            if path.file_name() == Some("index") {
                // RULE: index canonical("${dirname($path)}/")
                // FIXME? this should *always* redirect.
                path.check_canonical(path.parent_with_slash())?;
            } else if let Some(ret) = markdown_to_html(&self.context, ret)?
                && let Some(ret) = render(&self.context, Some(path.req), ret)?
            {
                // RULE: if file_exists("${path}.md") {
                //     canonical($path)
                //     render(markdown("${path}.md"))
                // }
                path.check_canonical(path.path())?;
                return ret.into_response(path.req);
            }
        }

        if let Some(ret) = path.join("index.md").open(&self.context)?
            && let Some(ret) = markdown_to_html(&self.context, ret)?
            && let Some(ret) = render(&self.context, Some(path.req), ret)?
        {
            // RULE: if file_exists("$path/index.md") {
            //     canonical("${path}/")
            //     render(markdown("$path/index.md"))
            // }
            path.check_canonical(&path.path_with_slash())?;
            return ret.into_response(path.req);
        }

        Err(actions::Error::NotFound)
    }
}

impl<C: Context + fmt::Debug> fmt::Debug for Router<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Router")
            .field("context", &self.context)
            .finish()
    }
}

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Configuration {
    /// The path to the directory containing pages and static assets.
    pub root_path: PathBuf,
    /// The path to the directory containing templates.
    pub templates_path: PathBuf,
}

impl Default for Configuration {
    /// Create a configuration using the default subdirectories names in the
    /// current directory.
    fn default() -> Self {
        Self::default_in(".")
    }
}

impl Configuration {
    /// Create a configuration using the default subdirectories under `root`.
    pub fn default_in<P: Into<PathBuf>>(root: P) -> Self {
        let root: PathBuf = root.into();
        Self { templates_path: root.join("templates"), root_path: root }
    }
}

/// A request for a path
pub struct RequestPath<'a> {
    /// The clean request path. Starts with `'/'`.
    path: String,

    /// The HTTP request.
    pub req: &'a HttpRequest,
    // FIXME add original path to detect when we need a 301 redirect?
}

impl<'a> RequestPath<'a> {
    /// Get a clean path from the request path.
    ///
    /// # Errors
    ///
    ///   * [`actions::Error::InternalString`] if the path doesn’t start with /
    ///     or if the path contains a .. segment.
    ///
    /// This will not return [`actions::Error::RedirectCanonical`] because we
    /// want to check the matching page or static file to ensure there actually
    /// is a canonical path, and to determine what it is (e.g. it might match a
    /// directory and thus end with '/').
    fn clean_path(path: &str) -> actions::Result<String> {
        // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
        if !path.starts_with('/') {
            Err(actions::Error::InternalString(format!(
                "request path {path:?} does not start with /"
            )))
        } else if path.split('/').any(|v| v == "..") {
            Err(actions::Error::InternalString(format!(
                "request path {path:?} contains .."
            )))
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

    /// Create a new, clean request path.
    ///
    /// # Errors
    ///
    ///   * [`actions::Error::InternalString`] if the path doesn’t start with /
    ///     or if the path contains a .. segment.
    pub fn new(path: &str, req: &'a HttpRequest) -> actions::Result<Self> {
        let path = Self::clean_path(path)?;
        Ok(Self { path, req })
    }

    /// Try to open the path as a file.
    ///
    /// # Returns
    ///
    ///   * `Ok(Some(ret))` for file
    ///   * `Ok(None)` if the file is not found
    ///   * <code>Err([actions::Error])</code> for other IO errors.
    fn open<C: Context>(
        &self,
        context: &C,
    ) -> actions::Result<Option<PathReturn>> {
        // Convert not found error to `Ok(None)` indicating that we should try
        // the next rule.
        match PathReturn::new(context.working_path().join(&self.path[1..])) {
            Ok(ret) => Ok(Some(ret)),
            Err(error) if actions::is_not_found(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Redirect to a canonical path if necessary.
    ///
    /// # Errors
    ///
    /// Returns [`actions::Error::RedirectCanonical`] if a redirect is required, and
    /// `Ok(())` otherwise.
    fn check_canonical(&self, canonical: &str) -> actions::Result<()> {
        if self.req.path() == canonical {
            Ok(())
        } else {
            Err(actions::Error::RedirectCanonical(canonical.to_owned()))
        }
    }

    /// Get the path string.
    fn path(&self) -> &str {
        &self.path
    }

    /// Get the path string with a trailing slash.
    fn path_with_slash(&self) -> String {
        if self.path.ends_with('/') {
            self.path.clone()
        } else {
            format!("{}/", &self.path)
        }
    }

    /// Get the file name portion of the path.
    ///
    /// Returns `None` if there is no filename, e.g. for `"/"`. Never returns
    /// an empty string.
    fn file_name(&self) -> Option<&str> {
        self.path.rsplit('/').next().filter(|name| !name.is_empty())
    }

    /// Get the parent directory of the path with a trailing slash.
    ///
    /// # Returns
    ///
    ///   * For `/` or `/dir`, returns `"/"`.
    ///   * For `/dir/file`, returns `"/dir/"`.
    fn parent_with_slash(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..=i],
            None => unreachable!("path must start with /"),
        }
    }

    /// Does the path end with `suffix` (ignoring case)?
    fn ends_with_ignore_case(&self, suffix: &str) -> bool {
        // FIXME should this handle paths like "/.md" specially?
        self.path
            .len()
            .checked_sub(suffix.len())
            .and_then(|difference| self.path.get(difference..))
            .map(|real_suffix| real_suffix.eq_ignore_ascii_case(suffix))
            .unwrap_or(false)
    }

    /// Join the paths
    fn join(&self, other: &str) -> Self {
        let path = match (self.path.ends_with('/'), other.starts_with('/')) {
            (true, true) => {
                format!("{}{}", &self.path.trim_end_matches('/'), other)
            }
            (true, false) | (false, true) => format!("{}{}", &self.path, other),
            (false, false) => format!("{}/{}", &self.path, other),
        };

        Self { path, req: self.req }
    }

    /// Try to add an extension to the path.
    ///
    /// Returns `None` if the path ends with a `'/'`.
    fn with_extension(&self, suffix: &str) -> Option<Self> {
        if self.path.ends_with('/') {
            None
        } else {
            let path = &self.path;
            Some(RequestPath { path: format!("{path}{suffix}"), req: self.req })
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    use assert2::check;

    /// For easier comparisons.
    fn wrapped_clean_path(path: &str) -> Result<String, String> {
        RequestPath::clean_path(path).map_err(|error| match error {
            actions::Error::InternalString(msg) => msg,
            other => panic!("unexpected error: {other:?}"),
        })
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

    #[test]
    fn clean_path_file() {
        check!(wrapped_clean_path("/foo") == ok("/foo"));
        check!(wrapped_clean_path("/a/b") == ok("/a/b"));
    }

    #[test]
    fn clean_path_dir() {
        check!(wrapped_clean_path("/dir/") == ok("/dir"));
        check!(wrapped_clean_path("/a/b/") == ok("/a/b"));
    }

    #[test]
    fn clean_path_root_self() {
        check!(wrapped_clean_path("/") == ok("/"));
        check!(wrapped_clean_path("/.") == ok("/"));
        check!(wrapped_clean_path("/./") == ok("/"));
        check!(wrapped_clean_path("/./.") == ok("/"));
        check!(wrapped_clean_path("/././") == ok("/"));
    }

    #[test]
    fn clean_path_root_multi_slash() {
        check!(wrapped_clean_path("//") == ok("/"));
        check!(wrapped_clean_path("/.//") == ok("/"));
        check!(wrapped_clean_path("//./") == ok("/"));
        check!(wrapped_clean_path("///") == ok("/"));
    }

    #[test]
    fn clean_path_errors() {
        check!(
            wrapped_clean_path("/../a")
                == err("request path \"/../a\" contains ..")
        );
        check!(
            wrapped_clean_path("a")
                == err("request path \"a\" does not start with /")
        );
        check!(
            wrapped_clean_path("")
                == err("request path \"\" does not start with /")
        );
    }
}
