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
    data.insert("name", "world");

    let template = mustache::compile_str("hello {{name}}\n").unwrap();
    template.render(&mut io::stdout(), &data).unwrap();
}
