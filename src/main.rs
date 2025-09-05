//! riki executable.

mod logging;
mod params;

use anyhow::bail;
use handlebars::Handlebars;
use params::{Command, Params, Parser};
use riki::Page;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Wrapper to handle errors.
///
/// See [`cli()`].
fn main() -> ExitCode {
    let params = Params::parse();
    cli(&params).unwrap_or_else(|error| {
        params.warn(format!("Error: {error:#}\n")).unwrap();
        ExitCode::FAILURE
    })
}

/// Do the actual work.
///
/// Returns the exit code to use.
///
/// # Errors
///
/// This returns any errors encountered during the run so that they can be
/// outputted nicely in [`main()`].
fn cli(params: &Params) -> anyhow::Result<ExitCode> {
    logging::init(params.verbose)?;

    match &params.command {
        Command::Render { template, page } => {
            let template_name = template.to_string_lossy();
            let mut tpls = Handlebars::new();
            tpls.register_template_file(
                &template_name,
                find_template(template, page)?,
            )?;
            let page = Page::read_from(page)?;

            print!("{}", tpls.render(&template_name, &page)?);
        }
        Command::Info { page } => {
            let metadata = Page::read_from(page)?.metadata_as_string()?;
            if !metadata.is_empty() {
                println!("{metadata}");
            }
        }
        Command::Serve { basedir, bind } => {
            riki::http::serve(basedir, bind)?;
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Find the correct template path.
///
/// If `template_path` doesn’t exist, try looking relative to the “pages”
/// directory parent of `page_path` (if it exists).
///
/// # Errors
///
/// Returns [`anyhow::Error`] if it can’t find the template path.
fn find_template(
    template_path: &Path,
    page_path: &Path,
) -> anyhow::Result<PathBuf> {
    if template_path.exists() {
        return Ok(template_path.to_path_buf());
    }

    if let Some(pages_dir) = page_path
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new("pages")))
    {
        if let Some(parent) = pages_dir.parent() {
            let new_template_path = parent.join(template_path);
            if new_template_path.exists() {
                return Ok(new_template_path);
            }
        }
    }

    bail!("{template_path:?} does not exist.")
}
