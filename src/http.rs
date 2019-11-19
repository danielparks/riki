use actix_web::{
    self,
    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
    web,
};
use actix_web::middleware::Logger;
use simplelog::*;
use std::path::PathBuf;

use crate::errors::MyError;
use crate::errors::Result;
use crate::render::Page;

pub fn serve() -> Result<()> {
    let address = "127.0.0.1:8000";

    init_logging();

    HttpServer::new(|| {
            App::new()
                .wrap(Logger::new("%a %t \"%r\" %s %b %Ts"))
                .route("/{path:.*}", web::get().to(render))
        })
        .bind(address).map_err(MyError::BindErrorMap(address))?
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

fn find_page_file(req_path: &str) -> actix_web::Result<PathBuf> {
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

    Err(actix_web::error::ErrorNotFound("page not found"))
}

fn render(req: HttpRequest) -> actix_web::Result<HttpResponse> {
    let template = PathBuf::from("templates/default.tmpl");

    let path = find_page_file(req.path())?;
    let page = Page::read_from(&path)
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let mut buffer = vec![];
    let template = mustache::compile_path(&template)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    template.render(&mut buffer, &page)
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(buffer))
}
