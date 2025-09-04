//! Code to deal with executable parameters.
#![allow(clippy::allow_attributes, reason = "framework code from a template")]

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

pub use clap::Parser;

/// Parameters to configure executable.
#[derive(Debug, clap::Parser)]
#[command(version, about)]
pub struct Params {
    /// Whether or not to output in color
    #[arg(long, default_value = "auto", value_name = "WHEN")]
    pub color: ColorChoice,

    /// Verbosity (may be repeated up to three times)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The command to run
    #[clap(subcommand)]
    pub command: Command,
}

/// The command to run.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Render a page file
    Render {
        /// Path to template to use
        #[arg(short, long, default_value = "templates/default.hbs")]
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

        /// Address to bind to
        #[arg(long, default_value = "localhost:8000")]
        bind: String,
    },
}

impl Params {
    /// Print a warning message in error color to `err_stream()`.
    pub fn warn<S: AsRef<str>>(&self, message: S) -> io::Result<()> {
        let mut err_out = self.err_stream();
        err_out.set_color(&error_color())?;
        err_out.write_all(message.as_ref().as_bytes())?;
        err_out.reset()?;

        Ok(())
    }

    /// Get stream to use for standard output.
    #[allow(dead_code, reason = "framework code")]
    pub fn out_stream(&self) -> StandardStream {
        StandardStream::stdout(self.color_choice(&io::stdout()))
    }

    /// Get stream to use for errors.
    pub fn err_stream(&self) -> StandardStream {
        StandardStream::stderr(self.color_choice(&io::stderr()))
    }

    /// Whether or not to output on a stream in color.
    ///
    /// Checks if passed stream is a terminal.
    pub fn color_choice<T: IsTerminal>(
        &self,
        stream: &T,
    ) -> termcolor::ColorChoice {
        if self.color == ColorChoice::Auto && !stream.is_terminal() {
            termcolor::ColorChoice::Never
        } else {
            self.color.into()
        }
    }
}

/// Whether or not to output in color
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum ColorChoice {
    /// Output in color when running in a terminal that supports it
    Auto,

    /// Always output in color
    Always,

    /// Never output in color
    Never,
}

impl Default for ColorChoice {
    fn default() -> Self {
        Self::Auto
    }
}

impl From<ColorChoice> for termcolor::ColorChoice {
    fn from(choice: ColorChoice) -> Self {
        match choice {
            ColorChoice::Auto => Self::Auto,
            ColorChoice::Always => Self::Always,
            ColorChoice::Never => Self::Never,
        }
    }
}

/// Returns color used to output errors.
pub fn error_color() -> ColorSpec {
    let mut color = ColorSpec::new();
    color.set_fg(Some(Color::Red));
    color.set_intense(true);
    color
}
