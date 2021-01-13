mod errors;
mod render;
pub use crate::http::errors::*;
pub use crate::http::render::*;

use actix_files::NamedFile;
use actix_web::{
    self,
    middleware::Logger,
    web,

    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
    Responder,
};
use crate::errors::Error;
use crate::errors::Result;
use crate::templates::TemplateManager;
use simplelog::*;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

// TODO testing
// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode
//      - log errors

#[actix_web::main]
pub async fn serve<S: 'static + AsRef<str>>(address: S) -> Result<()> {
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
        .run()
        .await
        .map_err(Error::Io)
}

fn init_logging() {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed
        ),
    ]).unwrap();
}

fn static_render(req: &HttpRequest, path: &Path) -> WebResult<HttpResponse> {
    match NamedFile::open(path) {
        Ok(file) => {
            match file.into_response(&req) {
                Ok(response) => {
                    Ok(response)
                },
                Err(error) => {
                    // actix_web::Error doesn’t have Send, so we can’t convert
                    // to a crate::Error type.
                    eprintln!("ERROR static file into_response {:}\n{:}", path.display(), &error);
                    Err(Error::StaticFile{}.into())
                },
            }
        },
        Err(error) => {
            eprintln!("ERROR static file open {:}: {:}", path.display(), &error);
            Err(Error::from(error).into())
        },
    }
}

fn clean_file_path<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    // TODO? Actix seems to do deal with .. and maybe // for us. Simplify?
    match path.as_ref().strip_prefix("/") {
        Ok(path) => {
            let mut path_buf = PathBuf::from("static");
            path_buf.push(path);

            if path_buf.is_absolute() || path_buf.has_root() {
                eprintln!("ERROR calculated path \"{}\" is not relative",
                    path_buf.display());
                return None;
            }

            let dotdot = OsStr::new("..");
            if path_buf.iter().find(|&v| v == dotdot).is_some() {
                eprintln!("ERROR calculated path \"{}\" contains ..",
                    path_buf.display());
                return None;
            }

            if path_buf.is_file() {
                Some(path_buf)
            } else {
                None
            }
        },
        Err(error) => {
            // WTF. It should always start with "/".
            eprintln!("ERROR path.strip_prefix(\"/\") for \"{}\": {}",
                path.as_ref().display(), &error);
            None
        },
    }
}

async fn path_handler(req: HttpRequest, tpls: web::Data<Mutex<TemplateManager>>) -> impl Responder {
    let mut tpls = tpls.lock().unwrap();

    let result = match clean_file_path(req.path()) {
        Some(path) => static_render(&req, &path),
        _ => render(&req, &mut tpls),
    };

    match result {
        Ok(response) => response,
        Err(error) => render_error(&req, &mut tpls, error),
    }
}
