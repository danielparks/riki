//! Serve pages over HTTP

mod errors;
pub use crate::http::errors::*;

use crate::errors::{Error, Result};
use crate::render::Page;
use crate::templates::TemplateManager;
use actix_files::NamedFile;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use tracing;
use tracing_actix_web::TracingLogger;

// TODO testing
// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode
//      - log errors

/// Main entry point for serving over HTTP
///
/// # Errors
///
/// May return an error if the server could not start correctly.
#[actix_web::main]
pub async fn serve<S: AsRef<str>>(address: S) -> Result<()> {
    let address = address.as_ref();

    let data = Data::new(TemplateManager::from_directory("templates")?);

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&data))
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
async fn path_handler(
    req: HttpRequest,
    tpls: Data<TemplateManager>,
) -> impl Responder {
    clean_path(req.path())
        .and_then(|path| match render_static(&req, path) {
            Err(WebError::NotFound) => {
                tracing::trace!("static not found, trying page");
                render_page(&req, path, &tpls)
            }
            other => other,
        })
        .unwrap_or_else(|error: WebError| error.render(&req, &tpls))
}

/// Get a relative path that can be joined to another path safely.
///
/// # Errors
///
///   * [`WebError::InternalString`] if the path doesn’t start with / or if the
///     path contains a .. segment.
fn clean_path(path: &str) -> WebResult<&str> {
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
        Ok(path.trim_start_matches('/'))
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
    relative_path: &str,
) -> WebResult<HttpResponse> {
    Ok(open_static(&PathBuf::from("static").join(relative_path))?
        .into_response(req))
}

/// Open a path as a [`NamedFile`].
///
/// This reads one byte from the file to check if it’s actually a directory.
///
/// # Errors
///
///   * [`io::Error`] if there was a problem opening or reading the file.
fn open_static(path: &Path) -> io::Result<NamedFile> {
    let mut file = fs::File::open(path)?;

    // Read 1 byte to check if the file is a directory. Using `is_dir()` would
    // create a race condition.
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;

    NamedFile::from_file(file, path)
}

/// Render a page to be served over HTTP.
///
/// # Errors
///
/// Returns [`WebError`] if the page cannot be found or cannot be rendered.
fn render_page(
    _req: &HttpRequest,
    relative_path: &str,
    tpls: &TemplateManager,
) -> WebResult<HttpResponse> {
    let root = PathBuf::from("pages");
    let relative_path = relative_path.trim_end_matches('/');

    let page = try_read_page(root.join(format!("{relative_path}.md")))
        .or_else(|| try_read_page(root.join(relative_path).join("index.md")))
        .unwrap_or(Err(WebError::NotFound))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(page.render_to_string(tpls)?))
}

/// Read a page file, or return `None` if it doesn’t exist or is a directory.
///
/// # Errors
///
///   * [`WebError`] if the page cannot be read.
fn try_read_page(path: PathBuf) -> Option<WebResult<Page>> {
    match fs::read_to_string(&path) {
        Ok(content) => match Page::from_string(content) {
            Ok(mut page) => {
                page.file = path;
                Some(Ok(page))
            }
            Err(error) => Some(Err(WebError::Internal(error))),
        },
        Err(error) => match WebError::from(error) {
            WebError::NotFound => None,
            error => Some(Err(error)),
        },
    }
}
