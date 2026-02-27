//! riki executable.

mod logging;
mod params;

use anyhow::anyhow;
use params::{Command, Params, Parser};
use riki::actions::is_not_found;
use riki::actions::{RealFileReturn, StaticContext};
use riki::{actions, config, http, render, rules};
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
            let context = StaticContext {
                tpls: render::templates_from_directory(
                    templates_dir
                        .clone()
                        .or_else(|| find_templates_dir(page_path))
                        .ok_or_else(|| {
                            anyhow!("Could not find templates directory")
                        })?,
                )?
                .into(),
                ..StaticContext::default()
            };

            print!(
                "{}",
                actions::render(
                    &context,
                    actions::markdown_to_html(
                        &context,
                        RealFileReturn::from_file_system(page_path)?
                    )?
                )?
                .body
                .into_string()?
            );
        }
        Command::Info { page_path } => {
            print!(
                "{}",
                serde_yaml::to_string(
                    &actions::markdown_to_html(
                        &StaticContext::default(),
                        RealFileReturn::from_file_system(page_path)?,
                    )?
                    .metadata
                )?
            );
        }
        Command::Serve { base_dir, bind } => {
            check_dir(base_dir)?;
            let template_dir = format!("{base_dir}/templates");
            match rules::default_rules(base_dir.clone(), template_dir) {
                Ok(configuration) => http::serve(configuration, bind)?,
                Err(diagnostics) => {
                    config::print_diagnostics(
                        &rules::SOURCE,
                        &params.err_stream(),
                        &diagnostics,
                    );
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        Command::Dump { path, kind } => {
            let source = config::parser2::FileSource::read(path)?;
            match if kind.tokens {
                config::dump_config_tokens(&source)
            } else if kind.cst {
                config::parse_cst(&source).map(|cst| println!("{cst}"))
            } else {
                config::parse(&source)
                    .map(|rules| config::dump_canonical(&rules))
            } {
                Ok(()) => {}
                Err(diagnostics) => {
                    config::print_diagnostics(
                        &source,
                        &params.err_stream(),
                        &diagnostics,
                    );
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        Command::DumpDefaultRules { root, templates } => {
            match rules::default_rules(root.clone(), templates.clone()) {
                Ok(rules) => config::dump_canonical(&rules),
                Err(diagnostics) => {
                    config::print_diagnostics(
                        &rules::SOURCE,
                        &params.err_stream(),
                        &diagnostics,
                    );
                    return Ok(ExitCode::FAILURE);
                }
            }
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

/// Check that path is a directory or a symlink that resolves to a directory.
///
/// # Errors
///
///   * [`riki::Error::MissingDirectory`] not a directory or doesn’t exist.
///   * [`riki::Error::Io`] some other problem getting info about `path`.
pub fn check_dir<P: AsRef<Path>>(path: P) -> riki::Result<()> {
    let path = path.as_ref();
    match path.metadata().map(|m| m.is_dir()) {
        Ok(true) => Ok(()),
        Err(error) if !is_not_found(&error) => Err(riki::Error::Io(error)),
        _ => Err(riki::Error::MissingDirectory(path.to_path_buf())),
    }
}
