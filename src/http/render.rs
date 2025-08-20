//! Render a page to be served over HTTP.

use crate::http::errors::{WebError, WebResult};
use crate::render::Page;
use crate::templates::TemplateManager;
use actix_web::{HttpRequest, HttpResponse};
use std::path::PathBuf;

/// Render a page to be served over HTTP.
///
/// # Errors
///
/// Returns [`WebError`] if the page cannot be found or cannot be rendered.
pub fn render(
    req: &HttpRequest,
    tpls: &mut TemplateManager,
) -> WebResult<HttpResponse> {
    let path = find_page_file(req.path())?;
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
