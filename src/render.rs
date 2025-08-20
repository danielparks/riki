//! Handle rendering a page.

use dom_query::Document;
use pulldown_cmark::{Options, Parser, html};
use regex::Regex;
use serde::Serialize;
use serde_yaml::Error as YamlError;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::result;
use std::sync::LazyLock;

use crate::errors::{Error, Result};
use crate::templates::TemplateManager;

/// Page metadata
type Metadata = HashMap<String, String>;

/// A rendered page
#[derive(Debug, Serialize)]
pub struct Page {
    /// The path to the file on disk, or "-" for stdin (FIXME)
    pub file: PathBuf,

    /// Metadata about the page
    pub metadata: Metadata,

    /// The HTML body of the page
    pub body: String,
}

impl Page {
    /// Load a `Page` from `path`.
    ///
    /// # Errors
    ///
    /// This will return [`Error`] if there is a problem loading `path`.
    /// and
    pub fn read_from(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .map_err(|source| Error::ReadPageFile { source })?;

        let mut page = Self::from_string(&raw)?;
        page.file = PathBuf::from(path);
        Ok(page)
    }

    /// Load a `Page` from a string.
    ///
    /// # Errors
    ///
    /// This will return [`Error`] if there is a problem parsing `raw`.
    pub fn from_string(raw: &str) -> Result<Self> {
        let (header, body) = Self::split_raw_page(raw);

        let mut metadata = Self::metadata_from_string(header)?;
        let fragment = Document::fragment(Self::render_markdown(body));

        if !metadata.contains_key("title") {
            let h1 = fragment.select_single("h1");
            if h1.length() > 0 {
                metadata.insert("title".into(), h1.text().into());
            }
        }

        Ok(Self {
            file: PathBuf::from("-"), // i.e. STDIN
            metadata,
            body: fragment.html_root().inner_html().to_string(),
        })
    }

    /// Serialize the page metadata to a string.
    ///
    /// # Errors
    ///
    /// This will return [`Error::MetadataRender`] if there is a problem
    /// serializing the metadata as YAML.
    pub fn metadata_as_string(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.metadata)
            .map_err(|source| Error::MetadataRender { source })?;

        let prefix = "---\n";
        let start = if yaml.starts_with(prefix) {
            prefix.len()
        } else {
            0
        };

        let mut cleaned = String::new();
        if &yaml[start..] != "{}" {
            // Metadata wasn’t empty.
            cleaned.push_str(&yaml[start..]);
        }

        Ok(cleaned)
    }

    /// Render the page to a string.
    ///
    /// # Errors
    ///
    /// This will return [`Error`] if there is a problem loading the template or
    /// rendering the page.
    pub fn render_to_string(
        &self,
        tpls: &mut TemplateManager,
    ) -> Result<String> {
        if let Some(tpl_name) = self.metadata.get("template") {
            Ok(tpls.get(tpl_name)?.render_to_string(&self)?)
        } else {
            Ok(tpls.default()?.render_to_string(&self)?)
        }
    }

    /// Split the raw page string into metadata and body.
    fn split_raw_page(raw: &str) -> (&str, &str) {
        static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?:^|\s*[\r\n])---(?:$|[\r\n]\s*)").unwrap()
        });
        let parts: Vec<&str> = SPLIT_RE.splitn(raw, 2).collect();
        match parts[..] {
            [yaml, body] => (yaml, body),
            [body] => ("", body),
            _ => unreachable!(),
        }
    }

    /// Load metadata from string.
    ///
    /// # Errors
    ///
    /// Returns [`YamlError`] if the YAML is invalid.
    fn metadata_from_string(raw: &str) -> result::Result<Metadata, YamlError> {
        if raw.trim().is_empty() {
            Ok(HashMap::new())
        } else {
            serde_yaml::from_str(raw)
        }
    }

    /// Render `markdown` as HTML.
    fn render_markdown(markdown: &str) -> String {
        let mut buffer = String::new();
        let parser = Parser::new_ext(markdown, Options::all());
        html::push_html(&mut buffer, parser);

        buffer
    }
}
