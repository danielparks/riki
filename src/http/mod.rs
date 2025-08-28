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
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
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

/// Get a relative path that can be joined to another path safely.
///
/// # Errors
///
///   * [`WebError::InternalString`] if the path doesn’t start with / or if the
///     path contains a .. segment.
fn clean_path(path: &str) -> WebResult<String> {
    // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
    if !path.starts_with('/') {
        Err(WebError::InternalString(format!(
            "reqest path {path:?} does not start with /"
        )))
    } else if path.split('/').any(|v| v == "..") {
        Err(WebError::InternalString(format!(
            "stripped request path {path:?} contains .."
        )))
    } else {
        // This guarantees that the returned path doesn’t start or end with a /,
        // and doesn’t contain any "" or "." segments.
        #[expect(clippy::comparison_to_empty, reason = "clarity")]
        Ok(path
            .split('/')
            .filter(|part| *part != "." && *part != "")
            .collect::<Vec<_>>()
            .join("/"))
    }
}

/// Return a static file as an [`HttpResponse`].
///
/// # Errors
///
/// This returns <code>[WebError::Internal][]([Error::Io][])</code> if there is
/// a problem reading the static file.
fn render_static(
    req: &HttpRequest,
    static_path: &Path,
    relative_path: &str,
) -> WebResult<HttpResponse> {
    Ok(match open_static_file(req, static_path, relative_path) {
        Err(WebError::NotFound) => {
            let index_relative_path = format!("{relative_path}/index.html");
            open_static_file(
                req,
                static_path,
                index_relative_path.trim_start_matches('/'),
            )
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
fn open_static_file(
    req: &HttpRequest,
    root: &Path,
    relative_path: &str,
) -> WebResult<NamedFile> {
    let path = root.join(relative_path);
    let file = open_confirmed_file(&path)?;

    let canonical_path = if relative_path.ends_with("/index.html") {
        // This leaves a trailing '/'. relative_path is guaranteed not to start
        // with '/', so there must be another character before that, too.
        format!("/{}", relative_path.strip_suffix("index.html").unwrap())
    } else if relative_path == "index.html" {
        "/".to_owned()
    } else {
        format!("/{relative_path}")
    };

    if req.path() == canonical_path {
        Ok(NamedFile::from_file(file, path)?)
    } else {
        Err(WebError::RedirectCanonical(canonical_path))
    }
}

/// Render a page to be served over HTTP.
///
/// # Errors
///
/// Returns [`WebError`] if the page cannot be found or cannot be rendered.
fn render_page(
    req: &HttpRequest,
    root: &Path,
    relative_path: &str,
    tpls: &Handlebars<'_>,
) -> WebResult<HttpResponse> {
    // clean_path() guarantees that relative_path doesn’t start or end with /.
    let page = match try_read_file_page(req, root, relative_path) {
        Err(WebError::NotFound) => {
            try_read_directory_page(req, root, relative_path)
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
fn try_read_file_page(
    req: &HttpRequest,
    root: &Path,
    relative_path: &str,
) -> WebResult<Page> {
    let path = root.join(format!("{relative_path}.md"));
    let content = fs::read_to_string(&path)?;

    let canonical_path = if relative_path.ends_with("/index") {
        // This leaves a trailing '/'. relative_path is guaranteed not to start
        // with '/', so there must be another character before that, too.
        format!("/{}", relative_path.strip_suffix("index").unwrap())
    } else if relative_path == "index" {
        "/".to_owned()
    } else {
        format!("/{relative_path}")
    };

    if req.path() == canonical_path {
        Page::from_source(Source::from_path(path), content)
            .map_err(WebError::Internal)
    } else {
        Err(WebError::RedirectCanonical(canonical_path))
    }
}

/// Read a directory page (i.e. `index.md`).
///
/// # Errors
///
///   * [`WebError`] if the page cannot be read.
fn try_read_directory_page(
    req: &HttpRequest,
    root: &Path,
    relative_path: &str,
) -> WebResult<Page> {
    let path = root.join(relative_path).join("index.md");
    let content = fs::read_to_string(&path)?;

    let canonical_path = if relative_path.is_empty() {
        "/".to_owned()
    } else {
        format!("/{relative_path}/")
    };

    if req.path() == canonical_path {
        Page::from_source(Source::from_path(path), content)
            .map_err(WebError::Internal)
    } else {
        Err(WebError::RedirectCanonical(canonical_path))
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
