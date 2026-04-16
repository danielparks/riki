//! Manage templates.

use crate::embeds;
use crate::errors::{Error, Result};
use handlebars::{DirectorySourceOptions, Handlebars};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

mod helpers;

/// Manage template registries.
///
/// This keeps track of the live registry object ([`Handlebars`]) for each
/// template directory that we use.
#[derive(Debug, Default)]
pub struct TemplatesManager {
    /// A map of template directories to template registries.
    by_path: RwLock<HashMap<PathBuf, Arc<Handlebars<'static>>>>,
}

impl TemplatesManager {
    /// Get the template registry for a directory.
    ///
    /// This will create a new registry if it doesn’t already exist.
    ///
    /// # Errors
    ///
    /// If this returns an error, no entry will be cached.
    ///
    ///   * [`Error::MissingDirectory`] if `path` isn’t a directory.
    ///   * [`Error::TemplateCompile`] if a template fails to compile.
    ///
    /// # Panics
    ///
    /// Panics if there was an error compiling the embedded templates.
    pub fn templates_for_directory<P: Into<PathBuf>>(
        &self,
        path: P,
    ) -> Result<Arc<Handlebars<'static>>> {
        let path = path.into();
        if !path.is_dir() {
            return Err(Error::MissingDirectory(path));
        }

        if let Some(tpls) = self.by_path.read().unwrap().get(&path) {
            Ok(Arc::clone(tpls))
        } else {
            let tpls = Arc::new(templates_from_directory(&path)?);
            self.by_path
                .write()
                .unwrap()
                .insert(path, Arc::clone(&tpls));
            Ok(tpls)
        }
    }
}

/// Get a copy of the base template registry.
///
/// # Panics
///
/// Panics if there is an error compiling the embedded templates.
#[must_use]
pub fn base_templates() -> Handlebars<'static> {
    // FIXME is it faster to clone the base templates? If so, use LazyLock.
    let mut tpls = Handlebars::new();
    tpls.set_strict_mode(true);

    helpers::register(&mut tpls);

    // Embedded templates will override future templates if dev mode is turned
    // on before they’re registered.
    tpls.register_embed_templates_with_extension::<embeds::Templates>(".hbs")
        .unwrap();
    tpls.set_dev_mode(true);

    tpls
}

/// Load a new template registry from a directory.
///
/// # Errors
///
///   * [`Error::MissingDirectory`] if `path` isn’t a directory.
///   * [`Error::TemplateCompile`] if a template fails to compile.
///
/// # Panics
///
/// Panics if there is an error compiling the embedded templates.
pub fn templates_from_directory<P: AsRef<Path>>(
    path: P,
) -> Result<Handlebars<'static>> {
    let path = path.as_ref();
    if !path.is_dir() {
        return Err(Error::MissingDirectory(path.to_path_buf()));
    }

    let mut tpls = base_templates();
    tpls.register_templates_directory(path, DirectorySourceOptions::default())?;
    Ok(tpls)
}
