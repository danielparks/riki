use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
};
use actix_web::middleware::Logger;
use actix_web::web;
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

fn render(req: HttpRequest) -> actix_web::Result<HttpResponse> {
    // FIXME! clean path
    let path = req.match_info().get("path").unwrap_or("");

    let template = PathBuf::from("templates/default.tmpl");
    let page = PathBuf::from(format!("pages/{}.md", path));

    let template = mustache::compile_path(&template).unwrap();
    let page = Page::read_from(&page).unwrap();

    let mut buffer = vec![];
    template.render(&mut buffer, &page).unwrap();

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=UTF-8")
        .body(buffer))
}
