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

mod errors;
mod tests;
pub use errors::*;
pub mod util;

use crate::actions::{ContentReturn, MediaType, PathReturn, Return};
use crate::errors::{Error, Result};
use crate::render::elements::{
    self, ElementError, handle_a_email, handle_last_modified,
};
use crate::render::{self, render_source_to_string, templates_from_directory};
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use dom_query::Document;
use handlebars::Handlebars;
use std::mem;
use std::path::PathBuf;
use tracing;
use tracing_actix_web::TracingLogger;

// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode

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

/// Main entry point for serving over HTTP
///
/// # Errors
///
/// May return an error if the server could not start correctly.
#[actix_web::main]
pub async fn serve<S: AsRef<str>>(
    config: Configuration,
    address: S,
) -> Result<()> {
    let address = address.as_ref();

    util::check_dir(&config.root_path)?;

    let router = Data::new(Router::new(
        templates_from_directory(&config.templates_path)?,
        config,
    ));

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&router))
            .wrap(TracingLogger::default())
            .service(path_handler)
    })
    .bind(address)
    .map_err(|error| Error::BindError {
        source: error,
        address: String::from(address),
    })?
    .run()
    .await
    .map_err(Error::Io)
}

/// Handle all GET requests
#[expect(clippy::future_not_send, reason = "Required by Actix")]
#[get("/{path:.*}")]
pub async fn path_handler(
    req: HttpRequest,
    router: Data<Router<'_>>,
) -> impl Responder {
    match RequestPath::new(req.path(), &req) {
        Ok(path) => router.route(path).await,
        Err(error) => Err(error),
    }
    .unwrap_or_else(|error: WebError| {
        tracing::error!("{}: {error:?}", req.path());
        error.render(&req, &router.context().tpls)
    })
}

/// Route requests to the right actions
#[derive(Debug)]
pub struct Router<'a> {
    /// Context for actions
    context: Context<'a>,
}

impl<'a> Router<'a> {
    /// Create a new router.
    #[must_use]
    pub const fn new(tpls: Handlebars<'a>, config: Configuration) -> Self {
        let context = Context { config, tpls };
        Self { context }
    }

    /// Get the context
    #[must_use]
    pub const fn context(&self) -> &Context<'a> {
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
    ) -> WebResult<HttpResponse> {
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

        Err(WebError::NotFound)
    }
}

/// Context for actions
#[derive(Debug, Default)]
pub struct Context<'a> {
    /// Configuration
    pub config: Configuration,

    /// Templates for rendering pages
    pub tpls: Handlebars<'a>,
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
    ///   * [`WebError::InternalString`] if the path doesn’t start with / or if
    ///     the path contains a .. segment.
    ///
    /// This will not return [`WebError::RedirectCanonical`] because we want to
    /// check the matching page or static file to ensure there actually is a
    /// canonical path, and to determine what it is (e.g. it might match a
    /// directory and thus end with '/').
    fn clean_path(path: &str) -> WebResult<String> {
        // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
        if !path.starts_with('/') {
            Err(WebError::InternalString(format!(
                "request path {path:?} does not start with /"
            )))
        } else if path.split('/').any(|v| v == "..") {
            Err(WebError::InternalString(format!(
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
    ///   * [`WebError::InternalString`] if the path doesn’t start with / or if
    ///     the path contains a .. segment.
    pub fn new(path: &str, req: &'a HttpRequest) -> WebResult<Self> {
        let path = Self::clean_path(path)?;
        Ok(Self { path, req })
    }

    /// Try to open the path as a file.
    ///
    /// # Returns
    ///
    ///   * `Ok(Some(ret))` for file
    ///   * `Ok(None)` if the file is not found
    ///   * <code>Err([WebError])</code> for other IO errors.
    fn open(&self, context: &Context) -> WebResult<Option<PathReturn>> {
        // Convert not found error to `Ok(None)` indicating that we should try
        // the next rule.
        match PathReturn::new(context.config.root_path.join(&self.path[1..])) {
            Ok(ret) => Ok(Some(ret)),
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Redirect to a canonical path if necessary.
    ///
    /// # Errors
    ///
    /// Returns [`WebError::RedirectCanonical`] if a redirect is required, and
    /// `Ok(())` otherwise.
    fn check_canonical(&self, canonical: &str) -> WebResult<()> {
        if self.req.path() == canonical {
            Ok(())
        } else {
            Err(WebError::RedirectCanonical(canonical.to_owned()))
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

/// Render passed content in a template.
///
/// # Errors
///
/// Will return [`WebError`] if there is a problem getting content from `ret` or
/// rendering the template.
pub fn render<R: Return>(
    context: &Context<'_>,
    req: Option<&HttpRequest>,
    ret: R,
) -> WebResult<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;

    let template = ret
        .metadata
        .get("template")
        .map(String::as_str)
        .unwrap_or_else(|| "default");

    ret.body.ensure_string()?;
    let document =
        Document::from(context.tpls.render(template, &ret).map_err(
            |error| Error::TemplateRender {
                source: error,
                page_source: Box::new(ret.source.clone()),
            },
        )?);

    let ctx = elements::Context {
        document: &document,
        page: &ret,
        req,
        show_detailed_errors: true,
    };
    for node in document.select("a-email").nodes() {
        if let Err(ElementError(msg)) = handle_a_email(&ctx, node) {
            tracing::error!("Handling <a-email>: {msg}");
            let b = document.tree.new_element("b");
            b.set_text(msg);
            node.replace_with(&b);
        }
    }
    for node in document.select("last-modified").nodes() {
        if let Err(ElementError(msg)) = handle_last_modified(&ctx, node) {
            tracing::error!("Handling <last-modified>: {msg}");
            let b = document.tree.new_element("b");
            b.set_text(msg);
            node.replace_with(&b);
        }
    }

    ret.content_type = MediaType::TEXT_HTML_UTF8;
    ret.body = document.html().into();

    Ok(Some(ret))
}

/// Load metadata and convert body to HTML.
///
/// # Errors
///
/// Will return [`WebError`] if there is a problem getting content from `ret` or
/// parsing page metadata from the content.
pub fn markdown_to_html<R: Return>(
    _context: &Context<'_>,
    ret: R,
) -> WebResult<Option<ContentReturn>> {
    let mut ret = ret.into_content_return()?;
    let raw_page = mem::take(&mut ret.body).into_string()?;
    let (header, body) = render::split_raw_page(&raw_page);

    ret.metadata.extend(
        render::metadata_from_string(header).map_err(crate::Error::from)?,
    );
    ret.body = render::render_markdown(body).into();
    ret.content_type = MediaType::TEXT_HTML_UTF8;
    ret.ensure_metadata_title()?;

    Ok(Some(ret))
}

/// Redact sensitive values from passed Markdown.
///
/// # Errors
///
/// Will return [`WebError`] if there is a problem getting content from `ret`.
pub fn redact_source<R: Return>(
    _context: &Context<'_>,
    ret: R,
) -> WebResult<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;
    ret.body = render_source_to_string(ret.body.into_string()?).into();
    ret.content_type = MediaType::TEXT_MARKDOWN_UTF8;
    Ok(Some(ret))
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    use assert2::check;

    /// For easier comparisons.
    fn wrapped_clean_path(path: &str) -> Result<String, String> {
        RequestPath::clean_path(path).map_err(|error| match error {
            WebError::InternalString(msg) => msg,
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
