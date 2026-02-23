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

use crate::actions::{self, RequestContext, Return};
use crate::render::{TemplatesManager, base_templates};
use crate::rules;
use actix_web::{
    self, App, HttpRequest, HttpResponse, HttpServer, Responder, get, web::Data,
};
use std::path::PathBuf;
use tracing;
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
            tracing::error!("{}: {error:?}", req.path());
            match router.context(&req) {
                Ok(context) => error.render(&req, &context.tpls),
                Err(error2) => {
                    tracing::error!(
                        "{} failed to get context: {error2:?}",
                        req.path()
                    );
                    error.render(&req, &base_templates())
                }
            }
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

impl<'tpls> Router<'tpls> {
    /// Get the [`actions::Context`] for a request.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured templates directory doesn’t exist or
    /// a template fails to compile.
    pub fn context<'req>(
        &self,
        req: &'req HttpRequest,
    ) -> crate::Result<RequestContext<'req>>
    where
        'tpls: 'req,
    {
        RequestContext::new(
            self.config.root_path.clone(),
            self.manager
                .templates_for_directory(&self.config.templates_path)?,
            req,
        )
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
        let context = self.context(request)?;
        tracing::trace!("route request: {:?}", request);
        for rule in rules::default_rules() {
            match rule.evaluate(&context) {
                Ok(ret) => {
                    tracing::trace!("success {}: {ret:?}", rule.canonical());
                    return ret.into_response(&context);
                }
                Err(actions::Error::NotFound) => {
                    tracing::trace!("skip {}", rule.canonical());
                }
                Err(error) => {
                    tracing::trace!("error {}: {error:?}", rule.canonical());
                    return Err(error);
                }
            }
        }
        Err(actions::Error::NotFound)
    }
}

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Configuration {
    /// The path to the directory containing pages and static assets.
    pub root_path: PathBuf,
    /// The path to the directory containing templates.
    pub templates_path: PathBuf,
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
    pub fn default_in<P: Into<PathBuf>>(root: P) -> Self {
        let root: PathBuf = root.into();
        Self { templates_path: root.join("templates"), root_path: root }
    }
}
