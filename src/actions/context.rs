//! Context information for actions

use actix_web::HttpRequest;
use handlebars::Handlebars;
use std::path::PathBuf;
use std::sync::Arc;

/// Variable names available to be used in configuration.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum Variable {
    /// The path of the request, not including a query string.
    Path,

    /// The HTTP verb used, e.g. `"GET"`.
    Verb,

    /// The `Host` header, or `""` if its invalid or not set.
    Host,
}

/// Access variables containing request information used in configuration.
///
/// These can be interpolated into the configuration, e.g. `/srv/$path`.
pub trait VariableMap<'a> {
    /// Get a variable value by name.
    ///
    /// ```
    /// use riki::actions::context::{StaticVariables, Variable, VariableMap};
    ///
    /// let vars = StaticVariables {
    ///     path: "/example",
    ///     verb: "POST",
    ///     ..StaticVariables::default()
    /// };
    /// assert_eq!(vars.get("path".try_into().unwrap()), "/example");
    /// assert_eq!(vars.get(Variable::Verb), "POST");
    /// ```
    fn get(&self, variable: Variable) -> &'a str;
}

/// Static variable values (for testing).
#[derive(Clone, Debug)]
pub struct StaticVariables<'a> {
    /// Request path
    pub path: &'a str,

    /// Request verb
    pub verb: &'a str,

    /// Request host
    pub host: &'a str,
}

impl<'a> VariableMap<'a> for StaticVariables<'a> {
    fn get(&self, variable: Variable) -> &'a str {
        match variable {
            Variable::Path => self.path,
            Variable::Verb => self.verb,
            Variable::Host => self.host,
        }
    }
}

impl Default for StaticVariables<'static> {
    fn default() -> Self {
        Self { path: "/", verb: "GET", host: "localhost" }
    }
}

/// Variables from a request object.
#[derive(Clone, Debug)]
pub struct RequestVariables<'a> {
    /// The request object.
    pub request: &'a HttpRequest,
}

impl<'a> VariableMap<'a> for RequestVariables<'a> {
    fn get(&self, variable: Variable) -> &'a str {
        match variable {
            Variable::Path => self.request.path(),
            Variable::Verb => self.request.method().as_str(),
            Variable::Host => self
                .request
                .headers()
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
        }
    }
}

/// Static context for actions (for testing).
#[derive(Clone, Debug)]
pub struct Context<'a, V: VariableMap<'a>> {
    /// Working directory.
    pub working_path: PathBuf,

    /// Templates for rendering pages.
    pub tpls: Arc<Handlebars<'a>>,

    /// Variables for interpolation into strings.
    pub variables: V,
}

impl<'a, V: VariableMap<'a> + Default> Default for Context<'a, V> {
    fn default() -> Self {
        // FIXME
        Self {
            working_path: PathBuf::default(),
            tpls: Arc::new(Handlebars::default()),
            variables: Default::default(),
        }
    }
}

impl<'a> Context<'a, RequestVariables<'a>> {
    /// Get a new context for a request.
    #[must_use]
    pub const fn new(
        working_path: PathBuf,
        tpls: Arc<Handlebars<'a>>,
        request: &'a HttpRequest,
    ) -> Self {
        Self { working_path, tpls, variables: RequestVariables { request } }
    }
}

/// Convenient alias for a context with static variables.
pub type StaticContext = Context<'static, StaticVariables<'static>>;

/// Convenient alias for a context with variables from a request.
pub type RequestContext<'a> = Context<'a, RequestVariables<'a>>;
