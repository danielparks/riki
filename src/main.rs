use anyhow::Result as AnyResult;
use std::env;
use std::io;
use std::process::exit;

use rustwiki::*;

mod logging;
mod params;

use params::{Command, Params, Parser};

fn cli(params: Params) -> AnyResult<()> {
    logging::init(params.verbose)?;

    match params.command {
        Command::Render { template, page } => {
            let template = mustache::compile_path(&template)?;
            let page = Page::read_from(&page)?;

            template.render(&mut io::stdout(), &page)?;
        }
        Command::Info { page } => {
            let metadata = Page::read_from(&page)?.metadata_as_string()?;
            if metadata != "" {
                println!("{}", metadata);
            }
        }
        Command::Serve { basedir } => {
            // Switch to base directory. The default is ".".
            env::set_current_dir(&basedir)?;

            rustwiki::http::serve("127.0.0.1:8000")?;
        }
    }

    Ok(())
}

fn main() {
    if let Err(error) = cli(Params::parse()) {
        eprintln!("Error: {:#}", error);
        exit(1);
    }
}
