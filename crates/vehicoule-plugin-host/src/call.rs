//! Capability calls, their failures and their reports (docs/plugin-host-spec.md, sections 3, 6, 7 and 9).
//!
//! Each call gets a fresh store and a fresh instance with its own budgets, and both are dropped when it returns, so
//! nothing a plugin keeps in its memory survives to the next call. The host reports the outcome and what the call
//! used, and never retries. Requests go through the HTTP sandbox of `host.rs` (section 5).

use std::time::Duration;

use thiserror::Error;

use crate::bindings::base::vehicoule::plugin::types;
use crate::bindings::search::SearchPluginPre;
use crate::media::{OutputSize, output_bytes};
use crate::{ByteSize, Capability, Fuel, InstalledPlugin, ItemKind, LogEntry, Page, PluginHost};

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
    /// The call ran past its wall time.
    #[error("the plugin ran past its wall time")]
    WallTime,
    /// The plugin asked for more memory than the ceiling.
    #[error("the plugin went over its memory ceiling")]
    MemoryLimit,
    /// The plugin trapped (crashed).
    #[error("the plugin trapped: {0}")]
    Trap(String),
    /// The call tried to send more requests than its HTTP budget allows.
    #[error("the plugin sent more requests than its budget")]
    TooManyRequests,
    /// A response body was over the HTTP budget.
    #[error("a response body was over its budget")]
    ResponseTooLarge,
    /// A request took longer than its deadline.
    #[error("a request timed out")]
    RequestTimeout,
    /// The value returned is over the output budget (strings and list elements, see [`HostConfig::max_output`]).
    ///
    /// [`HostConfig::max_output`]: crate::HostConfig::max_output
    #[error("the plugin returned a value of size {}, over the ceiling of {}", size.get(), limit.get())]
    OutputTooLarge {
        /// The value's size.
        size: ByteSize,
        /// The ceiling.
        limit: ByteSize,
    },
    /// Wasmtime failed for a reason that is none of the above.
    #[error("the Wasm runtime failed: {0}")]
    Runtime(String),
}

/// What one call used, whether it returned a value or failed (section 7), for the budgets' measurements
/// (section 9).
///
/// A call that never ran, because the plugin does not provide the capability, reports nothing used.
///
/// ```
/// use vehicoule_plugin_host::CallReport;
///
/// let report = CallReport::default();
/// assert_eq!(report.requests_sent, 0);
/// assert_eq!(report.output, None);
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallReport {
    /// The fuel the call consumed: all of it when it ran out.
    pub fuel_used: Fuel,
    /// The most bytes its memories and tables held together, as the memory ceiling counts them.
    pub memory_peak: ByteSize,
    /// The requests it sent to the network.
    pub requests_sent: u32,
    /// The requests the sandbox refused; they never reached the network (section 5).
    pub requests_refused: u32,
    /// The size of what the plugin returned, a value or its own error, as the output budget counts it; `None` when
    /// it returned nothing.
    pub output: Option<ByteSize>,
    /// What the plugin logged, in order, within the log budget, for the edge to write where it keeps logs; never to
    /// show to the user as app text (section 5).
    pub logs: Vec<LogEntry>,
    /// The messages logged after the budget's last, dropped.
    pub logs_dropped: u32,
}

impl Default for CallReport {
    fn default() -> Self {
        Self {
            fuel_used: Fuel::new(0),
            memory_peak: ByteSize::new(0),
            requests_sent: 0,
            requests_refused: 0,
            output: None,
            logs: Vec::new(),
            logs_dropped: 0,
        }
    }
}

/// A call's outcome with its report.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reported<T> {
    /// The plugin's value, or why there is none.
    pub outcome: Result<T, PluginFailure>,
    /// What the call used.
    pub report: CallReport,
}

impl PluginHost {
    /// One page of results for `query`, of the given kinds, from `cursor` (a `next` of an earlier page) or from the
    /// start.
    ///
    /// # Errors
    ///
    /// The outcome is a [`PluginFailure`] when the plugin returned its own error, went over a budget or trapped, or
    /// does not provide search.
    #[must_use]
    pub fn search(
        &self,
        plugin: &InstalledPlugin,
        query: &str,
        kinds: &[ItemKind],
        cursor: Option<&str>,
    ) -> Reported<Page> {
        let kinds: Vec<types::ItemKind> = kinds.iter().copied().map(Into::into).collect();
        let (answer, report) = self.call(plugin, Capability::Search, |store, pre| {
            let instance = SearchPluginPre::new(pre.clone())?.instantiate(&mut *store)?;
            instance
                .vehicoule_plugin_search()
                .call_search(store, query, &kinds, cursor)
        });
        self.bounded(answer.map(|answer| answer.map(Page::from)), report)
    }

    /// Completions of what the user is typing, in the plugin's order.
    ///
    /// The outcome is a [`PluginFailure`] as for [`PluginHost::search`].
    #[must_use]
    pub fn suggest(&self, plugin: &InstalledPlugin, prefix: &str) -> Reported<Vec<String>> {
        let (answer, report) = self.call(plugin, Capability::Search, |store, pre| {
            let instance = SearchPluginPre::new(pre.clone())?.instantiate(&mut *store)?;
            instance
                .vehicoule_plugin_search()
                .call_suggest(store, prefix)
        });
        self.bounded(answer, report)
    }

    /// The plugin's answer, or the failure it names, once its size is checked against the output budget and written
    /// in the report. A plugin error's text counts too: it is output like any other.
    fn bounded<T: OutputSize>(
        &self,
        answer: Result<Result<T, types::Error>, PluginFailure>,
        mut report: CallReport,
    ) -> Reported<T> {
        let answer = match answer {
            Ok(answer) => answer.map_err(PluginError::from),
            Err(failure) => {
                return Reported {
                    outcome: Err(failure),
                    report,
                };
            }
        };
        let size = match &answer {
            Ok(value) => output_bytes(value),
            Err(error) => output_bytes(error),
        };
        report.output = Some(size);
        let limit = self.config().max_output;
        let outcome = if size > limit {
            Err(PluginFailure::OutputTooLarge { size, limit })
        } else {
            answer.map_err(PluginFailure::Plugin)
        };
        Reported { outcome, report }
    }
}

impl OutputSize for PluginError {
    fn output_size(&self) -> usize {
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
        assert_eq!(PluginError::InvalidInput("abc".to_owned()).output_size(), 3);
        assert_eq!(PluginError::Upstream("abcd".to_owned()).output_size(), 4);
        assert_eq!(PluginError::NotFound.output_size(), 0);
        assert_eq!(PluginError::Unsupported.output_size(), 0);
        assert_eq!(
            PluginError::RateLimited { retry_after: None }.output_size(),
            0
        );
    }
}
