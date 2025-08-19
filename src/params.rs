//! Code to deal with executable parameters.

use std::path::PathBuf;

pub use clap::Parser;

/// Parameters to configure executable.
#[derive(Debug, clap::Parser)]
#[command(version, about)]
pub struct Params {
    /// Verbosity (may be repeated up to three times)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Render a page file
    Render {
        /// Path to template to use
        #[arg(short, long, default_value = "templates/default.tmpl")]
        template: PathBuf,
        /// Path to page file to render
        page: PathBuf,
    },
    /// Get metadata from a page file
    Info {
        /// Path to page file
        page: PathBuf,
    },
    /// Start web server
    Serve {
        /// Directory tree containing templates and pages
        #[arg(name = "path", default_value = ".")]
        basedir: PathBuf,
    },
}
