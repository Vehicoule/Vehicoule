//! The host's budgets (docs/plugin-host-spec.md, section 6).
//!
//! No value is written here: the edge gives every budget, tests use arbitrary ones, and the real values are set from
//! measurements in an ADR.

use std::time::Duration;

/// A size in bytes.
///
/// ```
/// use vehicoule_plugin_host::ByteSize;
///
/// assert_eq!(ByteSize::new(4096).get(), 4096);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(u64);

impl ByteSize {
    /// A size of `bytes` bytes.
    #[must_use]
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// The size in bytes.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The size as a `usize`, saturated to `usize::MAX` where it does not fit: a ceiling larger than the address
    /// space is no ceiling.
    pub(crate) fn saturating_usize(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

/// An amount of Wasmtime fuel, the deterministic CPU budget of a call.
///
/// ```
/// use vehicoule_plugin_host::Fuel;
///
/// assert_eq!(Fuel::new(1_000).get(), 1_000);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fuel(u64);

impl Fuel {
    /// `units` units of fuel.
    #[must_use]
    pub const fn new(units: u64) -> Self {
        Self(units)
    }

    /// The number of units.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A number of ticks of the host's [`Ticker`](crate::Ticker), the wall-time budget of a call.
///
/// The edge ticks at a period it chooses; a call given `n` ticks ends at the `n`-th tick after it started, so it ran
/// between `n - 1` and `n` periods.
///
/// ```
/// use vehicoule_plugin_host::Ticks;
///
/// assert_eq!(Ticks::new(20).get(), 20);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticks(u64);

impl Ticks {
    /// `count` ticks.
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    /// The number of ticks.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The HTTP budget of one call.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_plugin_host::{ByteSize, HttpBudget};
///
/// let http = HttpBudget::new(10, ByteSize::new(2 << 20), Duration::from_secs(10));
/// assert_eq!(http.max_requests, 10);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpBudget {
    /// The most requests a call may send to the network; refused requests do not count, as they never reach it.
    pub max_requests: u32,
    /// The largest response body a request may receive.
    pub max_response_body: ByteSize,
    /// How long one request may take.
    pub request_deadline: Duration,
}

impl HttpBudget {
    /// An HTTP budget from the edge's configuration.
    #[must_use]
    pub const fn new(
        max_requests: u32,
        max_response_body: ByteSize,
        request_deadline: Duration,
    ) -> Self {
        Self {
            max_requests,
            max_response_body,
            request_deadline,
        }
    }
}

/// The log budget of one call: how many messages a plugin may log, and how long each may be (section 5).
///
/// ```
/// use vehicoule_plugin_host::{ByteSize, LogBudget};
///
/// let log = LogBudget::new(20, ByteSize::new(512));
/// assert_eq!(log.max_messages, 20);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogBudget {
    /// The most messages a call keeps; the ones after are counted and dropped.
    pub max_messages: u32,
    /// The longest message kept; a longer one is cut at a character boundary.
    pub max_message: ByteSize,
}

impl LogBudget {
    /// A log budget from the edge's configuration.
    #[must_use]
    pub const fn new(max_messages: u32, max_message: ByteSize) -> Self {
        Self {
            max_messages,
            max_message,
        }
    }
}

/// The budgets the host enforces.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_plugin_host::{ByteSize, Fuel, HostConfig, HttpBudget, LogBudget, Ticks};
///
/// let config = HostConfig::new(
///     ByteSize::new(8 << 20),
///     Fuel::new(50_000_000),
///     Ticks::new(20),
///     ByteSize::new(64 << 20),
///     ByteSize::new(1 << 20),
///     HttpBudget::new(10, ByteSize::new(2 << 20), Duration::from_secs(10)),
///     LogBudget::new(20, ByteSize::new(512)),
/// );
/// assert_eq!(config.max_plugin_size, ByteSize::new(8 << 20));
/// assert_eq!(config.max_output, ByteSize::new(1 << 20));
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostConfig {
    /// The largest plugin accepted, checked before any parsing or compilation.
    pub max_plugin_size: ByteSize,
    /// The fuel given to each call into a plugin, `describe` included.
    pub fuel_per_call: Fuel,
    /// The wall time each call into a plugin may last, `describe` included, in ticks of the host's
    /// [`Ticker`](crate::Ticker). It ends a call blocked where fuel does not run out, such as a slow request.
    pub wall_time: Ticks,
    /// The ceiling on the bytes a call holds in all of its linear memories and tables together.
    pub max_memory: ByteSize,
    /// The ceiling on the size of what a call returns: the bytes of every string in the value, plus one for every
    /// element of every list.
    pub max_output: ByteSize,
    /// The HTTP budget of each call.
    pub http: HttpBudget,
    /// The log budget of each call.
    pub log: LogBudget,
}

impl HostConfig {
    /// Budgets from the edge's configuration.
    #[must_use]
    pub const fn new(
        max_plugin_size: ByteSize,
        fuel_per_call: Fuel,
        wall_time: Ticks,
        max_memory: ByteSize,
        max_output: ByteSize,
        http: HttpBudget,
        log: LogBudget,
    ) -> Self {
        Self {
            max_plugin_size,
            fuel_per_call,
            wall_time,
            max_memory,
            max_output,
            http,
            log,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_usize_keeps_a_value_that_fits() {
        assert_eq!(ByteSize::new(65_536).saturating_usize(), 65_536);
        assert_eq!(ByteSize::new(0).saturating_usize(), 0);
    }
}
