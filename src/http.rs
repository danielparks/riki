use actix_web::{
    self,
    dev::HttpResponseBuilder,
    http,
    middleware::Logger,
    web,

    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
};
use anyhow::Error as AnyError;
use std::error::Error as StdError;
use serde::Serialize;
use simplelog::*;
use std::fmt;
use std::path::PathBuf;
use std::result;
use std::sync::Mutex;
use thiserror::Error;

use crate::errors::Error;
use crate::errors::Result;
use crate::render::Page;
use crate::templates::TemplateManager;

// TODO testing
// TODO error pages
// TODO better error handling
// TODO selectable layout
// TODO automatic title
// TODO static

#[derive(Debug, Error)]
pub enum WebError {
    #[error("internal server error")]
    Internal(#[from] Error),

    #[error("page not found")]
    NotFound,
}

pub type WebResult<T, E = WebError> = result::Result<T, E>;

pub fn serve<S: AsRef<str>>(address: S) -> Result<()> {
    let address = address.as_ref();
    init_logging();

    // We clone this into the app data, which makes this entirely thread safe.
    // Unfortunately, actix uses a data structure that forces us to use Mutex
    // in order to get a mutable ref.
    let tpls = TemplateManager::new("templates")?;

    HttpServer::new(move || {
            App::new()
                .data(Mutex::new(tpls.clone()))
                .wrap(Logger::new("%a %t \"%r\" %s %b %Ts"))
                .route("/{path:.*}", web::get().to(path_handler))
        })
        .bind(address).map_err(Error::BindErrorMap(address))?
        .run()?;

    Ok(())
}

fn init_logging() {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed
        ).unwrap(),
    ]).unwrap();
}

fn path_handler(req: HttpRequest, tpls: web::Data<Mutex<TemplateManager>>) -> HttpResponse {
    let mut tpls = tpls.lock().unwrap();

    match render(&req, &mut tpls) {
        Ok(response) => response,
        Err(error) => render_error(&req, &mut tpls, error),
    }
}

#[derive(Debug, Serialize)]
struct ErrorOutput {
    pub short: String,
    pub long: String,
}

impl ErrorOutput {
    fn from<E>(error: E) -> ErrorOutput
        where E: StdError + Send + Sync + 'static
    {
        let error = AnyError::from(error);
        ErrorOutput {
            short: format!("{}", error),
            long: format!("{:?}", error),
        }
    }
}

impl fmt::Display for ErrorOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       write!(f, "{}", self.short)
    }
}

fn render_error(req: &HttpRequest, tpls: &mut TemplateManager, error: WebError) -> HttpResponse {
    let code = match error {
        WebError::Internal(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        WebError::NotFound => http::StatusCode::NOT_FOUND,
    };

    let error = ErrorOutput::from(error);

    let buffer = match tpls.get(&"error") {
        Ok(tpl) => {
            match tpl.render_to_string(&error) {
                Ok(buffer) => buffer,
                Err(error2) => fallback_render_error(&req, &error, &ErrorOutput::from(error2)),
            }
        },
        Err(error2) => fallback_render_error(&req, &error, &ErrorOutput::from(error2)),
    };

    HttpResponseBuilder::new(code)
        .content_type("text/html; charset=UTF-8")
        .body(buffer)
}

fn fallback_render_error(_req: &HttpRequest, error: &ErrorOutput, error2: &ErrorOutput) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="UTF-8">
        <title>Error: {}</title>
    </head>
    <body>
        <h1>Error: {}</h1>
        <pre>{}</pre>
        <h3>While trying to render the error page, another error occurred:</h3>
        <pre>{}</pre>
    </body>
</html>"#,
        error.short, error.short, error.long, error2.long)
}

fn render(req: &HttpRequest, tpls: &mut TemplateManager) -> WebResult<HttpResponse> {
    let path = find_page_file(req.path())?;
    let buffer = render_path(&path, tpls)?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(buffer))
}

fn find_page_file(req_path: &str) -> WebResult<PathBuf> {
    // Actix mostly cleans the path for us (FIXME it should redirect).

    // req_path always starts with a /
    let base = format!("pages{}", req_path.trim_end_matches('/'));

    let test = PathBuf::from(format!("{}.md", base));
    if test.is_file() {
        return Ok(test);
    }

    let test = PathBuf::from(base).join("index.md");
    if test.is_file() {
        return Ok(test);
    }

    Err(WebError::NotFound)
}

fn render_path(path: &PathBuf, tpls: &mut TemplateManager) -> Result<String> {
    let page = Page::read_from(&path)?;

    Ok(tpls.default()?.render_to_string(&page)?)
}
