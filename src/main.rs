use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

use rustwiki::*;

#[derive(Debug, StructOpt)]
struct Params {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Render a page file
    #[structopt(name="render", no_version)]
    Render {
        /// Path to template to use
        #[structopt(short, long, default_value="templates/default.tmpl", parse(from_os_str))]
        template: PathBuf,
        /// Path to page file to render
        #[structopt(parse(from_os_str))]
        page: PathBuf,
    },
    /// Get metadata from a page file
    #[structopt(name="info", no_version)]
    Info {
        /// Path to page file
        #[structopt(parse(from_os_str))]
        page: PathBuf,
    },
    /// Start web server
    #[structopt(name="serve", no_version)]
    Serve {
        /// Directory tree containing templates and pages
        #[structopt(name="path", default_value=".", parse(from_os_str))]
        basedir: PathBuf,
    },
}

fn cli(params: Params) -> Result<()> {

    match params.command {
        Command::Render{template, page} => {
            let template = mustache::compile_path(&template)?;
            let page = Page::read_from(&page)?;

            template.render(&mut io::stdout(), &page)?;
        }
        Command::Info{page} => {
            let metadata = Page::read_from(&page)?.metadata_as_string()?;
            if metadata != "" {
                println!("{}", metadata);
            }
        }
        Command::Serve{basedir} => {
            // Switch to base directory. The default is ".".
            env::set_current_dir(&basedir)?;

            rustwiki::http::serve()?;
        }
    }

    Ok(())
}

fn main() {
    if let Err(error) = cli(Params::from_args()) {
        eprintln!("{}:", error);
        if let Some(source) = error.source() {
            eprintln!("    {}", source);
        }

        exit(1);
    }
}
