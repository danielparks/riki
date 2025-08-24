//! riki executable.

mod logging;
mod params;

use handlebars::Handlebars;
use params::{Command, Params, Parser};
use riki::Page;
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
            let mut tpls = Handlebars::new();
            tpls.register_template_file("tpl", template)?;
            let page = Page::read_from(page)?;

            print!("{}", tpls.render("tpl", &page)?);
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
