//! Render error pages.

use crate::errors::Error;
use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder, http};
use handlebars::Handlebars;
use htmlize::escape_text;
use maplit::hashmap;
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
    ///
    /// This generally means that the request will fall through to the next
    /// layer, e.g. if it was looking for a static file it will look for a page.
    #[error("page not found")]
    NotFound,

    /// Permission denied error.
    #[error("access forbidden")]
    Forbidden,
}

impl From<io::Error> for WebError {
    /// Convert [`io::Error`] into [`WebError`]. Handles fall-through logic.
    ///
    /// See [`WebError::NotFound`].
    fn from(error: io::Error) -> Self {
        match error.kind() {
            // These errors all map to `NotFound` for the purpose of falling
            // through to the next possible match.
            ErrorKind::IsADirectory
            | ErrorKind::NotFound
            | ErrorKind::NotADirectory => Self::NotFound,
            ErrorKind::PermissionDenied => Self::Forbidden,
            _ => Self::Internal(Error::Io(error)),
        }
    }
}

impl WebError {
    /// Render the error into an `HttpResponse`.
    #[must_use]
    pub fn render(&self, req: &HttpRequest, tpls: &Handlebars) -> HttpResponse {
        let (code, template_name, data) = match self {
            Self::Internal(error) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "error500",
                hashmap! {
                    "error" => error.to_string(),
                    "error_debug" => format!("{error:#?}"),
                    "req_path" => req.path().to_owned(),
                },
            ),
            Self::InternalString(error) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "error500",
                hashmap! {
                    "error" => error.clone(),
                    "error_debug" => error.clone(),
                    "req_path" => req.path().to_owned(),
                },
            ),
            Self::NotFound => (
                http::StatusCode::NOT_FOUND,
                "error404",
                hashmap! { "req_path" => req.path().to_owned() },
            ),
            Self::Forbidden => (
                http::StatusCode::FORBIDDEN,
                "error403",
                hashmap! { "req_path" => req.path().to_owned() },
            ),
        };

        let buffer = tpls
            .render(template_name, &data)
            .unwrap_or_else(|error2| self.fallback_render(req, &error2.into()));

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
