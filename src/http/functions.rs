//! Functions that will become actions.

use crate::actions::{self, ContentReturn, MediaType, Return};
use crate::render::elements::{
    self, ElementError, handle_a_email, handle_last_modified,
};
use crate::render::{self, render_source_to_string};
use actix_web::{self, HttpRequest};
use dom_query::Document;
use handlebars::Handlebars;
use std::mem;
use std::path::PathBuf;
use tracing;

/// Render passed content in a template.
///
/// # Errors
///
/// Will return [`actions::Error`] if there is a problem getting content from
/// `ret` or rendering the template.
pub fn render<R: Return>(
    context: &Context<'_>,
    req: Option<&HttpRequest>,
    ret: R,
) -> actions::Result<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;

    let template = ret
        .metadata
        .get("template")
        .map(String::as_str)
        .unwrap_or_else(|| "default");

    ret.body.ensure_string()?;
    let document =
        Document::from(context.tpls.render(template, &ret).map_err(
            |error| crate::Error::TemplateRender {
                source: error,
                page_source: Box::new(ret.source.clone()),
            },
        )?);

    let ctx = elements::Context {
        document: &document,
        page: &ret,
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

    ret.content_type = MediaType::TEXT_HTML_UTF8;
    ret.body = document.html().into();

    Ok(Some(ret))
}

/// Load metadata and convert body to HTML.
///
/// # Errors
///
/// Will return [`actions::Error`] if there is a problem getting content from
/// `ret` or parsing page metadata from the content.
pub fn markdown_to_html<R: Return>(
    _context: &Context<'_>,
    ret: R,
) -> actions::Result<Option<ContentReturn>> {
    let mut ret = ret.into_content_return()?;
    let raw_page = mem::take(&mut ret.body).into_string()?;
    let (header, body) = render::split_raw_page(&raw_page);

    ret.metadata.extend(
        render::metadata_from_string(header).map_err(crate::Error::from)?,
    );
    ret.body = render::render_markdown(body).into();
    ret.content_type = MediaType::TEXT_HTML_UTF8;
    ret.ensure_metadata_title()?;

    Ok(Some(ret))
}

/// Redact sensitive values from passed Markdown.
///
/// # Errors
///
/// Returns [`actions::Error`] for problems getting content from `ret`.
pub fn redact_source<R: Return>(
    _context: &Context<'_>,
    ret: R,
) -> actions::Result<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;
    ret.body = render_source_to_string(ret.body.into_string()?).into();
    ret.content_type = MediaType::TEXT_MARKDOWN_UTF8;
    Ok(Some(ret))
}

/// Context for actions
#[derive(Debug, Default)]
pub struct Context<'a> {
    /// Configuration
    pub config: Configuration,

    /// Templates for rendering pages
    pub tpls: Handlebars<'a>,
}

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Configuration {
    /// The path to the directory containing pages and static assets.
    pub root_path: PathBuf,
    /// The path to the directory containing templates.
    pub templates_path: PathBuf,
}

impl Default for Configuration {
    /// Create a configuration using the default subdirectories names in the
    /// current directory.
    fn default() -> Self {
        Self::default_in(".")
    }
}

impl Configuration {
    /// Create a configuration using the default subdirectories under `root`.
    pub fn default_in<P: Into<PathBuf>>(root: P) -> Self {
        let root: PathBuf = root.into();
        Self { templates_path: root.join("templates"), root_path: root }
    }
}
