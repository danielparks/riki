use pulldown_cmark::{Parser, Options, html};
use regex::Regex;
use serde::Serialize;
use serde_yaml::Error as YamlError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::result;

use crate::errors::Error;
use crate::errors::Result;
use crate::templates::TemplateManager;

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
            .map_err(|source| Error::ReadPageFile { source })?;

        let mut page = Page::from_string(&raw)?;
        page.file = PathBuf::from(path);
        Ok(page)
    }

    pub fn from_string(raw: &str) -> Result<Page> {
        let (header, body) = Page::split_raw_page(&raw);

        let mut metadata = Page::metadata_from_string(&header)?;
        let body = Page::render_markdown(&body);

        if ! metadata.contains_key("title") {
            // Parsing HTML with a regular expression? What could go wrong.
            lazy_static! {
                static ref H1: Regex = Regex::new("<h1(?:\\s[^>]*)?>.*?</h1>").unwrap();
                static ref TAGS: Regex = Regex::new("</?\\w.*?>").unwrap();
            }

            if let Some(mat) = H1.find(&body) {
                let title = mat.as_str();
                metadata.insert("title".into(), TAGS.replace_all(title, "").into());
            }
        }

        Ok(Page {
            file: PathBuf::from("-"), // i.e. STDIN
            metadata: metadata,
            body: body,
        })
    }

    pub fn from_error<E>(error: &E) -> Page
        where E: std::fmt::Debug
    {
        let mut meta = HashMap::new();
        meta.insert("title".to_string(), "ERROR".to_string());

        Page {
            file: PathBuf::from(""),
            metadata: meta,
            body: format!("error {:?}", error),
        }
    }

    pub fn metadata_as_string(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.metadata)
            .map_err(|source| Error::MetadataRender { source })?;

        let prefix = "---\n";
        let start = if yaml.starts_with(prefix) { prefix.len() } else { 0 };

        let mut cleaned = String::new();
        if &yaml[start..] != "{}" {
            // Metadata wasn’t empty.
            cleaned.push_str(&yaml[start..])
        }

        Ok(cleaned)
    }

    pub fn render_to_string(&self, tpls: &mut TemplateManager) -> Result<String> {
        if let Some(tpl_name) = self.metadata.get("template") {
            Ok(tpls.get(tpl_name)?.render_to_string(&self)?)
        } else {
            Ok(tpls.default()?.render_to_string(&self)?)
        }
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
