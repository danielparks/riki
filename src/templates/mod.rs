//! Manage templates

use crate::embeds;
use crate::errors::{Error, Result};
use handlebars::{DirectorySourceOptions, Handlebars};
use std::path::Path;

mod helpers;

/// Get the basic template registry.
///
/// # Panics
///
/// Panics if there is an error compiling the embedded templates.
#[must_use]
pub fn templates() -> Handlebars<'static> {
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

/// Convenience function get template registry loaded from a directory.
///
/// # Errors
///
///   * [`Error::TemplateCompile`] if a template fails to compile.
pub fn templates_from_directory<P: AsRef<Path>>(
    path: P,
) -> Result<Handlebars<'static>> {
    let path = path.as_ref();
    if !path.is_dir() {
        return Err(Error::MissingDirectory(path.to_path_buf()));
    }

    let mut tpls = templates();
    tpls.register_templates_directory(path, DirectorySourceOptions::default())?;
    Ok(tpls)
}
