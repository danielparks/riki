//! Code to represent rules about how data is processed and returned.

use crate::config::actions::Action;
use crate::config::errors::ParseResult;
use crate::config::model::{
    ConfigRule, ConfigSettings, Configuration, ConfigurationBuilder,
    ParsedString,
};

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
fn parsed(s: &str) -> ParsedString<'_> {
    use crate::config::parser2::StringType;
    ParsedString::from_string_content(s, StringType::QuotedDouble).unwrap()
}

/// Get the default rules for Riki.
///
/// # Errors
///
/// Returns an error if there is a problem creating a glob.
pub fn default_rules<'a>(
    root_path: &'a str,
    templates_path: &'a str,
) -> ParseResult<'a, Configuration<'a>> {
    #![expect(clippy::allow_attributes, reason = "rust-clippy issue #13358")]

    #[allow(clippy::wildcard_imports, reason = "convenience")]
    use crate::config::actions::functions::*;

    let settings = ConfigSettings {
        root: parsed(root_path),
        templates: parsed(templates_path),
    };

    let mut config = ConfigurationBuilder::new();
    // *.md redact_source(canonical($clean_path))
    config.add(rule(
        "**/*.md",
        &settings,
        redact_source(canonical(parsed("$clean_path"))),
    ))?;
    // index.html canonical("${dirname($clean_path)}/")
    config.add(rule(
        "**/index.html",
        &settings,
        canonical(as_dir(dirname(parsed("$clean_path")))),
    ))?;

    // if file_exists("$clean_path") {
    //     canonical($clean_path) // returns $clean_path as a file if it
    // matches. }
    config.add(rule(
        "**",
        &settings,
        canonical(if_file(parsed("$clean_path"))),
    ))?;

    // if file_exists("$clean_path/index.html") {
    //     if canonical("${clean_path}/") {
    //         $clean_path/index.html
    //     }
    // }
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}/index.html")),
                as_dir(parsed("$clean_path")),
            )),
            parsed("${clean_path}/index.html"),
        ),
    ))?;

    // index canonical("${dirname($clean_path)}/")
    config.add(rule(
        "**/index",
        &settings,
        canonical(as_dir(dirname(parsed("$clean_path")))),
    ))?;

    // if file_exists("${clean_path}.md") {
    //     if canonical($clean_path) {
    //         render(markdown("${clean_path}.md"))
    //     }
    // }
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}.md")),
                parsed("$clean_path"),
            )),
            render(markdown(if_file(parsed("${clean_path}.md")))),
        ),
    ))?;

    // if file_exists("$clean_path/index.md") {
    //     if canonical("${clean_path}/") {
    //         render(markdown("$clean_path/index.md"))
    //     }
    // }
    config.add(rule(
        "**",
        &settings,
        condition(
            canonical(condition(
                if_file(parsed("${clean_path}/index.md")),
                as_dir(parsed("$clean_path")),
            )),
            render(markdown(if_file(parsed("${clean_path}/index.md")))),
        ),
    ))?;

    config.build()
}
