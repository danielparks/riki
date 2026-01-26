//! Functions that will become actions.

use super::{ContentReturn, MediaType, Result, Return};
use crate::render::elements::{
    self, ElementError, handle_a_email, handle_last_modified,
};
use crate::render::{self, render_source_to_string};
use actix_web::{self, HttpRequest};
use dom_query::Document;
use handlebars::Handlebars;
use std::mem;
use std::path::{Path, PathBuf};
use tracing;

/// Render passed content in a template.
///
/// # Errors
///
/// Will return [`super::Error`] if there is a problem getting content from
/// `ret` or rendering the template.
pub fn render<C: Context, R: Return>(
    context: &C,
    req: Option<&HttpRequest>,
    ret: R,
) -> Result<Option<ContentReturn>> {
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
        Document::from(context.tpls().render(template, &ret).map_err(
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
/// Will return [`super::Error`] if there is a problem getting content from
/// `ret` or parsing page metadata from the content.
pub fn markdown_to_html<C: Context, R: Return>(
    _context: &C,
    ret: R,
) -> Result<Option<ContentReturn>> {
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
/// Returns [`super::Error`] for problems getting content from `ret`.
pub fn redact_source<C: Context, R: Return>(
    _context: &C,
    ret: R,
) -> Result<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;
    ret.body = render_source_to_string(ret.body.into_string()?).into();
    ret.content_type = MediaType::TEXT_MARKDOWN_UTF8;
    Ok(Some(ret))
}

/// Context for actions
pub trait Context {
    /// Get templates
    fn tpls(&self) -> &Handlebars<'_>;

    /// Get the path to the current working directory
    fn working_path(&self) -> &Path;
}

/// Static context with preset configuration.
#[derive(Debug, Default, Clone)]
pub struct StaticContext<'a> {
    /// Working directory
    pub working_path: PathBuf,

    /// Templates for rendering pages
    pub tpls: Handlebars<'a>,
}

impl Context for StaticContext<'_> {
    fn tpls(&self) -> &Handlebars<'_> {
        &self.tpls
    }

    fn working_path(&self) -> &Path {
        &self.working_path
    }
}
