//! riki executable.

mod logging;
mod params;

use anyhow::anyhow;
use params::{Command, Params, Parser};
use riki::{Page, templates_from_directory};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Wrapper to handle errors.
///
/// See [`cli()`].
fn main() -> ExitCode {
    let params = Params::parse();
    cli(&params).unwrap_or_else(|error| {
        tracing::debug!("Exiting with error: {error:#?}");
        let error = format!("{error}\n");
        if error.to_lowercase().starts_with("error") {
            params.warn(error).unwrap();
        } else {
            params.warn(format!("Error: {error}")).unwrap();
        }

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
        Command::Render { template_dir, page } => {
            let template_dir = template_dir
                .clone()
                .or_else(|| find_template_dir(page))
                .ok_or_else(|| anyhow!("Could not find templates directory"))?;
            let tpls = templates_from_directory(template_dir)?;
            let page = Page::read_from(page)?;

            print!("{}", page.render_to_string(&tpls)?);
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

/// Find the template directory.
///
/// This looks for a “templates” directory that’s a sibling to a “pages”
/// directory that’s an ancestor of `page_path`. Returns `None` if it can’t find
/// the directory.
fn find_template_dir(page_path: &Path) -> Option<PathBuf> {
    Some(
        page_path
            .ancestors()
            .find(|path| path.file_name() == Some(OsStr::new("pages")))?
            .parent()?
            .join("templates"),
    )
}
