use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;

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

#[derive(Debug)]
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

    fn read_from(path: &Path) -> Result<Page, io::Error> {
        let raw = fs::read_to_string(path)?;
        let (header, body) = Page::split_raw_page(&raw);

        let mut page = Page {
            file: PathBuf::from(path),
            metadata: HashMap::new(),
            body: body.to_string(),
        };

        match serde_yaml::from_str(&header) {
            Ok(hash) => {
                page.metadata = hash;
            }
            Err(e) => {
                // FIXME! this should ignore EndOfStream and return all other errors.
                println!("Error while parsing metadata: {}", e);
            }
        }

        Ok(page)
    }
}

fn main() {
    // read page yaml
    // serve
    let params = Params::from_args();

    // Switch to base directory. The default of "." results in a no-op.
    assert!(env::set_current_dir(&params.base).is_ok());

    match params.command {
        Command::Render{template, page} => {
            let template = mustache::compile_path(&template).unwrap();
            let page_raw = fs::read_to_string(&page).unwrap();

            let mut data = HashMap::new();
            data.insert("title", "hello <b>world</b>");
            data.insert("body", &page_raw);

            template.render(&mut io::stdout(), &data).unwrap();
        }
        Command::Info{page} => {
            let metadata = Page::read_from(&page).unwrap().metadata;
            let yaml = serde_yaml::to_string(&metadata).unwrap();

            let prefix = "---\n";
            let start = if yaml.starts_with(prefix) { prefix.len() } else { 0 };

            if &yaml[start..] != "{}" {
                // Metadata isn’t empty.
                println!("{}", &yaml[start..]);
            }
        }
    }
}
