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
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use tracing;
use tracing_actix_web::TracingLogger;

// TODO testing
// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode
//      - log errors

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
    fn default_in<P: Into<PathBuf>>(root: P) -> Self {
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

    let config = Data::new(Configuration::default_in(path.as_ref()));
    let tpls =
        Data::new(TemplateManager::from_directory(&config.templates_path)?);

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
async fn path_handler(
    req: HttpRequest,
    tpls: Data<TemplateManager>,
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
        // FIXME: redirect (maybe if the canonical path != req.path())?
        #[expect(clippy::comparison_to_empty)]
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
    let candidate = static_path.join(relative_path);
    Ok(match open_static(&candidate) {
        Err(WebError::NotFound) => open_static(&candidate.join("index.html")),
        other => other,
    }?
    .into_response(req))
}

/// Open a path as a [`NamedFile`].
///
/// This reads one byte from the file to check if it’s actually a directory.
///
/// # Errors
///
///   * [`WebError`] if there was a problem opening or reading the file.
fn open_static(path: &Path) -> WebResult<NamedFile> {
    let mut file = fs::File::open(path)?;

    // Read 1 byte to check if the file is a directory. Using `is_dir()` would
    // create a race condition.
    let mut buffer: [u8; 1] = [0];
    _ = file.read(&mut buffer)?;
    file.rewind()?;

    Ok(NamedFile::from_file(file, path)?)
}

/// Render a page to be served over HTTP.
///
/// # Errors
///
/// Returns [`WebError`] if the page cannot be found or cannot be rendered.
fn render_page(
    _req: &HttpRequest,
    root: &Path,
    relative_path: &str,
    tpls: &TemplateManager,
) -> WebResult<HttpResponse> {
    // clean_path() guarantees that relative_path doesn’t start or end with /.
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
        Ok(content) => {
            Some(Page::from_source(path, content).map_err(WebError::Internal))
        }
        Err(error) => match WebError::from(error) {
            WebError::NotFound => None,
            error => Some(Err(error)),
        },
    }
}

#[cfg(test)]
mod tests {
    use actix_http::Request;
    use actix_web::body::BoxBody;
    use actix_web::dev::{Service, ServiceResponse};
    use actix_web::test::TestRequest;
    use actix_web::{App, body, test, web};
    use assert2::{check, let_assert};
    use temp_dir::TempDir;
    use tracing_test::traced_test;

    use super::*;

    /// Initialize the test app.
    #[expect(clippy::future_not_send)] // Actix doesn’t require Send.
    async fn init_app() -> (
        TempDir,
        Configuration,
        impl Service<
            Request,
            Response = ServiceResponse<BoxBody>,
            Error = actix_web::Error,
        >,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let config = Configuration::default_in(temp_dir.path());
        fs::create_dir(&config.pages_path).unwrap();
        fs::create_dir(&config.static_path).unwrap();

        let mut tpls = TemplateManager::new();
        tpls.load_from_string("default", "{{& body }}").unwrap();
        tpls.load_from_string("error403", "403").unwrap();
        tpls.load_from_string("error404", "404").unwrap();
        tpls.load_from_string("error500", "{{& error_debug }}")
            .unwrap();

        (
            temp_dir,
            config.clone(),
            test::init_service(
                App::new()
                    .app_data(Data::new(tpls))
                    .app_data(Data::new(config))
                    .service(path_handler),
            )
            .await,
        )
    }

    /// Convert byte string to right type for comparison with a response body.
    const B: fn(&'static [u8]) -> web::Bytes = web::Bytes::from_static;

    /// Make a GET request to the test app.
    #[expect(clippy::future_not_send)] // Actix doesn’t require Send.
    async fn get<S, B, E>(app: S, uri: &str) -> S::Response
    where
        S: Service<Request, Response = ServiceResponse<B>, Error = E>,
        E: std::fmt::Debug,
    {
        test::call_service(&app, TestRequest::get().uri(uri).to_request()).await
    }

    #[actix_web::test]
    #[traced_test]
    async fn test_index_page_get() {
        let (_dir, config, app) = init_app().await;

        fs::write(config.pages_path.join("index.md"), "index").unwrap();

        // FIXME check content-type
        let resp = get(&app, "/").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"<p>index</p>\n"));

        let resp = get(&app, "/.").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"<p>index</p>\n"));
    }

    #[actix_web::test]
    #[traced_test]
    async fn test_static_get() {
        let (_dir, config, app) = init_app().await;

        fs::write(config.static_path.join("a.txt"), "AAA").unwrap();

        // FIXME check content-type
        let resp = get(&app, "/a.txt").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"AAA"));

        let resp = get(&app, "/a.txt/").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"AAA"));

        let resp = get(&app, "/a.txt/.").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"AAA"));
    }

    #[actix_web::test]
    #[traced_test]
    async fn test_fall_through() {
        let (_dir, config, app) = init_app().await;

        fs::write(config.static_path.join("static"), "STATIC").unwrap();
        fs::create_dir(config.pages_path.join("static")).unwrap();
        fs::write(config.pages_path.join("static/page.md"), "PAGE").unwrap();

        // FIXME check content-type
        let resp = get(&app, "/static").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"STATIC"));

        let resp = get(&app, "/static/page").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"<p>PAGE</p>\n"));

        let resp = get(&app, "/static/page/.").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"<p>PAGE</p>\n"));
    }

    #[actix_web::test]
    #[traced_test]
    async fn test_static_index_get() {
        let (_dir, config, app) = init_app().await;

        let b_dir = config.static_path.join("b");
        fs::create_dir(b_dir).unwrap();
        fs::write(config.static_path.join("b/index.html"), "BBB").unwrap();

        // FIXME check content-type
        let resp = get(&app, "/b").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"BBB"));

        let resp = get(&app, "/b/").await;
        check!(resp.status().as_u16() == 200);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"BBB"));
    }

    #[actix_web::test]
    #[traced_test]
    async fn test_not_found_get() {
        let (_dir, _config, app) = init_app().await;

        let resp = get(&app, "/not-found").await;
        check!(resp.status().as_u16() == 404);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"404"));
    }

    #[cfg(all(not(target_os = "hermit"), unix))]
    #[actix_web::test]
    #[traced_test]
    async fn test_forbidden_page_get() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, config, app) = init_app().await;

        let path = config.pages_path.join("forbidden.md");
        fs::write(&path, "forbidden").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

        let resp = get(&app, "/forbidden").await;
        check!(resp.status().as_u16() == 403);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"403"));
    }

    #[cfg(all(not(target_os = "hermit"), unix))]
    #[actix_web::test]
    #[traced_test]
    async fn test_forbidden_static_get() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, config, app) = init_app().await;

        let path = config.static_path.join("forbidden.txt");
        fs::write(&path, "forbidden").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

        let resp = get(&app, "/forbidden.txt").await;
        check!(resp.status().as_u16() == 403);
        let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
        check!(body == B(b"403"));
    }
}
