use pulldown_cmark::{Parser, Options, html};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::errors::*;

#[derive(Debug, Serialize)]
pub struct Page {
    pub file: std::path::PathBuf,
    pub metadata: HashMap<String, String>,
    pub body: String,
}

impl Page {
    pub fn read_from(path: &Path) -> Result<Page> {
        let raw = fs::read_to_string(path)
            .map_err(MyError::ReadPageFileMap(path))?;

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
                if let serde_yaml::ErrorImpl::EndOfStream = *(e.0) {
                    Ok(page)
                } else {
                    Err(MyError::ParsePageMetadata{
                        source: e,
                        path: path.to_path_buf(),
                    })
                }
            }
        }
    }

    pub fn metadata_as_string(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.metadata)?;

        let prefix = "---\n";
        let start = if yaml.starts_with(prefix) { prefix.len() } else { 0 };

        let mut cleaned = String::new();
        if &yaml[start..] != "{}" {
            // Metadata wasn’t empty.
            cleaned.push_str(&yaml[start..])
        }

        Ok(cleaned)
    }

    fn split_raw_page(raw: &str) -> (&str, &str) {
        let re = Regex::new(r"(?:^|[\r\n]+)---(?:$|[\r\n]+)").unwrap();
        let parts: Vec<&str> = re.splitn(&raw, 2).collect();
        match parts[..] {
            [yaml, body] => (yaml, body),
            [body] => ("", body),
            _ => unreachable!(),
        }
    }

    fn render_markdown(markdown: &str) -> String {
        let mut buffer = String::new();
        let parser = Parser::new_ext(&markdown, Options::all());
        html::push_html(&mut buffer, parser);

        buffer
    }
}
