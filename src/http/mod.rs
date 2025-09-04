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
pub use crate::http::errors::*;

use crate::errors::{Error, Result};
use crate::pages::{Page, Source};
use crate::templates::templates_from_directory;
use actix_files::NamedFile;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use handlebars::Handlebars;
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use tracing;
use tracing_actix_web::TracingLogger;

// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Configuration {
    /// The path to the directory containing pages.
    pub pages_path: PathBuf,
    /// The path to the directory containing static files.
    pub static_path: PathBuf,
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
        Self {
            pages_path: root.join("pages"),
            static_path: root.join("static"),
            templates_path: root.join("templates"),
        }
    }
}

/// Main entry point for serving over HTTP
///
/// # Errors
///
/// May return an error if the server could not start correctly.
#[actix_web::main]
pub async fn serve<P: AsRef<Path>, S: AsRef<str>>(
    path: P,
    address: S,
) -> Result<()> {
    let address = address.as_ref();
    let path = path.as_ref();
    check_dir(path)?;

    let config = Data::new(Configuration::default_in(path));
    check_dir(&config.pages_path)?;

    let tpls = Data::new(templates_from_directory(&config.templates_path)?);

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&tpls))
            .app_data(Data::clone(&config))
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
#[expect(clippy::future_not_send, reason = "Actix doesn’t require Send")]
#[get("/{path:.*}")]
pub async fn path_handler(
    req: HttpRequest,
    tpls: Data<Handlebars<'_>>,
    config: Data<Configuration>,
) -> impl Responder {
    clean_path(req.path())
        .and_then(|path| {
            match render_static(&req, &config.static_path, &path) {
                Err(WebError::NotFound) => {
                    tracing::trace!("static not found, trying page");
                    render_page(&req, &config.pages_path, &path, &tpls)
                }
                other => other,
            }
        })
        .unwrap_or_else(|error: WebError| {
            tracing::error!("{}: {error:?}", req.path());
            error.render(&req, &tpls)
        })
}

/// Get a clean path request path.
///
/// # Errors
///
///   * [`WebError::InternalString`] if the path doesn’t start with / or if the
///     path contains a .. segment.
///
/// This will not return [`WebError::RedirectCanonical`] because we want to
/// check the matching page or static file to ensure there actually is a
/// canonical path, and to determine what it is (e.g. it might match a directory
/// and thus end with '/').
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

/// Return a static file as an [`HttpResponse`].
///
/// # Errors
///
///   * [`WebError`] if there was a problem opening or reading the file.
///   * [`WebError::RedirectCanonical`] if the canonical path for the file does
///     not match the request path.
fn render_static(
    req: &HttpRequest,
    static_path: &Path,
    clean_path: &str,
) -> WebResult<HttpResponse> {
    Ok(match open_static_file(req, static_path, clean_path) {
        Err(WebError::NotFound) => {
            let index_clean_path = if clean_path.ends_with('/') {
                format!("{clean_path}index.html")
            } else {
                format!("{clean_path}/index.html")
            };
            open_static_file(req, static_path, &index_clean_path)
        }
        other => other,
    }?
    .into_response(req))
}

/// Open a file path as a [`NamedFile`].
///
/// This reads one byte from the file to check if it’s actually a directory.
///
/// # Errors
///
///   * [`WebError`] if there was a problem opening or reading the file.
///   * [`WebError::RedirectCanonical`] if the canonical path for the file does
///     not match the request path.
fn open_static_file(
    req: &HttpRequest,
    root: &Path,
    clean_path: &str,
) -> WebResult<NamedFile> {
    let path = root.join(&clean_path[1..]);
    let file = open_confirmed_file(&path)?;

    let canonical_path = if clean_path.ends_with("/index.html") {
        clean_path.strip_suffix("index.html").unwrap()
    } else {
        clean_path
    };

    if req.path() == canonical_path {
        Ok(NamedFile::from_file(file, path)?)
    } else {
        Err(WebError::RedirectCanonical(canonical_path.to_owned()))
    }
}

/// Render a page to be served over HTTP.
///
/// # Errors
///
///   * [`WebError`] if the page cannot be read.
///   * [`WebError::RedirectCanonical`] if the canonical path for the page does
///     not match the request path.
fn render_page(
    req: &HttpRequest,
    root: &Path,
    clean_path: &str,
    tpls: &Handlebars<'_>,
) -> WebResult<HttpResponse> {
    let page = match read_page_file(req, root, clean_path) {
        Err(WebError::NotFound) => {
            let index_clean_path = if clean_path.ends_with('/') {
                format!("{clean_path}index")
            } else {
                format!("{clean_path}/index")
            };
            read_page_file(req, root, &index_clean_path)
        }
        other => other,
    }?;

    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(page.render_to_string(tpls)?))
}

/// Read a file page.
///
/// # Errors
///
///   * [`WebError`] if the page cannot be read.
///   * [`WebError::RedirectCanonical`] if the canonical path for the page does
///     not match the request path.
fn read_page_file(
    req: &HttpRequest,
    root: &Path,
    clean_path: &str,
) -> WebResult<Page> {
    let path = root.join(format!("{}.md", &clean_path[1..]));
    let mut file = open_confirmed_file(&path)?;

    let canonical_path = if clean_path.ends_with("/index") {
        clean_path.strip_suffix("index").unwrap()
    } else {
        clean_path
    };

    if req.path() == canonical_path {
        let mut content = String::new();
        #[expect(
            clippy::verbose_file_reads,
            reason = "need to open file before checking path"
        )]
        file.read_to_string(&mut content)?;
        Page::from_source(Source::from_path(path), content)
            .map_err(WebError::Internal)
    } else {
        Err(WebError::RedirectCanonical(canonical_path.to_owned()))
    }
}

/// Check that path is a directory or a symlink that resolves to a directory.
///
/// # Errors
///
///   * [`Error::MissingDirectory`] not a directory or doesn’t exist.
///   * [`Error::Io`] some other problem getting info about `path`.
fn check_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    match path.metadata().map(|m| m.is_dir()) {
        Ok(true) => Ok(()),
        Err(error) if !is_not_found(&error) => Err(Error::Io(error)),
        _ => Err(Error::MissingDirectory(path.to_path_buf())),
    }
}

/// Open a file and confirm that it is a file.
///
/// This reads one byte to check if the file is a directory (using `is_dir()`
/// would create a race condition.)
///
/// Returns the opened file (rewound).
///
/// # Errors
///
///   * [`io::Error`] resulting from opening the file, reading a byte, or
///     seeking to the start of the file.
fn open_confirmed_file(path: &Path) -> io::Result<fs::File> {
    let mut file = fs::File::open(path)?;
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert2::check;

    /// For easier comparisons.
    fn wrapped_clean_path(path: &str) -> Result<String, String> {
        clean_path(path).map_err(|error| match error {
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
