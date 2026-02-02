//! riki executable.

mod logging;
mod params;

use anyhow::anyhow;
use params::{Command, Params, Parser};
use riki::actions::PathReturn;
use riki::{actions, http, render};
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
        Command::Render { templates_dir, page_path } => {
            let context = actions::StaticContext {
                tpls: render::templates_from_directory(
                    templates_dir
                        .clone()
                        .or_else(|| find_templates_dir(page_path))
                        .ok_or_else(|| {
                            anyhow!("Could not find templates directory")
                        })?,
                )?
                .into(),
                ..Default::default()
            };

            let ret = PathReturn::new(page_path.clone())?;
            let ret = actions::markdown_to_html(&context, ret)?
                .ok_or(actions::Error::NotFound)?;
            let ret = actions::render(&context, ret)?
                .ok_or(actions::Error::NotFound)?;

            print!("{}", ret.body.into_string()?);
        }
        Command::Info { page_path } => {
            let ret = actions::markdown_to_html(
                &actions::StaticContext::default(),
                PathReturn::new(page_path.clone())?,
            )?
            .ok_or(actions::Error::NotFound)?;
            print!("{}", serde_yaml::to_string(&ret.metadata)?);
        }
        Command::Serve { base_dir, bind } => {
            http::serve(http::Configuration::default_in(base_dir), bind)?;
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Find the template directory.
///
/// This looks for a “templates” directory that’s a sibling to a “pages”
/// directory that’s an ancestor of `page_path`. Returns `None` if it can’t find
/// the directory.
fn find_templates_dir(page_path: &Path) -> Option<PathBuf> {
    Some(
        page_path
            .ancestors()
            .find(|path| path.file_name() == Some(OsStr::new("pages")))?
            .parent()?
            .join("templates"),
    )
}
