use crate::errors::Result;
use crate::http::errors::{WebError, WebResult};
use crate::render::Page;
use crate::templates::TemplateManager;
use actix_web::{HttpRequest, HttpResponse};
use std::path::{Path, PathBuf};

pub fn render(
    req: &HttpRequest,
    tpls: &mut TemplateManager,
) -> WebResult<HttpResponse> {
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

fn render_path(path: &Path, tpls: &mut TemplateManager) -> Result<String> {
    Page::read_from(path)?.render_to_string(tpls)
}
