//! # Serve pages over HTTP
//!
//! [`path_handler()`][path_handler] first checks if the URL path corresponds to
//! something in the static directory, then it checks the pages. If nothing is
//! found it renders the error404 template.
//!
//! ## Canonical URLs and redirects
//!
//! Riki redirects to the canonical URL of a page when possible.
//!
//! The canonical URL will end with a / if (and only if) it corresponds to a
//! `index.html`-like page or static file.
//!
//! | Source path             | Canonical path   |
//! |-------------------------|------------------|
//! | `pages/page.md`         | `/page`          |
//! | `pages/dir/index.md`    | `/dir/`          |
//! | `static/static.html`    | `/static.html`   |
//! | `static/dir/index.html` | `/dir/`          |

mod tests;
pub mod util;

use crate::actions::{self, RequestVariables, VariableMap};
use crate::config::model::Configuration;
use crate::config::parser2::GeneratedSource;
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
        match rules::default_rules(base_dir, template_dir) {
            Ok(config) => config,
            Err(errors) => {
                let source = GeneratedSource("default rules");
                crate::config::print_diagnostics(
                    &source,
                    err_stream,
                    &crate::config::errors::errors_to_diagnostics(
                        errors, &source,
                    ),
                );
                return Err(crate::Error::ExitWithCode(ExitCode::FAILURE));
            }
        },
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

    /// Route a request
    ///
    /// Uses hard coded rules from [`rules::default_rules()`].
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
