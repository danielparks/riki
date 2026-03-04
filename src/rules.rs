//! # Code to represent rules about how data is processed and returned.
//!
//! The default rules ([`default_rules()`] and
//! [`default_rules_for_settings()`]) check for:
//!
//!   1. An `*.md` source that should be redacted and returned.
//!   2. A static file to return as-is.
//!   3. A `${path}.md` file to render and return.
//!
//! ## Canonical URLs and redirects
//!
//! Riki redirects to the canonical URL of a page when possible.
//!
//! The canonical URL will end with a / if (and only if) it corresponds to a
//! `index.html`-like page or static file.
//!
//! | Source path       | Canonical path   |
//! |-------------------|------------------|
//! | `page.md`         | `/page`          |
//! | `dir/index.md`    | `/dir/`          |
//! | `static.html`     | `/static.html`   |
//! | `dir/index.html`  | `/dir/`          |

use crate::config::SourcedConfiguration;
use crate::config::actions::Action;
use crate::config::errors::{Diagnostics, ParseResult};
use crate::config::model::{
    ConfigRule, ConfigSettings, Configuration, ConfigurationBuilder,
    ParsedString,
};
use crate::config::parser2::GeneratedSource;

/// Source for default rules.
pub const SOURCE: GeneratedSource = GeneratedSource("default rules");

/// Get the default rules for Riki.
///
/// # Errors
///
/// Returns [`Diagnostics`] for problems creating a glob, parsing a string, etc.
pub fn default_rules(
    root_path: String,
    templates_path: String,
) -> Result<
    SourcedConfiguration<GeneratedSource<'static>>,
    Diagnostics<GeneratedSource<'static>>,
> {
    default_rules_for_settings(ConfigSettings {
        root: root_path.into(),
        templates: templates_path.into(),
    })
}

/// Get the default rules for Riki.
///
/// # Errors
///
/// Returns [`Diagnostics`] for problems creating a glob, parsing a string, etc.
pub fn default_rules_for_settings(
    settings: ConfigSettings<'static>,
) -> Result<
    SourcedConfiguration<GeneratedSource<'static>>,
    Diagnostics<GeneratedSource<'static>>,
> {
    inner_default_rules_for_settings(settings)
        .map(|configuration| {
            SourcedConfiguration::generated(SOURCE, configuration)
        })
        .map_err(|errors| Diagnostics::from_errors(errors, SOURCE))
}

/// Get the default rules for Riki (internal [`ParseResult`] version).
///
/// # Errors
///
/// Returns an error if there is a problem creating a glob.
#[expect(
    clippy::needless_pass_by_value,
    reason = "&ConfigSettings would have to outlive function"
)]
#[expect(clippy::allow_attributes, reason = "rust-clippy issue #13358")]
fn inner_default_rules_for_settings(
    settings: ConfigSettings<'_>,
) -> ParseResult<'_, Configuration<'_>> {
    #[allow(clippy::wildcard_imports, reason = "convenience")]
    use crate::config::actions::functions::*;

    let mut config = ConfigurationBuilder::new();
    // root = ...
    // templates = ...
    // *.md redact_source(canonical($clean_path))
    config.add(rule(
        "**/*.md",
        &settings,
        redact_source(canonical(parsed("$clean_path")?)),
    ))?;
    // index.html canonical(as_dir(dirname($clean_path)))")
    config.add(rule(
        "**/index.html",
        &settings,
        canonical(as_dir(dirname(parsed("$clean_path")?))),
    ))?;

    // Returns $clean_path as a file if it matches:
    //     canonical(if_file($clean_path))
    config.add(rule(
        "**",
        &settings,
        canonical(if_file(parsed("$clean_path")?)),
    ))?;

    // condition(
    //     canonical(condition(
    //         if_file("${clean_path}/index.html"),
    //         as_dir($clean_path),
    //     )),
    //     "${clean_path}/index.html",
    // )
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}/index.html")?),
                as_dir(parsed("$clean_path")?),
            )),
            parsed("${clean_path}/index.html")?,
        ),
    ))?;

    // index canonical(as_dir(dirname($clean_path)))
    config.add(rule(
        "**/index",
        &settings,
        canonical(as_dir(dirname(parsed("$clean_path")?))),
    ))?;

    // condition(
    //     canonical(condition(
    //         if_file("${clean_path}.md"),
    //         $clean_path,
    //     )),
    //     render(markdown(if_file("${clean_path}.md"))),
    // ),
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}.md")?),
                parsed("$clean_path")?,
            )),
            render(markdown(if_file(parsed("${clean_path}.md")?))),
        ),
    ))?;

    // condition(
    //     canonical(condition(
    //         if_file("${clean_path}/index.md"),
    //         as_dir($clean_path),
    //     )),
    //     render(markdown(if_file("${clean_path}/index.md"))),
    // ),
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}/index.md")?),
                as_dir(parsed("$clean_path")?),
            )),
            render(markdown(if_file(parsed("${clean_path}/index.md")?))),
        ),
    ))?;

    config.build()
}

/// Create a [`ConfigRule`]
fn rule<'src, A: Into<Action<'src>>>(
    glob: &'src str,
    settings: &ConfigSettings<'src>,
    action: A,
) -> ConfigRule<'src> {
    ConfigRule::new(glob, settings.clone(), action.into())
}

/// Create a [`ParsedString`] from string content with variable interpolation.
///
/// # Panics
///
/// Panics if the string cannot be parsed.
fn parsed(s: &str) -> ParseResult<'_, ParsedString<'_>> {
    use crate::config::parser2::StringType;
    ParsedString::from_string_content(s, StringType::QuotedDouble)
}
