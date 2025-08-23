//! Render error pages.

use crate::errors::Error;
use crate::templates::TemplateManager;
use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder, http};
use htmlize::escape_text;
use std::io::{self, ErrorKind};
use std::result::Result;

/// `Result` type for `WebError`.
pub type WebResult<T, E = WebError> = Result<T, E>;

/// An error that will be reported to the user as a web page.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// Internal server error.
    #[error("internal server error: {0}")]
    Internal(#[from] crate::errors::Error),

    /// Internal server error.
    ///
    /// This takes a string message for rare conditions.
    #[error("{0}")]
    InternalString(String),

    /// Page not found error.
    #[error("page not found")]
    NotFound,

    /// Permission denied error.
    #[error("access forbidden")]
    Forbidden,
}

impl From<io::Error> for WebError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            // `IsADirectory` is equivalent to `NotFound` for easy fallbacks.
            ErrorKind::IsADirectory | ErrorKind::NotFound => Self::NotFound,
            ErrorKind::PermissionDenied => Self::Forbidden,
            _ => Self::Internal(Error::Io(error)),
        }
    }
}

impl WebError {
    /// Render the error into an `HttpResponse`.
    #[must_use]
    pub fn render(
        &self,
        req: &HttpRequest,
        tpls: &TemplateManager,
    ) -> HttpResponse {
        let (code, template_name, data) = match self {
            Self::Internal(error) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "error500",
                mustache_key_value("error", error),
            ),
            Self::InternalString(error) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "error500",
                mustache_key_value("error", error),
            ),
            Self::NotFound => (
                http::StatusCode::NOT_FOUND,
                "error404",
                mustache_key_value("req_path", req.path()),
            ),
            Self::Forbidden => (
                http::StatusCode::FORBIDDEN,
                "error403",
                mustache_key_value("req_path", req.path()),
            ),
        };

        let buffer = match tpls.get(template_name) {
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

/// Generate [`mustache::Data`] that only contains a single key value.
#[expect(clippy::needless_pass_by_value)] // `ToString` allows borrowing
fn mustache_key_value<V: ToString>(key: &str, value: V) -> mustache::Data {
    mustache::MapBuilder::new()
        .insert_str(key, value.to_string())
        .build()
}
