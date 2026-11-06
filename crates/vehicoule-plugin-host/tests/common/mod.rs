//! Helpers shared by the host's integration tests.
//!
//! The plugins are built by `scripts/build-plugin-fixtures.sh` into the directory named by
//! `VEHICOULE_PLUGIN_FIXTURES`. A missing directory fails the tests: they are never skipped.

use std::path::PathBuf;

use vehicoule_plugin_host::{ByteSize, Fuel, HostConfig, HostError, PluginHost};

/// Arbitrary budgets, large enough for a well-behaved plugin (the spec gives no value; tests choose).
pub(crate) const MAX_PLUGIN_SIZE: u64 = 8 << 20;
pub(crate) const FUEL: u64 = 50_000_000;
pub(crate) const MAX_MEMORY: u64 = 32 << 20;
pub(crate) const MAX_OUTPUT: u64 = 64 << 10;

/// The bytes of a test plugin; an error names what is missing, and the calling test unwraps it and fails.
pub(crate) fn fixture(name: &str) -> Result<Vec<u8>, String> {
    let dir = std::env::var_os("VEHICOULE_PLUGIN_FIXTURES").ok_or(
        "VEHICOULE_PLUGIN_FIXTURES must name the output of scripts/build-plugin-fixtures.sh",
    )?;
    let path = PathBuf::from(dir).join(format!("{name}.wasm"));
    std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

pub(crate) fn host_with(max_plugin_size: u64) -> Result<PluginHost, HostError> {
    PluginHost::new(HostConfig::new(
        ByteSize::new(max_plugin_size),
        Fuel::new(FUEL),
        ByteSize::new(MAX_MEMORY),
        ByteSize::new(MAX_OUTPUT),
    ))
}

pub(crate) fn host() -> Result<PluginHost, HostError> {
    host_with(MAX_PLUGIN_SIZE)
}
