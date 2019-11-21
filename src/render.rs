use pulldown_cmark::{Parser, Options, html};
use regex::Regex;
use serde::Serialize;
use serde_yaml::Error as YamlError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::result;

use crate::errors::*;

type Metadata = HashMap<String, String>;

#[derive(Debug, Serialize)]
pub struct Page {
    pub file: PathBuf,
    pub metadata: Metadata,
    pub body: String,
}

impl Page {
    pub fn read_from(path: &Path) -> Result<Page> {
        let raw = fs::read_to_string(path)
            .map_err(|source| MyError::ReadPageFile { source })?;

        let mut page = Page::from_string(&raw)?;
        page.file = PathBuf::from(path);
        Ok(page)
    }

    pub fn from_string(raw: &str) -> Result<Page> {
        let (header, body) = Page::split_raw_page(&raw);

        Ok(Page {
            file: PathBuf::from("-"), // i.e. STDIN
            metadata: Page::metadata_from_string(&header)?,
            body: Page::render_markdown(&body),
        })
    }

    pub fn metadata_as_string(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.metadata)
            .map_err(|source| MyError::MetadataRender { source })?;

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
        let re = Regex::new(r"(?:^|\s*[\r\n])---(?:$|[\r\n]\s*)").unwrap();
        let parts: Vec<&str> = re.splitn(&raw, 2).collect();
        match parts[..] {
            [yaml, body] => (yaml, body),
            [body] => ("", body),
            _ => unreachable!(),
        }
    }

    fn metadata_from_string(raw: &str) -> result::Result<Metadata, YamlError> {
        if raw.trim().len() == 0 {
            Ok(HashMap::new())
        } else {
            serde_yaml::from_str(&raw)
        }
    }

    fn render_markdown(markdown: &str) -> String {
        let mut buffer = String::new();
        let parser = Parser::new_ext(&markdown, Options::all());
        html::push_html(&mut buffer, parser);

        buffer
    }
}
