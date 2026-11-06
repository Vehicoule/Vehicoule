//! Capability calls and their failures (docs/plugin-host-spec.md, sections 3, 6 and 7).
//!
//! Each call gets a fresh store and a fresh instance with its own budgets, and both are dropped when it returns, so
//! nothing a plugin keeps in its memory survives to the next call. The host reports the outcome and never retries.
//! Calls have no network yet: every request is refused, as during install, until the HTTP sandbox and its budgets
//! come (section 11, step 3).

use std::time::Duration;

use thiserror::Error;

use crate::bindings::base::vehicoule::plugin::types;
use crate::bindings::search::SearchPluginPre;
use crate::media::TextSize;
use crate::{ByteSize, Capability, InstalledPlugin, ItemKind, Page, PluginHost};

/// An error the plugin itself returned, as the contract names them.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PluginError {
    /// The provider has no such item.
    #[error("not found")]
    NotFound,
    /// The provider cannot do this.
    #[error("unsupported")]
    Unsupported,
    /// The provider refused for now.
    #[error("rate limited")]
    RateLimited {
        /// How long the provider asked to wait, when it said; the core decides what to do with it.
        retry_after: Option<Duration>,
    },
    /// The provider refused the input.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The provider failed, or answered something the plugin cannot read.
    #[error("upstream failure: {0}")]
    Upstream(String),
}

/// Why a call returned no value. The host reports it; the core decides what to do.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PluginFailure {
    /// The plugin returned an error of the contract.
    #[error("the plugin answered: {0}")]
    Plugin(PluginError),
    /// The plugin does not provide this capability.
    #[error("the plugin does not provide {0:?}")]
    NotProvided(Capability),
    /// The call ran out of fuel.
    #[error("the plugin ran out of fuel")]
    FuelExhausted,
    /// The plugin asked for more memory than the ceiling.
    #[error("the plugin went over its memory ceiling")]
    MemoryLimit,
    /// The plugin trapped (crashed).
    #[error("the plugin trapped: {0}")]
    Trap(String),
    /// The value returned holds more text than the output budget.
    #[error("the plugin returned {} bytes of text, over the ceiling of {} bytes", size.get(), limit.get())]
    OutputTooLarge {
        /// The text the value held.
        size: ByteSize,
        /// The ceiling.
        limit: ByteSize,
    },
    /// Wasmtime failed for a reason that is none of the above.
    #[error("the Wasm runtime failed: {0}")]
    Runtime(String),
}

impl PluginHost {
    /// One page of results for `query`, of the given kinds, from `cursor` (a `next` of an earlier page) or from the
    /// start.
    ///
    /// # Errors
    ///
    /// A [`PluginFailure`]: the plugin's own error, a budget exceeded, a trap, or a plugin that does not provide
    /// search.
    pub fn search(
        &self,
        plugin: &InstalledPlugin,
        query: &str,
        kinds: &[ItemKind],
        cursor: Option<&str>,
    ) -> Result<Page, PluginFailure> {
        let kinds: Vec<types::ItemKind> = kinds.iter().copied().map(Into::into).collect();
        let answer = self.call(plugin, Capability::Search, |store, pre| {
            let instance = SearchPluginPre::new(pre.clone())?.instantiate(&mut *store)?;
            instance
                .vehicoule_plugin_search()
                .call_search(store, query, &kinds, cursor)
        })?;
        self.bounded(answer.map(Page::from))
    }

    /// Completions of what the user is typing, in the plugin's order.
    ///
    /// # Errors
    ///
    /// A [`PluginFailure`], as for [`PluginHost::search`].
    pub fn suggest(
        &self,
        plugin: &InstalledPlugin,
        prefix: &str,
    ) -> Result<Vec<String>, PluginFailure> {
        let answer = self.call(plugin, Capability::Search, |store, pre| {
            let instance = SearchPluginPre::new(pre.clone())?.instantiate(&mut *store)?;
            instance
                .vehicoule_plugin_search()
                .call_suggest(store, prefix)
        })?;
        self.bounded(answer)
    }

    /// The plugin's answer, or the failure it names, once its text is checked against the output budget. A plugin
    /// error's text counts too: it is output like any other.
    fn bounded<T: TextSize>(&self, answer: Result<T, types::Error>) -> Result<T, PluginFailure> {
        let answer = answer.map_err(PluginError::from);
        let size = match &answer {
            Ok(value) => value.text_size(),
            Err(error) => error.text_size(),
        };
        let size = ByteSize::new(u64::try_from(size).unwrap_or(u64::MAX));
        let limit = self.config().max_output;
        if size > limit {
            return Err(PluginFailure::OutputTooLarge { size, limit });
        }
        answer.map_err(PluginFailure::Plugin)
    }
}

impl TextSize for PluginError {
    fn text_size(&self) -> usize {
        match self {
            Self::InvalidInput(text) | Self::Upstream(text) => text.len(),
            Self::NotFound | Self::Unsupported | Self::RateLimited { .. } => 0,
        }
    }
}

impl From<types::Error> for PluginError {
    fn from(error: types::Error) -> Self {
        match error {
            types::Error::NotFound => Self::NotFound,
            types::Error::Unsupported => Self::Unsupported,
            types::Error::RateLimited(ms) => Self::RateLimited {
                retry_after: ms.map(Duration::from_millis),
            },
            types::Error::InvalidInput(text) => Self::InvalidInput(text),
            types::Error::Upstream(text) => Self::Upstream(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_error_maps_to_its_own_error() {
        assert_eq!(
            PluginError::from(types::Error::NotFound),
            PluginError::NotFound
        );
        assert_eq!(
            PluginError::from(types::Error::Unsupported),
            PluginError::Unsupported
        );
        assert_eq!(
            PluginError::from(types::Error::RateLimited(Some(1_500))),
            PluginError::RateLimited {
                retry_after: Some(Duration::from_millis(1_500))
            }
        );
        assert_eq!(
            PluginError::from(types::Error::RateLimited(None)),
            PluginError::RateLimited { retry_after: None }
        );
        assert_eq!(
            PluginError::from(types::Error::InvalidInput("q".to_owned())),
            PluginError::InvalidInput("q".to_owned())
        );
        assert_eq!(
            PluginError::from(types::Error::Upstream("down".to_owned())),
            PluginError::Upstream("down".to_owned())
        );
    }

    #[test]
    fn a_plugin_error_counts_its_text() {
        assert_eq!(PluginError::InvalidInput("abc".to_owned()).text_size(), 3);
        assert_eq!(PluginError::Upstream("abcd".to_owned()).text_size(), 4);
        assert_eq!(PluginError::NotFound.text_size(), 0);
        assert_eq!(PluginError::Unsupported.text_size(), 0);
        assert_eq!(
            PluginError::RateLimited { retry_after: None }.text_size(),
            0
        );
    }
}
