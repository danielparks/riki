use pulldown_cmark::{Parser, Options, html};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use thiserror::Error;

#[derive(Debug, StructOpt)]
struct Params {
    /// Directory tree containing templates and pages
    #[structopt(short, long, default_value=".", hide_default_value=true, parse(from_os_str))]
    base: PathBuf,
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
}

#[derive(Debug, Error)]
enum MyError {
    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("failed rendering page body")]
    PageRender(#[from] mustache::Error),

    #[error("failed rendering page metadata")]
    MetadataRender(#[from] serde_yaml::Error),

    #[error("failed reading page {path:?}")]
    ReadPageFile { source: io::Error, path: PathBuf },

    #[error("failed parsing page metadata in {path:?}")]
    ParsePageMetadata { source: serde_yaml::Error, path: PathBuf },
}

type Result<T, E = MyError> = std::result::Result<T, E>;

#[derive(Debug, Serialize)]
struct Page {
    file: std::path::PathBuf,
    metadata: HashMap<String, String>,
    body: String,
}

impl Page {
    fn split_raw_page(raw: &str) -> (&str, &str) {
        let re = Regex::new(r"(?:^|[\r\n]+)---(?:$|[\r\n]+)").unwrap();
        let parts: Vec<&str> = re.splitn(&raw, 2).collect();
        match parts[..] {
            [yaml, body] => (yaml, body),
            [body] => ("", body),
            _ => unreachable!(),
        }
    }

    fn read_from(path: &Path) -> Result<Page> {
        let raw = fs::read_to_string(path)
            .map_err(|e| MyError::ReadPageFile{
                source: e,
                path: path.to_path_buf(),
            })?;

        let (header, body) = Page::split_raw_page(&raw);

        let mut page = Page {
            file: PathBuf::from(path),
            metadata: HashMap::new(),
            body: Page::render_markdown(&body),
        };

        match serde_yaml::from_str(&header) {
            Ok(hash) => {
                page.metadata = hash;
                Ok(page)
            }
            Err(e) => {
                match *(e.0) {
                    serde_yaml::ErrorImpl::EndOfStream => Ok(page),
                    _ => Err(MyError::ParsePageMetadata{
                            source: e,
                            path: path.to_path_buf(),
                        }),
                }
            }
        }
    }

    fn render_markdown(markdown: &str) -> String {
        let mut buffer = String::new();
        let parser = Parser::new_ext(&markdown, Options::all());
        html::push_html(&mut buffer, parser);

        buffer
    }
}

fn cli(params: Params) -> Result<()> {
    // Switch to base directory. The default of "." results in a no-op.
    env::set_current_dir(&params.base)?;

    match params.command {
        Command::Render{template, page} => {
            let template = mustache::compile_path(&template)?;
            let page = Page::read_from(&page)?;

            template.render(&mut io::stdout(), &page)?;
        }
        Command::Info{page} => {
            let metadata = Page::read_from(&page)?.metadata;
            let yaml = serde_yaml::to_string(&metadata)
                .map_err(MyError::MetadataRender)?;

            let prefix = "---\n";
            let start = if yaml.starts_with(prefix) { prefix.len() } else { 0 };

            if &yaml[start..] != "{}" {
                // Metadata isn’t empty.
                println!("{}", &yaml[start..]);
            }
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
