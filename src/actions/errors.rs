//! # Errors
//!
//! Actions have their own [`Error`] type because action results usually become
//! HTTP responses (represented by variants on the type).
//!
//! The [`Error::NotFound`] variant is special in that it generally means that
//! the action should be canceled and the request should fall through to the
//! next configuration rule.

use axum::body::Body;
use axum::response::Response;
use handlebars::Handlebars;
use htmlize::escape_text;
use http::{StatusCode, header};
use maplit::hashmap;
use std::io::{self, ErrorKind};
use std::result;

/// `Result` type for [`Error`].
pub type Result<T = super::ActionReturn, E = Error> = result::Result<T, E>;

/// An error from an action.
///
/// These might be passed back to the client as an error page.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Internal server error.
    #[error("internal server error: {0}")]
    Internal(#[from] crate::Error),

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

    /// # Redirect.
    ///
    /// Generally this means that a non-canonical URL was requested.
    #[error("redirect to {0}")]
    RedirectCanonical(String),
}

impl From<crate::NotUtf8> for Error {
    fn from(error: crate::NotUtf8) -> Self {
        Self::Internal(crate::Error::NotUtf8(error))
    }
}

impl From<io::Error> for Error {
    /// Convert [`io::Error`] into [`Error`]. Handles fall-through logic.
    ///
    /// See [`Error::NotFound`].
    fn from(error: io::Error) -> Self {
        if is_not_found(&error) {
            Self::NotFound
        } else if error.kind() == ErrorKind::PermissionDenied {
            Self::Forbidden
        } else {
            Self::Internal(crate::Error::Io(error))
        }
    }
}

impl Error {
    /// Render the error into a [`Response`].
    ///
    /// # Panics
    ///
    /// Panics if the response builder is given invalid header values, which
    /// cannot happen with the values used here.
    #[must_use]
    pub fn render(&self, req_path: &str, tpls: &Handlebars) -> Response {
        let builder = http::Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=UTF-8");

        let (builder, template_name, data) = match self {
            Self::Internal(error) => (
                builder.status(StatusCode::INTERNAL_SERVER_ERROR),
                "error500",
                hashmap! {
                    "error" => error.to_string(),
                    "error_debug" => format!("{error:#?}"),
                    "req_path" => req_path.to_owned(),
                },
            ),
            Self::InternalString(error) => (
                builder.status(StatusCode::INTERNAL_SERVER_ERROR),
                "error500",
                hashmap! {
                    "error" => error.clone(),
                    "error_debug" => error.clone(),
                    "req_path" => req_path.to_owned(),
                },
            ),
            Self::NotFound => (
                builder.status(StatusCode::NOT_FOUND),
                "error404",
                hashmap! { "req_path" => req_path.to_owned() },
            ),
            Self::Forbidden => (
                builder.status(StatusCode::FORBIDDEN),
                "error403",
                hashmap! { "req_path" => req_path.to_owned() },
            ),
            Self::RedirectCanonical(url) => (
                builder
                    .status(StatusCode::MOVED_PERMANENTLY)
                    .header(header::LOCATION, url.as_str()),
                "redirect301",
                hashmap! { "canonical_url" => url.clone() },
            ),
        };

        builder
            .body(Body::from(
                tpls.render(template_name, &data).unwrap_or_else(|error2| {
                    self.fallback_render(req_path, &error2.into())
                }),
            ))
            .expect("valid response")
    }

    /// Render an error if there was a problem with the template.
    fn fallback_render(
        &self,
        _req_path: &str,
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

/// Check if an [`io::Error`] represents something not found for our purposes.
///
/// Generally this is used for purpose of falling through to the next possible
/// match, e.g. if a static file isn’t found, fall through to try a page.
#[must_use]
pub fn is_not_found(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::IsADirectory
            | ErrorKind::NotFound
            | ErrorKind::NotADirectory
    )
}
