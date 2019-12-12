use actix_web::{
    dev::HttpResponseBuilder,
    http,
    HttpRequest,
    HttpResponse,
};
use anyhow::Error as AnyError;
use htmlize::escape_text;
use serde::Serialize;
use std::error::Error as StdError;
use std::result::Result as StdResult;
use thiserror::Error;

use crate::errors::Error as CrateError;
use crate::templates::TemplateManager;

#[derive(Debug, Error)]
pub enum WebError {
    #[error("internal server error")]
    Internal(#[from] CrateError),

    #[error("page not <b>foo</b> found")]
    NotFound,
}

pub type WebResult<T, E = WebError> = StdResult<T, E>;

pub fn render_error(req: &HttpRequest, tpls: &mut TemplateManager, error: WebError) -> HttpResponse {
    let code = match error {
        WebError::Internal(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        WebError::NotFound => http::StatusCode::NOT_FOUND,
    };

    let error = ErrorOutput::from(error);

    let buffer = match tpls.get(&"error") {
        Ok(tpl) => {
            match tpl.render_to_string(&error) {
                Ok(buffer) => buffer,
                Err(error2) => fallback_render_error(&req, &error, &ErrorOutput::from(error2)),
            }
        },
        Err(error2) => fallback_render_error(&req, &error, &ErrorOutput::from(error2)),
    };

    HttpResponseBuilder::new(code)
        .content_type("text/html; charset=UTF-8")
        .body(buffer)
}

#[derive(Debug, Serialize)]
struct ErrorOutput {
    pub short: String,
    pub long: String,
}

impl ErrorOutput {
    fn from<E>(error: E) -> ErrorOutput
        where E: StdError + Send + Sync + 'static
    {
        let error = AnyError::from(error);
        ErrorOutput {
            short: format!("{}", error),
            long: format!("{:?}", error),
        }
    }
}

impl std::fmt::Display for ErrorOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
       write!(f, "{}", self.short)
    }
}

fn fallback_render_error(_req: &HttpRequest, error: &ErrorOutput, error2: &ErrorOutput) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="UTF-8">
        <title>Error: {}</title>
    </head>
    <body>
        <h1>Error: {}</h1>
        <pre>{}</pre>
        <h3>While trying to render the error page, another error occurred:</h3>
        <pre>{}</pre>
    </body>
</html>"#,
        escape_text(&error.short), escape_text(&error.short),
        escape_text(&error.long), escape_text(&error2.long))
}
