#[macro_use]
extern crate clap;

extern crate mustache;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clap)]
struct Params {
    /// Base of directory tree containing templates and pages
    #[clap(short = "b", long = "base", default_value = ".")]
    base: String,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clap)]
enum Command {
    /// Render a page file
    #[clap(name = "render")]
    Render {
        /// Path to template to use
        #[clap(short = "t", long = "template", default_value = "templates/default.tmpl")]
        template: String,
        /// Path to page file to render
        page: String,
    },
}


fn main() {
    // read page yaml
    // serve
    let params: Params = Params::parse();

    if params.base != "." {
        let base_path = Path::new(&params.base);
        assert!(env::set_current_dir(&base_path).is_ok());
    }

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
