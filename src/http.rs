mod errors;
pub use errors::*;

use actix_web::{
    self,
    middleware::Logger,
    web,

    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
};
use simplelog::*;
use std::path::PathBuf;
use std::sync::Mutex;

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
