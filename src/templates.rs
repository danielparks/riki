//! Manage templates

use crate::errors::{Error, Result};
use mustache::Template;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Load, compile, and cache templates
#[derive(Clone, Debug)]
pub struct TemplateManager {
    /// Cache of templates by name
    templates: HashMap<String, Template>,
}

impl Default for TemplateManager {
    /// Create a `TemplateManager` with only the built-in templates.
    fn default() -> Self {
        let mut manager = Self::new();
        manager.ensure_basic_templates();
        manager
    }
}

impl TemplateManager {
    /// Create an empty `TemplateManager`.
    #[must_use]
    pub fn new() -> Self {
        Self { templates: HashMap::new() }
    }

    /// Create a new `TemplateManager` from templates in a directory.
    ///
    /// # Errors
    ///
    /// This will return [`Error::Io`] if `directory` does not contain a path
    /// to a directory.
    pub fn from_directory<P: AsRef<Path>>(directory: P) -> Result<Self> {
        let mut manager = Self::new();
        manager.load_from_directory(directory)?;
        manager.ensure_basic_templates();

        Ok(manager)
    }

    /// Ensure that the basic templates, e.g. default, are loaded.
    ///
    /// This will **not** replace existing templates. It uses simple
    /// templates compiled into the executable.
    ///
    /// # Panics
    ///
    /// This will panic if [`mustache`] cannot compile one of the built-in
    /// templates. See [`mustache::compile_str()`].
    pub fn ensure_basic_templates(&mut self) {
        if !self.has("default") {
            self.load_from_string(
                "default",
                include_str!("templates/default.tmpl"),
            )
            .unwrap();
        }

        if !self.has("error404") {
            self.load_from_string(
                "error404",
                include_str!("templates/error404.tmpl"),
            )
            .unwrap();
        }

        if !self.has("error500") {
            self.load_from_string(
                "error500",
                include_str!("templates/error500.tmpl"),
            )
            .unwrap();
        }
    }

    /// Load all the .tmpl files under a directory.
    ///
    /// # Errors
    ///
    ///   * [`Error::Io`] if `directory` is not a path to a directory.
    ///   * [`Error::TemplateName`] if a template name would contain any
    ///     non-Unicode characters.
    ///   * [`Error::TemplateCompile`] if [`mustache`] cannot compile a
    ///     template. See [`mustache::compile_path()`].
    pub fn load_from_directory<P: AsRef<Path>>(
        &mut self,
        directory: P,
    ) -> Result<()> {
        let directory = directory.as_ref();
        self.load_from_subdirectory(directory, directory)
    }

    /// Recusively load all the .tmpl files in the `directory` subdirectoy of
    /// the `root` template directory.
    ///
    /// # Errors
    ///
    ///   * [`Error::Io`] if `directory` is not a path to a directory.
    ///   * [`Error::TemplateName`] if a template name would contain any
    ///     non-Unicode characters.
    ///   * [`Error::TemplateCompile`] if [`mustache`] cannot compile a
    ///     template. See [`mustache::compile_path()`].
    ///
    /// # Panics
    ///
    /// This will panic if `root` is not an ancestor of `directory` (or the same
    /// as `directory`).
    fn load_from_subdirectory(
        &mut self,
        root: &Path,
        directory: &Path,
    ) -> Result<()> {
        for entry in directory.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.load_from_subdirectory(root, &path)?;
            } else if path.extension().is_some_and(|ext| ext == "tmpl") {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or_else(|_| {
                        panic!("root {root:?} is not an ancestor of {path:?}")
                    })
                    .with_extension("");
                let name = name.to_str().ok_or_else(|| {
                    Error::TemplateName { path: path.clone() }
                })?;
                self.load_from_path(name, path)?;
            }
        }

        Ok(())
    }

    /// Get a template by name.
    ///
    /// # Errors
    ///
    ///   * [`Error::TemplateNotFound`] if no template is found.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Result<&Template> {
        self.templates.get(name.as_ref()).ok_or_else(|| {
            Error::TemplateNotFound { name: name.as_ref().to_owned() }
        })
    }

    /// Check if a template has been loaded.
    pub fn has<S: AsRef<str>>(&self, name: S) -> bool {
        self.templates.contains_key(name.as_ref())
    }

    // This lint doesn’t know ToString can be by reference:
    #[allow(clippy::needless_pass_by_value)]
    /// Load and compile a template from a string.
    ///
    /// If the template has already been loaded then it will be reloaded.
    ///
    /// # Errors
    ///
    /// This will return [`Error::TemplateCompile`] if [`mustache`] cannot
    /// compile the template. See [`mustache::compile_str()`].
    pub fn load_from_string<N: ToString, T: AsRef<str>>(
        &mut self,
        name: N,
        template: T,
    ) -> Result<()> {
        let name = name.to_string();

        tracing::debug!("Loading template {name:?} from memory");
        self.templates.insert(
            name,
            mustache::compile_str(template.as_ref()).map_err(|error| {
                Error::TemplateCompile {
                    source: error,
                    path: PathBuf::from("-"), // FIXME?
                }
            })?,
        );

        Ok(())
    }

    // This lint doesn’t know ToString can be by reference:
    #[allow(clippy::needless_pass_by_value)]
    /// Load and compile a template from a file.
    ///
    /// If the template has already been loaded then it will be reloaded.
    ///
    /// # Errors
    ///
    /// This will return [`Error::TemplateCompile`] if [`mustache`] cannot
    /// compile the template. See [`mustache::compile_path()`].
    pub fn load_from_path<N: ToString, P: AsRef<Path>>(
        &mut self,
        name: N,
        path: P,
    ) -> Result<()> {
        let name = name.to_string();
        let path = path.as_ref();

        tracing::debug!("Loading template {name:?} from {path:?}");
        self.templates.insert(
            name,
            mustache::compile_path(path).map_err(|error| {
                Error::TemplateCompile {
                    source: error,
                    path: PathBuf::from(path),
                }
            })?,
        );

        Ok(())
    }
}
