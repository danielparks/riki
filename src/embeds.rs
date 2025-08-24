//! Embedded assets.
#![expect(clippy::same_name_method)] // triggered by RustEmbed

use rust_embed::RustEmbed;

/// The embedded templates.
///
/// This is a private module, so it’s only public within the crate.
#[derive(RustEmbed)]
#[folder = "embed/templates"]
#[include = "*.hbs"]
pub struct Templates;
