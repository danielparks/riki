use mustache::Template;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::errors::{Error, Result};

/// Load, compile, and cache templates
#[derive(Clone, Debug)]
pub struct TemplateManager {
    directory: PathBuf,
    templates: HashMap<String, Template>,
}

impl TemplateManager {
    /// Create a new `TemplateManager` with the passed
    pub fn new<P: AsRef<Path>>(directory: P) -> Result<Self> {
        let manager = Self {
            directory: directory.as_ref().to_path_buf(),
            templates: HashMap::new(),
        };

        if manager.directory.is_dir() {
            Ok(manager)
        } else {
            Err(Error::from(io::Error::other(format!(
                "Loading templates: {:?} is not a directory",
                &manager.directory
            ))))
        }
    }

    pub fn default(&mut self) -> Result<&Template> {
        self.get(&"default")
    }

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
