mod errors;
mod render;
pub use crate::http::errors::*;
pub use crate::http::render::*;

use actix_web::{
    self,
    middleware::Logger,
    web,

    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
};
use crate::errors::Error;
use crate::errors::Result;
use crate::templates::TemplateManager;
use simplelog::*;
use std::sync::Mutex;

// TODO testing
// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode
//      - log errors
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
