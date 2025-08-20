//! Render error pages.

use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder, http};
use htmlize::escape_text;
use std::result::Result;
use thiserror::Error;

use crate::templates::TemplateManager;

/// An error that will be reported to the user as a web page.
#[derive(Debug, Error)]
pub enum WebError {
    /// Internal server error.
    #[error("internal server error: {0}")]
    Internal(#[from] crate::errors::Error),

    /// Page not found error.
    #[error("page {req_path} not found")]
    NotFound {
        /// The request path.
        req_path: String,
    },
}

/// `Result` type for `WebError`.
pub type WebResult<T, E = WebError> = Result<T, E>;

impl WebError {
    /// Render the error into an `HttpResponse`.
    pub fn render(
        &self,
        req: &HttpRequest,
        tpls: &mut TemplateManager,
    ) -> HttpResponse {
        let (code, template_path, data) = match self {
            Self::Internal(error) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "error500.tmpl",
                mustache::MapBuilder::new()
                    .insert_str("error", error.to_string())
                    .build(),
            ),
            Self::NotFound { req_path } => (
                http::StatusCode::NOT_FOUND,
                "error404.tmpl",
                mustache::MapBuilder::new()
                    .insert_str("req_path", req_path)
                    .build(),
            ),
        };

        let buffer = match tpls.get(&template_path) {
            Ok(tpl) => match tpl.render_data_to_string(&data) {
                Ok(buffer) => buffer,
                Err(error2) => self.fallback_render(req, &error2.into()),
            },
            Err(error2) => self.fallback_render(req, &error2.into()),
        };

        HttpResponseBuilder::new(code)
            .content_type("text/html; charset=UTF-8")
            .body(buffer)
    }

    /// Render an error if there was a problem with the template.
    fn fallback_render(
        &self,
        _req: &HttpRequest,
        error2: &anyhow::Error,
    ) -> String {
        let self_html = escape_text(self.to_string());
        let error2_html = escape_text(error2.to_string());

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="UTF-8">
        <title>Error: {self_html}</title>
    </head>
    <body>
        <h1>Error: {self_html}</h1>
        <h3>While trying to render the error page, another error occurred:</h3>
        <pre>{error2_html}</pre>
    </body>
</html>"#
        )
    }
}
