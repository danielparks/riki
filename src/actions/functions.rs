//! # Functions
//!
//! These will eventually become actions, which will be callable from a
//! configuration file:
//!
//!   * [`render()`]
//!   * [`markdown_to_html()`]
//!   * [`redact_source()`]

use super::{ContentReturn, Context, MediaType, Result, Return, VariableMap};
use crate::render::elements::{
    self, ElementError, handle_a_email, handle_last_modified,
};
use crate::render::{self, render_source_to_string};
use dom_query::Document;
use std::mem;
use tracing;

/// Render passed content in a template.
///
/// # Errors
///
/// Will return [`super::Error`] if there is a problem getting content from
/// `ret` or rendering the template.
pub fn render<'a, V: VariableMap<'a>, R: Return>(
    context: &'a Context<'a, V>,
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
        Document::from(context.tpls.render(template, &ret).map_err(
            |error| crate::Error::TemplateRender {
                source: error,
                page_source: Box::new(ret.source.clone()),
            },
        )?);

    let ctx = elements::Context {
        document: &document,
        page: &ret,
        variables: &context.variables,
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
pub fn markdown_to_html<'a, V: VariableMap<'a>, R: Return>(
    _context: &'a Context<'a, V>,
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
pub fn redact_source<'a, V: VariableMap<'a>, R: Return>(
    _context: &'a Context<'a, V>,
    ret: R,
) -> Result<Option<ContentReturn>> {
    // FIXME: caching headers based on template and Page.
    // FIXME: add cache-busting to href, src, etc. in HTML.
    let mut ret = ret.into_content_return()?;
    ret.body = render_source_to_string(ret.body.into_string()?).into();
    ret.content_type = MediaType::TEXT_MARKDOWN_UTF8;
    Ok(Some(ret))
}
