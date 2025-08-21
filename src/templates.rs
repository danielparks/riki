//! Manage templates

use crate::errors::{Error, Result};
use mustache::Template;
use std::collections::HashMap;
use std::io;
use std::path::Path;

/// Load, compile, and cache templates
#[derive(Clone, Debug)]
pub struct TemplateManager {
    /// Cache of templates by name
    templates: HashMap<String, Template>,
}

impl Default for TemplateManager {
    /// Create an empty `TemplateManager`.
    fn default() -> Self {
        Self { templates: HashMap::new() }
    }
}
impl TemplateManager {
    /// Create a new `TemplateManager` from templates in a directory.
    ///
    /// # Errors
    ///
    /// This will return [`Error::Io`] if `directory` does not contain a path
    /// to a directory.
    pub fn new<P: AsRef<Path>>(directory: P) -> Result<Self> {
        let mut manager = Self::default();

        let directory = directory.as_ref();
        if !directory.is_dir() {
            return Err(Error::Io(io::Error::other(format!(
                "Loading templates: {directory:?} is not a directory"
            ))));
        }

        manager.load_from_directory(directory)?;
        Ok(manager)
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
                tracing::debug!("Loading template {name:?} from {path:?}");
                self.templates.insert(
                    name.to_owned(),
                    mustache::compile_path(&path).map_err(|error| {
                        Error::TemplateCompile { source: error, path }
                    })?,
                );
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
}
