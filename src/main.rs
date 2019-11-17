extern crate mustache;
extern crate clap;

use clap::{Arg, App, SubCommand};
use std::collections::HashMap;
use std::env;
use std::io;
use std::path::Path;

fn main() {
    // command line arguments
    // render
    // Load template file
    // load page file
    // read page yaml
    // serve

    let app = App::new("rustwiki")
        .version("0.1.0")
        .about("Markdown wiki in Rust")
        .arg(Arg::with_name("base")
            .short("b")
            .long("base")
            .value_name("DIR")
            .help("Base of directory tree containing templates and pages")
            .takes_value(true))
        .arg(Arg::with_name("template")
            .short("t")
            .long("template")
            .value_name("FILE")
            .help("Template to use")
            .takes_value(true));
    let matches = app.get_matches();

    let base = matches.value_of("base");
    if ! base.is_none() {
        let base_path = Path::new(base.unwrap());
        assert!(env::set_current_dir(&base_path).is_ok());
    }

    let template_path = matches.value_of("template")
        .unwrap_or("templates/default.tmpl");
    let template = mustache::compile_path(template_path).unwrap();

    let mut data = HashMap::new();
    data.insert("title", "hello <b>world</b>");
    data.insert("body", "<p>hello <b>world</b></p>");

    template.render(&mut io::stdout(), &data).unwrap();
}
