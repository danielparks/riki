//! Handle configuration files

pub mod actions;
pub mod errors;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod parser2;
mod tests;

use bstr::BStr;
use lexer::{Diagnostic, tokenize};
use parser2::ContentSource;

/// Wrapper for [`SourcedConfiguration`] definition.
///
/// Module allows `#[expect]` to work on [`SourcedConfiguration`].
mod internal {
    #![expect(clippy::future_not_send, reason = "self_referencing")]
    #![expect(clippy::mem_forget, reason = "self_referencing")]

    use super::errors::Diagnostics;
    use super::model::{ConfigRule, Configuration};
    use super::parser2::{ContentSource, GeneratedSource, Source};
    use crate::render::TemplatesManager;
    use ouroboros::self_referencing;
    use std::fmt;

    /// A [`Configuration`] with its [`ContentSource`].
    #[self_referencing]
    pub struct SourcedConfiguration<S: Source + Sync + 'static> {
        /// The source of the configuration.
        source: S,

        /// The parsed configuration.
        #[borrows(source)]
        #[covariant]
        configuration: Configuration<'this>,
    }

    impl<S: ContentSource + Sync + 'static> SourcedConfiguration<S> {
        /// Parse a [`ContentSource`].
        ///
        /// # Errors
        ///
        /// Returns [`Diagnostics`] on parse errors.
        pub async fn parse_from(source: S) -> Result<Self, Diagnostics<S>> {
            SourcedConfigurationAsyncSendTryBuilder {
                source,
                configuration_builder: |source| {
                    Box::pin(async move { Configuration::parse(source) })
                },
            }
            .try_build_or_recover()
            .await
            .map_err(|(diagnostics, heads)| {
                Diagnostics::from_diagnostics(diagnostics, heads.source)
            })
        }
    }

    impl<'a> SourcedConfiguration<GeneratedSource<'a>> {
        /// Create a [`SourcedConfiguration`] from rules generated in code.
        pub fn generated<N: Into<GeneratedSource<'a>>>(
            source: N,
            configuration: Configuration<'static>,
        ) -> Self {
            SourcedConfigurationBuilder {
                source: source.into(),
                configuration_builder: |_| configuration,
            }
            .build()
        }
    }

    impl<S: Source + Sync + 'static> SourcedConfiguration<S> {
        /// Get the source.
        #[inline]
        #[must_use]
        pub fn source(&self) -> &S {
            self.borrow_source()
        }

        /// Get the configuration.
        #[inline]
        #[must_use]
        pub fn configuration(&self) -> &Configuration<'_> {
            self.borrow_configuration()
        }

        /// Get all the rules.
        #[inline]
        #[must_use]
        pub fn rules(&self) -> &[ConfigRule<'_>] {
            self.configuration().rules()
        }

        /// Get matching rules for a path.
        #[inline]
        #[must_use]
        pub fn matches(&self, path: &str) -> Vec<&ConfigRule<'_>> {
            self.configuration().matches(path)
        }

        /// Get matching rules for a path.
        #[inline]
        #[must_use]
        pub fn last_matching(&self, path: &str) -> Option<&ConfigRule<'_>> {
            self.configuration().last_matching(path)
        }

        /// Evaluate a request through the configuration rules.
        ///
        /// # Errors
        ///
        /// This tries to return errors as rendered responses, but if it fails
        /// it may return an error to be rendered by the fallback.
        pub fn evaluate(
            &self,
            manager: &TemplatesManager,
            request: &axum::extract::Request,
        ) -> crate::actions::Result<axum::response::Response> {
            self.configuration().evaluate(manager, request)
        }
    }

    impl<S: Source + Sync + fmt::Debug + 'static> fmt::Debug
        for SourcedConfiguration<S>
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("SourcedConfiguration")
                .field("source", &self.source())
                .field("configuration", &self.configuration())
                .finish()
        }
    }
}

pub use internal::SourcedConfiguration;

/// Dump the tokens from a configuration file to stdout.
///
/// For debugging and development.
///
/// # Errors
///
/// Returns <code>Vec<[Diagnostic]></code> for parse errors.
pub fn dump_config_tokens<S: ContentSource>(
    source: &S,
) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    for (token, span) in tokenize(source.content(), &mut diagnostics) {
        println!(
            "{token:?}({:?})",
            BStr::new(&source.content().as_bytes()[span])
        );
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}
