extern crate mustache;

use std::collections::HashMap;
use std::io;

fn main() {
    // command line arguments
    // render
    // Load template file
    // load page file
    // read page yaml
    // serve

    let mut data = HashMap::new();
    data.insert("title", "hello <b>world</b>");
    data.insert("body", "<p>hello <b>world</b></p>");

    let template = mustache::compile_str("<title>{{title}}</title>\n<body>{{& body}}</body>\n").unwrap();
    template.render(&mut io::stdout(), &data).unwrap();
}
