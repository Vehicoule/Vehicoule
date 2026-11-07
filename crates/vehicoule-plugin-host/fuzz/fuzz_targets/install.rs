//! Installs any bytes as a plugin: the host never panics or crashes, and a plugin it accepts has a manifest that
//! passes the manifest rules again (docs/plugin-host-spec.md, sections 3 to 5).
//!
//! A plugin's bytes are the most exposed input of the app: they come from a feed, and the host must reject anything
//! that is not a well-behaved plugin with a typed error.

#![no_main]

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use vehicoule_plugin_host::{
    ByteSize, Fuel, HostConfig, HttpBudget, HttpRequest, HttpResponse, Manifest, Network,
    NetworkError, PluginHost, RequestLimits, Ticks,
};

/// Install refuses every request a plugin makes in `describe`, so this network is never reached; if it were, the
/// target would report it.
struct Unreachable;

impl Network for Unreachable {
    fn fetch(
        &self,
        _request: &HttpRequest,
        _limits: RequestLimits,
    ) -> Result<HttpResponse, NetworkError> {
        panic!("install reached the network");
    }
}

/// One host for every input: building Wasmtime's engine for each would leave little time for the inputs.
fn host() -> &'static PluginHost {
    static HOST: OnceLock<PluginHost> = OnceLock::new();
    HOST.get_or_init(|| {
        // Budgets of the host's integration tests (tests/common/mod.rs), except a smaller fuel budget: an input that
        // loops in `describe` costs a run per input, so it must end quickly. The spec gives no value.
        let config = HostConfig::new(
            ByteSize::new(8 << 20),
            Fuel::new(1_000_000),
            Ticks::new(10),
            ByteSize::new(32 << 20),
            ByteSize::new(64 << 10),
            HttpBudget::new(3, ByteSize::new(1024), Duration::from_secs(10)),
        );
        match PluginHost::new(config, Arc::new(Unreachable)) {
            Ok(host) => host,
            Err(error) => panic!("the host cannot be built: {error}"),
        }
    })
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(plugin) = host().install(bytes) else {
        return;
    };
    let manifest = plugin.manifest();
    // Host patterns are checked by the `host_pattern` target; they have no text form to build a manifest from again.
    let again = Manifest::new(
        manifest.id().as_str(),
        manifest.name(),
        manifest.version(),
        manifest.domains().iter().copied(),
        manifest.capabilities().iter().copied(),
        Vec::<&str>::new(),
    );
    assert!(
        again.is_ok(),
        "an installed plugin's manifest breaks the manifest rules: {again:?}"
    );
});
