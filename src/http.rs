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
use crate::render::{TemplatesManager, base_templates};
use crate::rules;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use std::path::Path;
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
    config: Configuration,
    address: S,
) -> crate::Result<()> {
    let address = address.as_ref();

    util::check_dir(&config.root_path)?;

    let router = Data::new(Router::from_configuration(config));

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
    router: Data<Router<'_>>,
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
#[derive(Debug, Default)]
pub struct Router<'tpls> {
    /// Root path and template paths
    config: Configuration,

    /// Manage template registries
    manager: TemplatesManager<'tpls>,
}

impl Router<'_> {
    /// Create a router from a [`Configuration`].
    ///
    /// # Errors
    ///
    /// Returns an error if there is a problem loading templates.
    #[must_use]
    pub fn from_configuration(config: Configuration) -> Self {
        Self { config, manager: TemplatesManager::default() }
    }
}

impl Router<'_> {
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
        let config = rules::default_rules(
            &self.config.root_path,
            &self.config.templates_path,
        )
        .map_err(|error| {
            // FIXME better error -- print and die
            actions::Error::InternalString(format!(
                "Failed to build globs: {error:?}"
            ))
        })?;

        // This errors if clean_path() failed, which should never happen.
        // FIXME? move clean_path() out for explicit error handling?
        let variables = RequestVariables::new(request)?;

        match (|| {
            for rule in config.matches(&variables.clean_path()) {
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
                    config.last_matching(&variables.clean_path())
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

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Configuration {
    /// The path to the directory containing pages and static assets.
    pub root_path: String,
    /// The path to the directory containing templates.
    pub templates_path: String,
}

impl Default for Configuration {
    /// Create a configuration using the default subdirectories names in the
    /// current directory.
    fn default() -> Self {
        Self::default_in(".")
    }
}

impl Configuration {
    /// Create a configuration using the default subdirectories under `root`.
    pub fn default_in<S: Into<String>>(root: S) -> Self {
        let root_path = root.into();
        let templates_path = format!("{root_path}/templates");
        Self { root_path, templates_path }
    }

    /// Get `self.root_path` as a `Path`.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.root_path.as_ref()
    }

    /// Get `self.templates_path` as a `Path`.
    #[must_use]
    pub fn templates(&self) -> &Path {
        self.templates_path.as_ref()
    }
}
