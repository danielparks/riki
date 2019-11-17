extern crate mustache;
extern crate clap;

use clap::{Arg, App, SubCommand};
use std::collections::HashMap;
use std::io;

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
        .arg(Arg::with_name("template")
            .short("t")
            .long("template")
            .value_name("FILE")
            .help("Template to use")
            .takes_value(true));
    let matches = app.get_matches();

    let template_path = matches.value_of("template")
        .unwrap_or("templates/default.tmpl");
    let template = mustache::compile_path(template_path).unwrap();

    let mut data = HashMap::new();
    data.insert("title", "hello <b>world</b>");
    data.insert("body", "<p>hello <b>world</b></p>");

    template.render(&mut io::stdout(), &data).unwrap();
}
