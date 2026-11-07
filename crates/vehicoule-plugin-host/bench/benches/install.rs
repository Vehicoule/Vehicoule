//! Instruction count of installing a plugin: compiling its component, linking it and running its `describe`
//! (ADR 0016).
//!
//! The benchmark builds its host and reads the plugin's bytes in a setup that is not counted, then installs it once,
//! and fails when the install runs more instructions than its ceiling. The plugin is the host's `good` test plugin,
//! built by `scripts/build-plugin-fixtures.sh` into the directory named by `VEHICOULE_PLUGIN_FIXTURES`. A missing
//! directory fails the benchmark: it is never skipped.

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use vehicoule_plugin_host::{
    ByteSize, Fuel, HostConfig, HttpBudget, HttpRequest, HttpResponse, InstallError,
    InstalledPlugin, LogBudget, Network, NetworkError, PluginHost, RequestLimits, Ticks,
};

/// The variable naming the directory of the test plugins, as the host's integration tests read it.
const FIXTURES: &str = "VEHICOULE_PLUGIN_FIXTURES";

/// A network that answers nothing: installing calls only `describe`, which the host gives no network.
struct Offline;

impl Network for Offline {
    fn fetch(
        &self,
        _request: &HttpRequest,
        _limits: RequestLimits,
    ) -> Result<HttpResponse, NetworkError> {
        Err(NetworkError::Transport("offline".to_owned()))
    }
}

/// A host with the budgets of the host's integration tests, which the spec leaves to the app, and the bytes of the
/// `good` test plugin.
fn host_and_good_plugin() -> (PluginHost, Vec<u8>) {
    let config = HostConfig::new(
        ByteSize::new(8 << 20),
        Fuel::new(50_000_000),
        Ticks::new(10),
        ByteSize::new(32 << 20),
        ByteSize::new(64 << 10),
        HttpBudget::new(3, ByteSize::new(1024), Duration::from_secs(10)),
        LogBudget::new(4, ByteSize::new(16)),
    );
    let host = PluginHost::new(config, Arc::new(Offline))
        .expect("Wasmtime takes the host's configuration");
    let dir = std::env::var_os(FIXTURES)
        .expect("VEHICOULE_PLUGIN_FIXTURES names the output of scripts/build-plugin-fixtures.sh");
    let path = PathBuf::from(dir).join("good.wasm");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    (host, bytes)
}

/// The least margin over a recorded count, in instructions, as in the core's benchmarks (ADR 0016).
const LEAST_MARGIN: u64 = 100;

/// The configuration of a benchmark whose operation ran `measured` instructions when its ceiling was set: it fails
/// above 10 % more, or [`LEAST_MARGIN`] more when that is larger (ADR 0016). `measured` comes from the CI's `bench`
/// job, and raising it is the user's decision. The run keeps [`FIXTURES`], which gungraun clears by default.
fn measured(measured: u64) -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    let ceiling = measured + (measured / 10).max(LEAST_MARGIN);
    config.pass_through_env(FIXTURES);
    config.tool(Callgrind::default().hard_limits([(EventKind::Ir, ceiling)]));
    config
}

#[library_benchmark]
#[bench::good(setup = host_and_good_plugin, config = measured(645_716_273))]
fn install(
    (host, bytes): (PluginHost, Vec<u8>),
) -> (PluginHost, Result<InstalledPlugin, InstallError>) {
    let installed = black_box(host.install(black_box(&bytes)));
    if let Err(error) = &installed {
        panic!("the good test plugin installs: {error}");
    }
    (host, installed)
}

library_benchmark_group!(name = plugin, benchmarks = [install]);

main!(library_benchmark_groups = plugin);
