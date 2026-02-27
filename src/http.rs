//! # Serve pages over HTTP
//!
//! How pages are served are determined by a sequence of rules. See
//! [`riki::rules`][crate::rules].

mod tests;
pub mod util;

use crate::actions::{self, RequestVariables, VariableMap};
use crate::config::model::Configuration;
use crate::render::{TemplatesManager, base_templates};
use crate::rules;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use std::process::ExitCode;
use termcolor::StandardStream;
use tracing_actix_web::TracingLogger;

// TODO better error handling
//      - Bad page metadata errors should be shown to admin, but not user
//      - dev mode

/// Main entry point for serving over HTTP
///
/// # Errors
///
/// May return an error if the server could not start correctly.
#[actix_web::main]
pub async fn serve<S: AsRef<str>>(
    base_dir: String,
    address: S,
    err_stream: &StandardStream,
) -> crate::Result<()> {
    let address = address.as_ref();

    util::check_dir(&base_dir)?;

    let template_dir = format!("{base_dir}/templates");
    let router = Data::new(Router::from_configuration(
        rules::default_rules(base_dir, template_dir).map_err(
            |diagnostics| {
                crate::config::print_diagnostics(
                    &rules::SOURCE,
                    err_stream,
                    &diagnostics,
                );
                crate::Error::ExitWithCode(ExitCode::FAILURE)
            },
        )?,
    ));

    HttpServer::new(move || {
        App::new()
            .app_data(Data::clone(&router))
            .wrap(TracingLogger::default())
            .service(path_handler)
    })
    .bind(address)
    .map_err(|error| crate::Error::BindError {
        source: error,
        address: String::from(address),
    })?
    .run()
    .await
    .map_err(crate::Error::Io)
}

/// Handle all GET requests
#[expect(clippy::future_not_send, reason = "Required by Actix")]
#[get("/{path:.*}")]
pub async fn path_handler(
    req: HttpRequest,
    router: Data<Router<'_, '_>>,
) -> impl Responder {
    router
        .route(&req)
        .await
        .unwrap_or_else(|error: actions::Error| {
            tracing::error!("{} could not render error: {error:?}", req.path());
            error.render(&req, &base_templates())
        })
}

/// Route requests to the right actions
#[derive(Debug)]
pub struct Router<'src, 'tpls> {
    /// Configured rules
    config: Configuration<'src>,

    /// Manage template registries
    manager: TemplatesManager<'tpls>,
}

impl<'src> Router<'src, '_> {
    /// Create a router from a [`Configuration`].
    ///
    /// # Errors
    ///
    /// Returns an error if there is a problem loading templates.
    #[must_use]
    pub fn from_configuration(config: Configuration<'src>) -> Self {
        Self { config, manager: TemplatesManager::default() }
    }

    /// Route a request.
    ///
    /// # Errors
    ///
    /// Returned errors will be converted to appropriate HTTP responses.
    #[expect(clippy::future_not_send, reason = "Actix doesn’t require Send")]
    #[expect(clippy::unused_async, reason = "Required by Actix")]
    pub async fn route(
        &self,
        request: &HttpRequest,
    ) -> actions::Result<HttpResponse> {
        tracing::trace!("route request: {:?}", request);

        // This errors if clean_path() failed, which should never happen.
        // FIXME? move clean_path() out for explicit error handling?
        let variables = RequestVariables::new(request)?;

        match (|| {
            for rule in self.config.matches(&variables.clean_path()) {
                // FIXME &variables instead of clone()
                match rule.evaluate(&self.manager, request, variables.clone()) {
                    Err(actions::Error::NotFound) => (), // skip
                    other => return other,
                }
            }
            Err(actions::Error::NotFound)
        })() {
            Ok(response) => Ok(response),
            Err(error) => {
                tracing::trace!("error returned from rules: {error:?}");
                if let Some(rule) =
                    self.config.last_matching(&variables.clean_path())
                {
                    let templates_path =
                        rule.settings.templates.path_content(&variables);
                    // Error: loading templates (no dir, bad template?)
                    let tpls =
                        self.manager.templates_for_directory(templates_path)?;
                    Ok(error.render(request, &tpls))
                } else {
                    // Use default templates to render error.
                    Err(error)
                }
            }
        }
    }
}
