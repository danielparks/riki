//! Serve pages over HTTP

mod errors;
mod render;
pub use crate::http::errors::*;
pub use crate::http::render::*;

use crate::errors::Error;
use crate::errors::Result;
use crate::templates::TemplateManager;
use actix_files::NamedFile;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
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

    let data = Data::new(Mutex::new(TemplateManager::new("templates")?));

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&data))
            .wrap(TracingLogger::default())
            .service(path_handler)
    })
    .bind(address)
    .map_err(Error::BindErrorMap(address))?
    .run()
    .await
    .map_err(Error::Io)
}

/// Handle all GET requests
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
#[get("/{path:.*}")]
async fn path_handler(
    req: HttpRequest,
    tpls: Data<Mutex<TemplateManager>>,
) -> impl Responder {
    let result = match clean_file_path(req.path()) {
        Some(path) => static_render(&req, &path),
        _ => render(&req, &mut tpls.lock().unwrap()),
    };

    match result {
        Ok(response) => response,
        Err(error) => error.render(&req, &mut tpls.lock().unwrap()),
    }
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
fn clean_file_path<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
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

/// Return a static page as an [`HttpResponse`].
///
/// # Errors
///
/// This returns [`Error::Io`] when there is a problem reading the static file.
fn static_render(req: &HttpRequest, path: &Path) -> WebResult<HttpResponse> {
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
