//! rustwiki executable.

use std::env;
use std::io;
use std::process::ExitCode;

use rustwiki::Page;

mod logging;
mod params;

use params::{Command, Params, Parser};

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
            let template = mustache::compile_path(template)?;
            let page = Page::read_from(page)?;

            template.render(&mut io::stdout(), &page)?;
        }
        Command::Info { page } => {
            let metadata = Page::read_from(page)?.metadata_as_string()?;
            if !metadata.is_empty() {
                println!("{metadata}");
            }
        }
        Command::Serve { basedir, bind } => {
            // Switch to base directory. The default is ".".
            env::set_current_dir(basedir)?;

            rustwiki::http::serve(bind)?;
        }
    }

    Ok(ExitCode::SUCCESS)
}
