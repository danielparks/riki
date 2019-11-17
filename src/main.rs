extern crate mustache;
extern crate clap;

use clap::{Arg, App, AppSettings, SubCommand};
use std::collections::HashMap;
use std::env;
use std::fs;
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
        .setting(AppSettings::SubcommandRequired)
        .arg(Arg::with_name("base")
            .short("b")
            .long("base")
            .value_name("DIR")
            .help("Base of directory tree containing templates and pages"))
        .subcommand(SubCommand::with_name("render")
            .arg(Arg::with_name("template")
                .short("t")
                .long("template")
                .value_name("FILE")
                .default_value("templates/default.tmpl")
                .help("Template to use"))
            .arg(Arg::with_name("page")
                .value_name("PAGE")
                .required(true)
                .help("Page to render")));
    let matches = app.get_matches();

    let base = matches.value_of("base");
    if ! base.is_none() {
        let base_path = Path::new(base.unwrap());
        assert!(env::set_current_dir(&base_path).is_ok());
    }

    if let Some(render_matches) = matches.subcommand_matches("render") {
        let template_path = render_matches.value_of("template").unwrap();
        let template = mustache::compile_path(template_path).unwrap();

        let page_path = render_matches.value_of("page").unwrap();
        let page_raw = fs::read_to_string(page_path).unwrap();

        let mut data = HashMap::new();
        data.insert("title", "hello <b>world</b>");
        data.insert("body", &page_raw);

        template.render(&mut io::stdout(), &data).unwrap();
    }
}
