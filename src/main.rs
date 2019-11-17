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
    sub_command: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    /// Render a page file
    #[clap(name = "render")]
    Render(Render),
}

#[derive(Clap)]
struct Render {
    /// Path to template to use
    #[clap(short = "t", long = "template", default_value = "templates/default.tmpl")]
    template: String,
    /// Path to page file to render
    page: String,
}


fn main() {
    // read page yaml
    // serve
    let params: Params = Params::parse();

    if params.base != "." {
        let base_path = Path::new(&params.base);
        assert!(env::set_current_dir(&base_path).is_ok());
    }

    match params.sub_command {
        SubCommand::Render(sub) => {
            let template = mustache::compile_path(&sub.template).unwrap();
            let page_raw = fs::read_to_string(&sub.page).unwrap();

            let mut data = HashMap::new();
            data.insert("title", "hello <b>world</b>");
            data.insert("body", &page_raw);

            template.render(&mut io::stdout(), &data).unwrap();
        }
    }
}
