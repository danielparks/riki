//! Manage templates

use crate::errors::{Error, Result};
use mustache::Template;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// Load, compile, and cache templates
#[derive(Clone, Debug)]
pub struct TemplateManager {
    /// The directory containing the templates
    directory: PathBuf,
    /// Cache of templates by name
    templates: HashMap<String, Template>,
}

impl TemplateManager {
    /// Create a new `TemplateManager` from templates in a directory.
    ///
    /// # Errors
    ///
    /// This will return [`Error::Io`] if `directory` does not contain the path
    /// to a directory.
    pub fn new<P: AsRef<Path>>(directory: P) -> Result<Self> {
        let manager = Self {
            directory: directory.as_ref().to_path_buf(),
            templates: HashMap::new(),
        };

        if manager.directory.is_dir() {
            Ok(manager)
        } else {
            Err(Error::Io(io::Error::other(format!(
                "Loading templates: \"{}\" is not a directory",
                &manager.directory.display()
            ))))
        }
    }

    /// Get the default template.
    ///
    /// # Errors
    ///
    /// This will return [`Error::PageRender`] if [`mustache`] cannot compile the
    /// template. See [`mustache::compile_path()`].
    pub fn default(&mut self) -> Result<&Template> {
        self.get(&"default")
    }

    /// Get a template by name.
    ///
    /// # Errors
    ///
    /// This will return [`Error::PageRender`] if [`mustache`] cannot compile the
    /// template. See [`mustache::compile_path()`].
    pub fn get<S: AsRef<str>>(&mut self, name: &S) -> Result<&Template> {
        let name = name.as_ref();
        if self.templates.contains_key(name) {
            return Ok(&self.templates[name]);
        }

        self.load(&name)
    }

    /// Load and compile a template
    ///
    /// If the template has already been loaded then it will be reloaded.
    ///
    /// # Errors
    ///
    /// This will return [`Error::PageRender`] if [`mustache`] cannot compile the
    /// template. See [`mustache::compile_path()`].
    pub fn load<S: AsRef<str>>(&mut self, name: &S) -> Result<&Template> {
        let name = name.as_ref();

        // FIXME? just trust that name doesn’t contain '/'
        let mut path = self.directory.join(name);
        path.set_extension("tmpl");
        self.templates
            .insert(name.to_owned(), mustache::compile_path(&path)?);

        Ok(&self.templates[name])
    }
}
