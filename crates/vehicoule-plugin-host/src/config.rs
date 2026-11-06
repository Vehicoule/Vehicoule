//! The host's budgets (docs/plugin-host-spec.md, section 6).
//!
//! No value is written here: the edge gives every budget, tests use arbitrary ones, and the real values are set from
//! measurements in an ADR.

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

/// The budgets the host enforces.
///
/// ```
/// use vehicoule_plugin_host::{ByteSize, Fuel, HostConfig};
///
/// let config = HostConfig::new(
///     ByteSize::new(8 << 20),
///     Fuel::new(50_000_000),
///     ByteSize::new(64 << 20),
///     ByteSize::new(1 << 20),
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
    /// The ceiling on the bytes a call holds in all of its linear memories and tables together.
    pub max_memory: ByteSize,
    /// The ceiling on the size of what a call returns: the bytes of every string in the value, plus one for every
    /// element of every list.
    pub max_output: ByteSize,
}

impl HostConfig {
    /// Budgets from the edge's configuration.
    #[must_use]
    pub const fn new(
        max_plugin_size: ByteSize,
        fuel_per_call: Fuel,
        max_memory: ByteSize,
        max_output: ByteSize,
    ) -> Self {
        Self {
            max_plugin_size,
            fuel_per_call,
            max_memory,
            max_output,
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
