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
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
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
    if let Some(path) = find_static_path(req.path()) {
        render_static(&req, &path)
    } else {
        render_page(&req, &tpls)
    }
    .unwrap_or_else(|web_error| web_error.render(&req, &tpls))
}

/// Check if a request path corresponds to a static file
///
/// Returns `None` if there is no matching static file.
///
/// # Errors
///
/// This should never encounter an error since Actix should clean up the path
/// for us. If this does encounter an error, e.g. a path containing “..”, it
/// will log the error to stderr (FIXME) and return `None`.
fn find_static_path<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
    match path.as_ref().strip_prefix("/") {
        Ok(path) => {
            let mut path_buf = PathBuf::from("static");
            path_buf.push(path);

            if path_buf.is_absolute() || path_buf.has_root() {
                eprintln!(
                    "ERROR calculated path \"{}\" is not relative",
                    path_buf.display()
                );
                return None;
            }

            let dotdot = OsStr::new("..");
            if path_buf.iter().any(|v| v == dotdot) {
                eprintln!(
                    "ERROR calculated path \"{}\" contains ..",
                    path_buf.display()
                );
                return None;
            }

            if path_buf.is_file() {
                Some(path_buf)
            } else {
                None
            }
        }
        Err(error) => {
            // WTF. It should always start with "/".
            eprintln!(
                "ERROR path.strip_prefix(\"/\") for \"{}\": {}",
                path.as_ref().display(),
                &error
            );
            None
        }
    }
}

/// Return a static file as an [`HttpResponse`].
///
/// # Errors
///
/// This returns <code>[WebError::Internal][]([Error::Io][])</code> is there is
/// a problem reading the static file.
fn render_static(req: &HttpRequest, path: &Path) -> WebResult<HttpResponse> {
    match NamedFile::open(path) {
        Ok(file) => Ok(file.into_response(req)),
        Err(error) => {
            eprintln!(
                "ERROR static file open {:}: {:}",
                path.display(),
                &error
            );
            Err(WebError::Internal(Error::Io(error)))
        }
    }
}

/// Render a page to be served over HTTP.
///
/// # Errors
///
/// Returns [`WebError`] if the page cannot be found or cannot be rendered.
fn render_page(
    req: &HttpRequest,
    tpls: &TemplateManager,
) -> WebResult<HttpResponse> {
    let path = find_page_path(req.path())?;
    let buffer = Page::read_from(&path)?.render_to_string(tpls)?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(buffer))
}

/// Find the page file that corresponds to a request path.
///
/// # Errors
///
/// Returns [`WebError::NotFound`] if the page cannot be found.
fn find_page_path(req_path: &str) -> WebResult<PathBuf> {
    // Actix mostly cleans the path for us (FIXME it should redirect).

    // req_path always starts with a /
    let base = format!("pages{}", req_path.trim_end_matches('/'));

    let test = PathBuf::from(format!("{base}.md"));
    if test.is_file() {
        return Ok(test);
    }

    let test = PathBuf::from(base).join("index.md");
    if test.is_file() {
        return Ok(test);
    }

    Err(WebError::NotFound { req_path: req_path.to_owned() })
}
