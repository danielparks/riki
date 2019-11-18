extern crate mustache;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Params {
    /// Directory tree containing templates and pages
    #[structopt(short, long, default_value=".", hide_default_value=true, parse(from_os_str))]
    base: PathBuf,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
enum Command {
    /// Render a page file
    #[structopt(name="render", no_version)]
    Render {
        /// Path to template to use
        #[structopt(short, long, default_value="templates/default.tmpl", parse(from_os_str))]
        template: PathBuf,
        /// Path to page file to render
        page: String,
    },
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
    }
}
