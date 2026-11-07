//! Helpers shared by the host's integration tests.
//!
//! The plugins are built by `scripts/build-plugin-fixtures.sh` into the directory named by
//! `VEHICOULE_PLUGIN_FIXTURES`. A missing directory fails the tests: they are never skipped.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use vehicoule_plugin_host::{
    ByteSize, Fuel, HostConfig, HostError, HttpBudget, HttpRequest, HttpResponse, LogBudget,
    Network, NetworkError, PluginHost, RequestLimits, Ticker, Ticks,
};

/// Arbitrary budgets, large enough for a well-behaved plugin (the spec gives no value; tests choose).
pub(crate) const MAX_PLUGIN_SIZE: u64 = 8 << 20;
pub(crate) const FUEL: u64 = 50_000_000;
/// Never reached unless a test ticks: nothing else ticks the hosts of these tests.
pub(crate) const WALL_TIME: u64 = 10;
pub(crate) const MAX_MEMORY: u64 = 32 << 20;
pub(crate) const MAX_OUTPUT: u64 = 64 << 10;
pub(crate) const MAX_REQUESTS: u32 = 3;
pub(crate) const MAX_RESPONSE_BODY: u64 = 1024;
pub(crate) const REQUEST_DEADLINE: Duration = Duration::from_secs(10);
pub(crate) const MAX_LOG_MESSAGES: u32 = 4;
pub(crate) const MAX_LOG_MESSAGE: u64 = 16;

/// The bytes of a test plugin; an error names what is missing, and the calling test unwraps it and fails.
pub(crate) fn fixture(name: &str) -> Result<Vec<u8>, String> {
    let dir = std::env::var_os("VEHICOULE_PLUGIN_FIXTURES").ok_or(
        "VEHICOULE_PLUGIN_FIXTURES must name the output of scripts/build-plugin-fixtures.sh",
    )?;
    let path = PathBuf::from(dir).join(format!("{name}.wasm"));
    std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

type Answer = dyn Fn(&HttpRequest) -> Result<HttpResponse, NetworkError> + Send + Sync;

/// A network that never opens a socket: it records every request it is given, ticks its host's wall time as a slow
/// network would let time pass, and answers with `answer`.
pub(crate) struct FakeNetwork {
    requests: Mutex<Vec<(HttpRequest, RequestLimits)>>,
    ticks: Mutex<Option<(Ticker, u64)>>,
    answer: Box<Answer>,
}

impl FakeNetwork {
    pub(crate) fn new(
        answer: impl Fn(&HttpRequest) -> Result<HttpResponse, NetworkError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            ticks: Mutex::new(None),
            answer: Box::new(answer),
        })
    }

    /// Every request that reached the network, with its limits, in order.
    pub(crate) fn requests(&self) -> Vec<(HttpRequest, RequestLimits)> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Network for FakeNetwork {
    fn fetch(
        &self,
        request: &HttpRequest,
        limits: RequestLimits,
    ) -> Result<HttpResponse, NetworkError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((request.clone(), limits));
        if let Some((ticker, count)) = &*self.ticks.lock().unwrap_or_else(PoisonError::into_inner) {
            for _ in 0..*count {
                ticker.tick();
            }
        }
        (self.answer)(request)
    }
}

/// A network that answers every request with `200 ok`.
pub(crate) fn ok_network() -> Arc<FakeNetwork> {
    FakeNetwork::new(|_| Ok(HttpResponse::new(200, Vec::new(), b"ok".to_vec())))
}

fn config(max_plugin_size: u64, max_output: u64, wall_time: u64) -> HostConfig {
    HostConfig::new(
        ByteSize::new(max_plugin_size),
        Fuel::new(FUEL),
        Ticks::new(wall_time),
        ByteSize::new(MAX_MEMORY),
        ByteSize::new(max_output),
        HttpBudget::new(
            MAX_REQUESTS,
            ByteSize::new(MAX_RESPONSE_BODY),
            REQUEST_DEADLINE,
        ),
        LogBudget::new(MAX_LOG_MESSAGES, ByteSize::new(MAX_LOG_MESSAGE)),
    )
}

pub(crate) fn host_with(max_plugin_size: u64, max_output: u64) -> Result<PluginHost, HostError> {
    PluginHost::new(config(max_plugin_size, max_output, WALL_TIME), ok_network())
}

pub(crate) fn host() -> Result<PluginHost, HostError> {
    host_with(MAX_PLUGIN_SIZE, MAX_OUTPUT)
}

/// A host with the test budgets whose plugins reach `network`.
pub(crate) fn host_on(network: Arc<FakeNetwork>) -> Result<PluginHost, HostError> {
    PluginHost::new(config(MAX_PLUGIN_SIZE, MAX_OUTPUT, WALL_TIME), network)
}

/// A host whose calls last `wall_time` ticks, and whose `network` ticks it `ticks_per_request` times for every request
/// it is given.
pub(crate) fn host_ticking(
    network: &Arc<FakeNetwork>,
    wall_time: u64,
    ticks_per_request: u64,
) -> Result<PluginHost, HostError> {
    let host = PluginHost::new(
        config(MAX_PLUGIN_SIZE, MAX_OUTPUT, wall_time),
        Arc::clone(network) as Arc<dyn Network>,
    )?;
    *network.ticks.lock().unwrap_or_else(PoisonError::into_inner) =
        Some((host.ticker(), ticks_per_request));
    Ok(host)
}
