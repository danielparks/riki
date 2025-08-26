//! Handle rendering a page.

use crate::elements::{
    self, ElementError, handle_a_email, handle_last_modified,
};
use crate::errors::{Error, Result};
use actix_web::HttpRequest;
use dom_query::Document;
use handlebars::Handlebars;
use jiff::Timestamp;
use pulldown_cmark::{Options, Parser};
use regex::Regex;
use serde::Serialize;
use serde_yaml::Error as YamlError;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::result;
use std::sync::LazyLock;

/// Page metadata.
///
/// This is the YAML data in the page header, for example:
///
/// ```text
/// class: blog-post
///
/// ---
///
/// # How to use riki to host your web site
///
/// ...
/// ```
///
/// If not present, the `title` will be set to the contents of the first `<h1>`
/// on the page.
pub type Metadata = HashMap<String, String>;

/// The source of a [`Page`].
///
/// This will be available in the template as `{{source}}`. To access variant
/// fields, use <code>source.<i>variant</i>.<i>field</i></code>.
///
/// For example:
///
/// ```hbs
/// {{#if source.File.modified}}
///     <p>Last updated {{ source.File.modified }}</p>
/// {{/if}}
/// ```
#[derive(Clone, Debug, Default, Serialize)]
pub enum Source {
    /// From memory.
    #[default]
    Memory,

    /// From stdin.
    Stdin,

    /// From a file.
    File {
        /// Path to the file.
        ///
        /// Access in templates with `{{source.File.path}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        path: PathBuf,

        /// Time the file was last modified.
        ///
        /// Access in templates with `{{source.File.modified}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        modified: Option<Timestamp>,

        /// Time the file was created.
        ///
        /// Access in templates with `{{source.File.created}}`. Note that you
        /// probably want to wrap it with `{{#if source.File}}...{{/if}}` to
        /// prevent errors rendering pages from other sources.
        created: Option<Timestamp>,
    },
}

impl Source {
    /// Create a [`Source::File`] from a `Path`-like object.
    pub fn from_path<P: Into<PathBuf>>(path: P) -> Self {
        let path = path.into();
        let metadata = path.metadata().ok();
        Self::File {
            path,
            modified: metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
            created: metadata
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(|t| Timestamp::try_from(t).ok()),
        }
    }

    /// Get the last modified time, if available.
    #[must_use]
    pub const fn modified(&self) -> Option<Timestamp> {
        match self {
            Self::File { modified, .. } => *modified,
            _ => None,
        }
    }

    /// Get the creation time, if available.
    #[must_use]
    pub const fn created(&self) -> Option<Timestamp> {
        match self {
            Self::File { created, .. } => *created,
            _ => None,
        }
    }
}

/// A rendered page.
///
/// This is passed to the template, so the fields become available as variables
/// in the template.
#[derive(Debug, Serialize)]
pub struct Page {
    /// Source of the page.
    ///
    /// See [`Source`] for information on how to access its fields.
    pub source: Source,

    /// Metadata from the page header.
    ///
    /// For example, you might have a page file like:
    ///
    /// ```text
    /// title: My Page
    /// ---
    /// # Some content
    /// ```
    ///
    /// The title would be available to the template as `{{metadata.title}}`.
    pub metadata: Metadata,

    /// The name of the template to render with.
    // FIXME: it would be nice to have template metadata like modification and
    // creation time, but handlebars doesn’t support that.
    pub template: String,

    /// The HTML body of the page.
    ///
    /// This shouldn’t be HTML-escaped, so use the raw syntax: `{{{body}}}`.
    pub body: String,
}

impl Page {
    /// Load a `Page` from `path`.
    ///
    /// # Errors
    ///
    ///   * [`Error::ReadPageFile`] for problems reading `path`.
    ///   * [`Error::ParsePageMetadata`] for errors parsing metadata.
    pub fn read_from<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path: PathBuf = path.into();
        let raw = fs::read_to_string(&path).map_err(|error| {
            Error::ReadPageFile { source: error, path: path.clone() }
        })?;

        Self::from_source(Source::from_path(path), &raw)
    }

    /// Load a `Page` from a string.
    ///
    /// # Errors
    ///
    ///   * [`Error::ParsePageMetadata`] for errors parsing metadata.
    pub fn from_memory<S: AsRef<str>>(raw: S) -> Result<Self> {
        Self::from_source(Source::Memory, raw)
    }

    /// Load a `Page` from a string with a source.
    ///
    /// # Errors
    ///
    ///   * [`Error::ParsePageMetadata`] for errors parsing metadata.
    pub fn from_source<R: AsRef<str>>(source: Source, raw: R) -> Result<Self> {
        let (header, body) = split_raw_page(raw.as_ref());

        let mut metadata = metadata_from_string(header)?;
        let body = render_markdown(body);

        if !metadata.contains_key("title") {
            let fragment = Document::fragment(body.clone());
            let h1 = fragment.select_single("h1");
            if h1.length() > 0 {
                metadata.insert("title".into(), h1.text().into());
            }
        }

        let template = metadata
            .get("template")
            .cloned()
            .unwrap_or_else(|| "default".to_owned());

        Ok(Self { source, metadata, template, body })
    }

    /// Serialize the page metadata to a string.
    ///
    /// # Errors
    ///
    /// This will return [`Error::MetadataRender`] if there is a problem
    /// serializing the metadata as YAML.
    pub fn metadata_as_string(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.metadata)
            .map_err(Error::MetadataRender)?;

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
        tpls: &Handlebars,
        req: Option<&HttpRequest>,
    ) -> Result<String> {
        let document =
            Document::from(tpls.render(&self.template, &self).map_err(
                |error| Error::TemplateRender {
                    source: error,
                    page_source: Box::new(self.source.clone()),
                },
            )?);

        let ctx = elements::Context {
            document: &document,
            page: self,
            req,
            show_detailed_errors: true,
        };
        for node in document.select("a-email").nodes() {
            if let Err(ElementError(msg)) = handle_a_email(&ctx, node) {
                tracing::error!("Handling <a-email>: {msg}");
                let b = document.tree.new_element("b");
                b.set_text(msg);
                node.replace_with(&b);
            }
        }
        for node in document.select("last-modified").nodes() {
            if let Err(ElementError(msg)) = handle_last_modified(&ctx, node) {
                tracing::error!("Handling <last-modified>: {msg}");
                let b = document.tree.new_element("b");
                b.set_text(msg);
                node.replace_with(&b);
            }
        }

        Ok(document.html().to_string())
    }
}

/// Split the raw page string into metadata and body.
fn split_raw_page(raw: &str) -> (&str, &str) {
    static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:^|\s*[\r\n])---(?:$|[\r\n]\s*)").unwrap()
    });
    let mut iter = SPLIT_RE.splitn(raw, 2);
    match (iter.next(), iter.next()) {
        (Some(yaml), Some(body)) => (yaml, body),
        (Some(body), None) => ("", body),
        (None, _) => unreachable!(),
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
    pulldown_cmark::html::push_html(&mut buffer, parser);

    buffer
}
