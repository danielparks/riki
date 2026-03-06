//! # Custom elements
//!
//! ## `<a-email>`: add a hidden suffix to an email in a `mailto:` link.
//!
//! ```html
//! <p>A link to my <a-email href="mailto:email@example.com"></a-email></p>
//! ```
//!
//! Produces:
//!
//! ```html
//! <p>A link to my <a href="mailto:email-aMCXZ@example.com">email<span class="hidden">-aMCXZ</a>@example.com</a></p>
//! ```
//!
//! (“aMCXZ” is a URL-safe, unpadded base64 encoding of the current time.)
//!
//! ## `<last-modified>`: show the last modified time for the page.
//!
//! ```html
//! <p><last-modified tz="America/Los_Angeles" format="%Y-%m-%d"></last-modified></p>
//! ```
//!
//! Produces:
//!
//! ```html
//! <p><span>2025-09-09</span></p>
//! ```
//!
//! The `tz` attribute is optional, and defaults to “system” which is the local
//! timezone for the server.
//!
//! The `format` attribute is optional. See [`jiff::fmt::strtime`] for detailed
//! documentation (look for the [Conversion specifications] section).
//!
//! [Conversion specifications]: jiff::fmt::strtime#conversion-specifications

use crate::{ContentReturn, VariableMap};
use dom_query::{Document, NodeRef};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use std::result;
use thiserror::Error;
use url::Url;

/// An error from a custom element handler.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ElementError(pub String);

impl From<&str> for ElementError {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ElementError {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Context information for an element handler.
pub struct Context<'a, 'vars, V: VariableMap> {
    /// The document or fragment being processed.
    pub document: &'a Document,

    /// Page information.
    pub page: &'a ContentReturn,

    /// Variables generated from HTTP request.
    pub variables: &'vars V,

    /// Whether or not to show detailed error messages to the user.
    pub show_detailed_errors: bool,
}

/// Result of a custom element handler.
pub type Result<T, E = ElementError> = result::Result<T, E>;

/// Handle an `a-email` element
///
/// # Errors
///
/// Returns [`ElementError`] if there is a problem.
pub fn handle_a_email<V: VariableMap>(
    _ctx: &Context<'_, '_, V>,
    node: &NodeRef,
) -> Result<()> {
    let mut url: Url = node
        .attr("href")
        .ok_or("No href attribute on <a-email>")?
        .parse()
        .map_err(|_| "Invalid URL in href attribute on <a-email>")?;

    let mut iter = url.path().split('@');
    let (Some(user), Some(domain), None) =
        (iter.next(), iter.next(), iter.next())
    else {
        return Err("Invalid email address in <a-email>".into());
    };

    // Encode the current time (seconds since epoch) in URL-safe base64.
    let hidden = format!("-{}", {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let timestamp = Timestamp::now().as_second().to_be_bytes();
        let i = timestamp.iter().position(|c| *c != 0).unwrap_or(0);
        URL_SAFE_NO_PAD.encode(&timestamp[i..])
    });
    let new_email = format!("{user}{hidden}@{domain}");

    node.rename("a");
    if node.is_empty_element() {
        let span = node.tree.new_element("span");
        span.set_attr("class", "hidden");
        span.set_text(hidden);

        node.append_child(&node.tree.new_text(user));
        node.append_child(&span);
        node.append_child(&node.tree.new_text("@"));
        node.append_child(&node.tree.new_text(domain));
    }

    // This has to be done after we’re done with `user` and `domain`, since they
    // are borrowed from `url`.
    url.set_path(&new_email);
    node.set_attr("href", url.as_str());

    Ok(())
}

/// Handle an `a-email` element when outputting the source.
///
/// This just redacts the `href`, if present.
pub fn handle_a_email_source(node: &NodeRef) {
    let Some(href) = node.attr("href") else {
        // Nothing to redact.
        return;
    };

    match href.parse::<Url>() {
        Ok(mut url) => {
            url.set_path("***@*****");
            node.set_attr("href", url.as_str());
        }
        Err(_) => {
            node.set_attr("href", "*****");
        }
    }
}

/// Handle a `last-modified` element
///
/// # Errors
///
/// Returns [`ElementError`] if there is a problem.
pub fn handle_last_modified<V: VariableMap>(
    ctx: &Context<'_, '_, V>,
    node: &NodeRef,
) -> Result<()> {
    let Some(time) = ctx.page.source.modified() else {
        // FIXME leak? The docs say “Removes the selected node from its parent
        // node, but keeps it in the tree”.
        node.remove_from_parent();
        return Ok(());
    };

    let zoned = time.to_zoned(match node.attr_or("tz", "system").as_ref() {
        "system" => TimeZone::system(),
        tz => TimeZone::get(tz)
            .map_err(|error| format!("last-modified element: {error}"))?,
    });

    let output = if let Some(format) = node.attr("format") {
        zoned.strftime(format.as_ref()).to_string()
    } else {
        zoned.to_string()
    };

    node.remove_attrs(&["tz", "format"]);
    node.rename("span");
    node.remove_children();
    node.append_child(&node.tree.new_text(output));

    Ok(())
}
