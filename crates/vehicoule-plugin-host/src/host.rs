//! Installing a plugin: size, compilation, imports, `describe`, manifest, exports (docs/plugin-host-spec.md,
//! sections 3 to 5); and the store every call into a plugin gets, with its budgets and its network (sections 5 and
//! 6).

use std::sync::Arc;

use thiserror::Error;
use wasmtime::component::{Component, HasSelf, InstancePre, Linker};
use wasmtime::{Engine, ResourceLimiter, Store, Trap};

use crate::bindings::base::vehicoule::plugin::{http, log, types};
use crate::bindings::{artwork, base, link_import, lyrics, metadata, radio, resolve, search};
use crate::logs::Logs;
use crate::media::{OutputSize, output_bytes};
use crate::network::{HeaderRefusal, HttpsUrl, RequestLimits, UrlRefusal, header_refusal};
use crate::{
    ByteSize, CallReport, Capability, Domain, Fuel, HostConfig, HostPattern, HttpBudget,
    HttpRequest, Manifest, ManifestError, Method, Network, NetworkError, PluginFailure,
};

/// The only imports a plugin may have (section 5).
const ALLOWED_IMPORTS: [&str; 3] = [
    "vehicoule:plugin/types@0.1.0",
    "vehicoule:plugin/http@0.1.0",
    "vehicoule:plugin/log@0.1.0",
];

/// Why the host could not start.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum HostError {
    /// Wasmtime refused the engine or the linker.
    #[error("the Wasm engine could not be set up: {0}")]
    Engine(String),
}

/// Why a plugin was rejected at install. Nothing is kept when this is returned.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum InstallError {
    /// The plugin is over the size ceiling; it was neither parsed nor compiled.
    #[error("the plugin is {} bytes, over the ceiling of {} bytes", size.get(), limit.get())]
    TooLarge {
        /// The plugin's size.
        size: ByteSize,
        /// The ceiling.
        limit: ByteSize,
    },
    /// The bytes are not a valid component.
    #[error("the plugin does not compile: {0}")]
    Compile(String),
    /// The plugin imports something outside the contract, WASI included.
    #[error("the plugin imports `{0}`, which is outside the contract")]
    ForbiddenImport(String),
    /// The plugin's imports do not match the contract's types.
    #[error("the plugin does not link against the contract: {0}")]
    Link(String),
    /// `describe` ran out of fuel.
    #[error("the plugin ran out of fuel")]
    FuelExhausted,
    /// `describe` ran past its wall time.
    #[error("the plugin ran past its wall time")]
    WallTime,
    /// The plugin asked for more memory than the ceiling.
    #[error("the plugin went over its memory ceiling")]
    MemoryLimit,
    /// The plugin trapped (crashed).
    #[error("the plugin trapped: {0}")]
    Trap(String),
    /// The manifest is over the output budget, which bounds `describe` as any call.
    #[error("the manifest has a size of {}, over the ceiling of {}", size.get(), limit.get())]
    OutputTooLarge {
        /// The manifest's size.
        size: ByteSize,
        /// The ceiling.
        limit: ByteSize,
    },
    /// The manifest breaks a rule of section 4.
    #[error("invalid manifest: {0}")]
    InvalidManifest(#[from] ManifestError),
    /// A capability is declared but not exported, or exported but not declared.
    #[error("capability {capability:?} is {}", if *declared { "declared but not exported" } else { "exported but not declared" })]
    CapabilityMismatch {
        /// The capability.
        capability: Capability,
        /// Whether the manifest declares it (and the component does not export it).
        declared: bool,
    },
    /// Wasmtime failed for a reason that is none of the above.
    #[error("the Wasm runtime failed: {0}")]
    Runtime(String),
}

/// A plugin that passed every install check, ready to be called.
pub struct InstalledPlugin {
    manifest: Manifest,
    // Compiled and linked once; each call instantiates it in a fresh store.
    pre: InstancePre<CallState>,
}

impl std::fmt::Debug for InstalledPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstalledPlugin")
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

impl InstalledPlugin {
    /// The plugin's validated manifest, for the edge to store and show to the user.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// The state of one store: one call, then dropped (section 3).
pub(crate) struct CallState {
    limiter: MemoryLimiter,
    http: Http,
    /// Set when an HTTP budget ended the call, so the failure names it rather than the trap that stopped the plugin.
    http_stop: Option<HttpStop>,
    /// The requests of a capability call that the sandbox refused, for the call's report.
    requests_refused: u32,
    /// What a capability call logged; `None` during install, whose messages are dropped.
    logs: Option<Logs>,
}

/// What a call may do on the network.
enum Http {
    /// During install every request is refused (section 3): the plugin has no grant yet.
    Refused,
    /// A capability call: the manifest's hosts, within the call's budget.
    Granted {
        hosts: Vec<HostPattern>,
        network: Arc<dyn Network>,
        budget: HttpBudget,
        sent: u32,
    },
}

/// An HTTP budget that ended a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HttpStop {
    TooManyRequests,
    ResponseTooLarge,
    RequestTimeout,
}

impl CallState {
    /// What the call used, read once it returned or stopped. `fuel` is the call's whole fuel and `fuel_left` what
    /// Wasmtime says is left of it.
    fn report(self, fuel: Fuel, fuel_left: u64) -> CallReport {
        let (logs, logs_dropped) = self
            .logs
            .map_or((Vec::new(), 0), |logs| (logs.entries, logs.dropped));
        CallReport {
            fuel_used: Fuel::new(fuel.get().saturating_sub(fuel_left)),
            memory_peak: ByteSize::new(u64::try_from(self.limiter.used_bytes).unwrap_or(u64::MAX)),
            requests_sent: match self.http {
                Http::Granted { sent, .. } => sent,
                Http::Refused => 0,
            },
            requests_refused: self.requests_refused,
            output: None,
            logs,
            logs_dropped,
        }
    }

    /// Ends the call because of `stop`.
    fn stop(&mut self, stop: HttpStop) -> wasmtime::Error {
        self.http_stop = Some(stop);
        wasmtime::format_err!("an HTTP budget ended the call: {stop:?}")
    }
}

impl types::Host for CallState {}

impl http::Host for CallState {
    /// Checks the request (section 5) and, if it passes, counts it and hands it to the network. A refused request
    /// returns an error to the plugin and never reaches the network; an exceeded budget ends the call (section 6).
    fn fetch(
        &mut self,
        request: http::Request,
    ) -> wasmtime::Result<Result<http::Response, http::HttpError>> {
        let Http::Granted {
            hosts,
            network,
            budget,
            sent,
        } = &mut self.http
        else {
            return Ok(Err(http::HttpError::RefusedHost));
        };
        let url = match checked_url(&request, hosts) {
            Ok(url) => url,
            Err(refusal) => {
                self.requests_refused = self.requests_refused.saturating_add(1);
                return Ok(Err(refusal));
            }
        };
        if *sent >= budget.max_requests {
            return Err(self.stop(HttpStop::TooManyRequests));
        }
        *sent = sent.saturating_add(1);
        let limits = RequestLimits::new(budget.max_response_body, budget.request_deadline);
        let checked = HttpRequest {
            method: match request.method {
                http::Method::Get => Method::Get,
                http::Method::Head => Method::Head,
                http::Method::Post => Method::Post,
            },
            url,
            headers: request.headers,
            body: request.body,
        };
        match network.fetch(&checked, limits) {
            Ok(response)
                if ByteSize::new(u64::try_from(response.body.len()).unwrap_or(u64::MAX))
                    > limits.max_body =>
            {
                Err(self.stop(HttpStop::ResponseTooLarge))
            }
            Ok(response) => Ok(Ok(http::Response {
                status: response.status,
                headers: response.headers,
                body: response.body,
            })),
            Err(NetworkError::BodyTooLarge) => Err(self.stop(HttpStop::ResponseTooLarge)),
            Err(NetworkError::Timeout) => Err(self.stop(HttpStop::RequestTimeout)),
            Err(NetworkError::Transport(message)) => Ok(Err(http::HttpError::Transport(message))),
        }
    }
}

/// The request's URL if the sandbox lets the request through to `hosts` (section 5), or the error the plugin gets.
fn checked_url(
    request: &http::Request,
    hosts: &[HostPattern],
) -> Result<HttpsUrl, http::HttpError> {
    let url = match HttpsUrl::parse(&request.url) {
        Ok(url) => url,
        Err(UrlRefusal::Scheme) => return Err(http::HttpError::RefusedScheme),
        Err(UrlRefusal::Host) => return Err(http::HttpError::RefusedHost),
    };
    if !hosts.iter().any(|pattern| pattern.matches(url.host())) {
        return Err(http::HttpError::RefusedHost);
    }
    for (name, value) in &request.headers {
        match header_refusal(name, value) {
            Some(HeaderRefusal::Host) => return Err(http::HttpError::RefusedHost),
            Some(HeaderRefusal::Malformed) => {
                return Err(http::HttpError::Transport(format!(
                    "header `{}` cannot be sent",
                    name.escape_debug()
                )));
            }
            None => {}
        }
    }
    Ok(url)
}

impl log::Host for CallState {
    /// Keeps the message for the call's report, within the log budget (section 5). During install it is dropped.
    fn log(&mut self, level: log::Level, message: String) -> wasmtime::Result<()> {
        if let Some(logs) = &mut self.logs {
            logs.push(level.into(), &message);
        }
        Ok(())
    }
}

/// Bounds the bytes held by all of a call's linear memories and tables together, and remembers that a growth was
/// denied so the failure is reported as a memory limit rather than as the trap the plugin raises when its allocator
/// fails.
///
/// The ceiling is on the sum: a component can hold several memories (several core instances, or multi-memory), and a
/// per-memory check would let each reach the ceiling. Wasmtime reports every creation (from zero) and every growth.
/// A growth it allowed and then failed stays counted: `memory_grow_failed` is also called for growths never offered
/// here, so refunding on it could shrink the count below what the plugin holds.
#[derive(Debug)]
pub(crate) struct MemoryLimiter {
    max_bytes: usize,
    used_bytes: usize,
    denied: bool,
}

impl MemoryLimiter {
    pub(crate) fn new(max: ByteSize) -> Self {
        Self {
            max_bytes: max.saturating_usize(),
            used_bytes: 0,
            denied: false,
        }
    }

    fn allow(&mut self, current_bytes: usize, desired_bytes: usize) -> bool {
        let total = self
            .used_bytes
            .saturating_add(desired_bytes.saturating_sub(current_bytes));
        let allowed = total <= self.max_bytes;
        if allowed {
            self.used_bytes = total;
        } else {
            self.denied = true;
        }
        allowed
    }
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(self.allow(current, desired))
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Wasmtime stores one pointer per table element (`ResourceLimiter::table_growing`).
        let element = size_of::<usize>();
        Ok(self.allow(
            current.saturating_mul(element),
            desired.saturating_mul(element),
        ))
    }
}

/// Runs plugins. One per app; installs and calls are independent of each other.
pub struct PluginHost {
    engine: Engine,
    linker: Linker<CallState>,
    config: HostConfig,
    network: Arc<dyn Network>,
}

impl PluginHost {
    /// A host enforcing `config`, whose plugins' checked requests go to `network`.
    ///
    /// # Errors
    ///
    /// [`HostError::Engine`] when Wasmtime refuses the configuration.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use vehicoule_plugin_host::{
    ///     ByteSize, Fuel, HostConfig, HttpBudget, HttpRequest, HttpResponse, Network, NetworkError, PluginHost,
    ///     LogBudget, RequestLimits, Ticks,
    /// };
    ///
    /// struct Offline;
    ///
    /// impl Network for Offline {
    ///     fn fetch(&self, _request: &HttpRequest, _limits: RequestLimits) -> Result<HttpResponse, NetworkError> {
    ///         Err(NetworkError::Transport("offline".to_owned()))
    ///     }
    /// }
    ///
    /// let config = HostConfig::new(
    ///     ByteSize::new(1 << 20),
    ///     Fuel::new(1_000_000),
    ///     Ticks::new(20),
    ///     ByteSize::new(16 << 20),
    ///     ByteSize::new(1 << 20),
    ///     HttpBudget::new(10, ByteSize::new(1 << 20), Duration::from_secs(10)),
    ///     LogBudget::new(20, ByteSize::new(512)),
    /// );
    /// assert!(PluginHost::new(config, Arc::new(Offline)).is_ok());
    /// ```
    pub fn new(config: HostConfig, network: Arc<dyn Network>) -> Result<Self, HostError> {
        let mut wasm = wasmtime::Config::new();
        wasm.consume_fuel(true);
        wasm.epoch_interruption(true);
        let engine = Engine::new(&wasm).map_err(|e| HostError::Engine(e.to_string()))?;
        let mut linker = Linker::new(&engine);
        base::Base::add_to_linker::<CallState, HasSelf<CallState>>(&mut linker, |state| state)
            .map_err(|e| HostError::Engine(e.to_string()))?;
        Ok(Self {
            engine,
            linker,
            config,
            network,
        })
    }

    /// Checks and describes a plugin whose bytes the feed layer already verified.
    ///
    /// # Errors
    ///
    /// The first check the plugin fails, as an [`InstallError`]; nothing is kept.
    pub fn install(&self, bytes: &[u8]) -> Result<InstalledPlugin, InstallError> {
        let size = ByteSize::new(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if size > self.config.max_plugin_size {
            return Err(InstallError::TooLarge {
                size,
                limit: self.config.max_plugin_size,
            });
        }
        let component = Component::new(&self.engine, bytes)
            .map_err(|e| InstallError::Compile(e.to_string()))?;
        if let Some((name, _)) = component
            .component_type()
            .imports(&self.engine)
            .find(|(name, _)| !ALLOWED_IMPORTS.contains(name))
        {
            return Err(InstallError::ForbiddenImport(name.to_owned()));
        }
        let pre = self
            .linker
            .instantiate_pre(&component)
            .map_err(|e| InstallError::Link(e.to_string()))?;
        let raw = self.describe(&pre)?;
        let size = output_bytes(&raw);
        if size > self.config.max_output {
            return Err(InstallError::OutputTooLarge {
                size,
                limit: self.config.max_output,
            });
        }
        let manifest = Manifest::new(
            &raw.id,
            &raw.name,
            &raw.version,
            raw.domains.iter().map(|d| domain(*d)),
            raw.capabilities.iter().map(|c| capability(*c)),
            &raw.http_hosts,
        )?;
        for capability in Capability::ALL {
            let declared = manifest.capabilities().contains(&capability);
            if declared != exports(capability, &pre) {
                return Err(InstallError::CapabilityMismatch {
                    capability,
                    declared,
                });
            }
        }
        Ok(InstalledPlugin { manifest, pre })
    }

    /// The clock of the calls' wall time, for the edge to tick.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use std::time::Duration;
    /// # use vehicoule_plugin_host::{
    /// #     ByteSize, Fuel, HostConfig, HttpBudget, HttpRequest, HttpResponse, Network, NetworkError, PluginHost,
    /// #     LogBudget, RequestLimits, Ticks,
    /// # };
    /// # struct Offline;
    /// # impl Network for Offline {
    /// #     fn fetch(&self, _request: &HttpRequest, _limits: RequestLimits) -> Result<HttpResponse, NetworkError> {
    /// #         Err(NetworkError::Transport("offline".to_owned()))
    /// #     }
    /// # }
    /// # let config = HostConfig::new(
    /// #     ByteSize::new(1 << 20),
    /// #     Fuel::new(1_000_000),
    /// #     Ticks::new(20),
    /// #     ByteSize::new(16 << 20),
    /// #     ByteSize::new(1 << 20),
    /// #     HttpBudget::new(10, ByteSize::new(1 << 20), Duration::from_secs(10)),
    /// #     LogBudget::new(20, ByteSize::new(512)),
    /// # );
    /// let host = PluginHost::new(config, Arc::new(Offline))?;
    /// let ticker = host.ticker();
    /// // The edge calls this from its own timer, at the period it chose for `Ticks`.
    /// std::thread::spawn(move || ticker.tick());
    /// # Ok::<(), vehicoule_plugin_host::HostError>(())
    /// ```
    #[must_use]
    pub fn ticker(&self) -> Ticker {
        Ticker {
            engine: self.engine.clone(),
        }
    }

    /// The budgets this host enforces.
    pub(crate) fn config(&self) -> &HostConfig {
        &self.config
    }

    /// Runs `call` against a fresh instance of `plugin` if it provides `capability`, in a fresh store with the call's
    /// budgets, names what stopped it if it did not return, and reports what it used.
    pub(crate) fn call<R>(
        &self,
        plugin: &InstalledPlugin,
        capability: Capability,
        call: impl FnOnce(&mut Store<CallState>, &InstancePre<CallState>) -> wasmtime::Result<R>,
    ) -> (Result<R, PluginFailure>, CallReport) {
        if !plugin.manifest.capabilities().contains(&capability) {
            return (
                Err(PluginFailure::NotProvided(capability)),
                CallReport::default(),
            );
        }
        let http = Http::Granted {
            hosts: plugin.manifest.http_hosts().to_vec(),
            network: Arc::clone(&self.network),
            budget: self.config.http,
            sent: 0,
        };
        let logs = Logs::new(plugin.manifest.id().clone(), self.config.log);
        let mut store = match self.store(http, Some(logs)) {
            Ok(store) => store,
            Err(e) => {
                return (
                    Err(Stop::Runtime(e.to_string()).into()),
                    CallReport::default(),
                );
            }
        };
        let outcome = call(&mut store, &plugin.pre).map_err(|e| stop(&e, &store).into());
        // Fuel is always on (`PluginHost::new`), so this cannot fail; if it did, the whole fuel would be reported as
        // used, never less than was.
        let fuel_left = store.get_fuel().unwrap_or(0);
        let report = store
            .into_data()
            .report(self.config.fuel_per_call, fuel_left);
        (outcome, report)
    }

    /// A store for one call: its own memory ceiling, its own fuel, its own wall time and its own network grant.
    fn store(&self, http: Http, logs: Option<Logs>) -> wasmtime::Result<Store<CallState>> {
        let mut store = Store::new(
            &self.engine,
            CallState {
                limiter: MemoryLimiter::new(self.config.max_memory),
                http,
                http_stop: None,
                requests_refused: 0,
                logs,
            },
        );
        store.limiter(|state| &mut state.limiter);
        store.set_fuel(self.config.fuel_per_call.get())?;
        store.set_epoch_deadline(self.config.wall_time.get());
        store.epoch_deadline_trap();
        Ok(store)
    }

    /// Calls `describe` in a fresh store, with every request refused and the call's budgets.
    fn describe(
        &self,
        pre: &InstancePre<CallState>,
    ) -> Result<base::exports::vehicoule::plugin::plugin::Manifest, InstallError> {
        let mut store = self
            .store(Http::Refused, None)
            .map_err(|e| InstallError::Runtime(e.to_string()))?;
        base::BasePre::new(pre.clone())
            .and_then(|pre| pre.instantiate(&mut store))
            .and_then(|plugin| plugin.vehicoule_plugin_plugin().call_describe(&mut store))
            .map_err(|e| stop(&e, &store).into())
    }
}

/// Ticks the wall time of every call of the [`PluginHost`] it came from (section 6).
///
/// Cheap to clone and to send to another thread; a tick is one atomic increment. A call blocked in a host function,
/// such as a request, sees the ticks when it returns to the plugin's code.
#[derive(Clone, Debug)]
pub struct Ticker {
    engine: Engine,
}

impl Ticker {
    /// One tick: every call that has run for its [`HostConfig::wall_time`] ticks now ends with a wall-time failure.
    pub fn tick(&self) {
        self.engine.increment_epoch();
    }
}

impl OutputSize for base::exports::vehicoule::plugin::plugin::Manifest {
    fn output_size(&self) -> usize {
        [
            self.id.output_size(),
            self.name.output_size(),
            self.version.output_size(),
            self.http_hosts.output_size(),
            self.domains.len(),
            self.capabilities.len(),
        ]
        .iter()
        .fold(0, |total, part| total.saturating_add(*part))
    }
}

/// What stopped a plugin that did not return, at install or in a call.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    FuelExhausted,
    WallTime,
    MemoryLimit,
    Http(HttpStop),
    Trap(String),
    Runtime(String),
}

impl From<Stop> for InstallError {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::FuelExhausted => Self::FuelExhausted,
            Stop::WallTime => Self::WallTime,
            Stop::MemoryLimit => Self::MemoryLimit,
            // Install refuses every request before it is counted or sent, so no HTTP budget can end `describe`.
            Stop::Http(stop) => {
                Self::Runtime(format!("unexpected HTTP budget at install: {stop:?}"))
            }
            Stop::Trap(trap) => Self::Trap(trap),
            Stop::Runtime(error) => Self::Runtime(error),
        }
    }
}

impl From<Stop> for PluginFailure {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::FuelExhausted => Self::FuelExhausted,
            Stop::WallTime => Self::WallTime,
            Stop::MemoryLimit => Self::MemoryLimit,
            Stop::Http(HttpStop::TooManyRequests) => Self::TooManyRequests,
            Stop::Http(HttpStop::ResponseTooLarge) => Self::ResponseTooLarge,
            Stop::Http(HttpStop::RequestTimeout) => Self::RequestTimeout,
            Stop::Trap(trap) => Self::Trap(trap),
            Stop::Runtime(error) => Self::Runtime(error),
        }
    }
}

/// Names what stopped instantiation or a call: a budget the host itself ended first (an HTTP budget, or a denied
/// memory growth, which surfaces as a trap), then the trap.
fn stop(error: &wasmtime::Error, store: &Store<CallState>) -> Stop {
    if let Some(http) = store.data().http_stop {
        return Stop::Http(http);
    }
    if store.data().limiter.denied {
        return Stop::MemoryLimit;
    }
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => Stop::FuelExhausted,
        Some(Trap::Interrupt) => Stop::WallTime,
        Some(trap) => Stop::Trap(trap.to_string()),
        None => Stop::Runtime(error.to_string()),
    }
}

/// Whether the component exports the capability's interface with every function of the contract.
fn exports(capability: Capability, pre: &InstancePre<CallState>) -> bool {
    match capability {
        Capability::Search => {
            search::exports::vehicoule::plugin::search::GuestIndices::new(pre).is_ok()
        }
        Capability::Metadata => {
            metadata::exports::vehicoule::plugin::metadata::GuestIndices::new(pre).is_ok()
        }
        Capability::Resolve => {
            resolve::exports::vehicoule::plugin::resolve::GuestIndices::new(pre).is_ok()
        }
        Capability::Radio => {
            radio::exports::vehicoule::plugin::radio::GuestIndices::new(pre).is_ok()
        }
        Capability::Artwork => {
            artwork::exports::vehicoule::plugin::artwork::GuestIndices::new(pre).is_ok()
        }
        Capability::Lyrics => {
            lyrics::exports::vehicoule::plugin::lyrics::GuestIndices::new(pre).is_ok()
        }
        Capability::LinkImport => {
            link_import::exports::vehicoule::plugin::link_import::GuestIndices::new(pre).is_ok()
        }
    }
}

fn domain(domain: types::Domain) -> Domain {
    match domain {
        types::Domain::Music => Domain::Music,
        types::Domain::Video => Domain::Video,
        types::Domain::Books => Domain::Books,
    }
}

fn capability(capability: types::Capability) -> Capability {
    match capability {
        types::Capability::Search => Capability::Search,
        types::Capability::Metadata => Capability::Metadata,
        types::Capability::Resolve => Capability::Resolve,
        types::Capability::Radio => Capability::Radio,
        types::Capability::Artwork => Capability::Artwork,
        types::Capability::Lyrics => Capability::Lyrics,
        types::Capability::LinkImport => Capability::LinkImport,
    }
}

#[cfg(kani)]
mod proofs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_growth_is_allowed_up_to_the_ceiling_and_remembered_beyond() {
        let mut limiter = MemoryLimiter::new(ByteSize::new(65_536 * 4));
        assert!(limiter.memory_growing(0, 65_536 * 4, None).unwrap());
        assert!(!limiter.denied);
        assert!(
            !limiter
                .memory_growing(65_536 * 4, 65_536 * 5, None)
                .unwrap()
        );
        assert!(limiter.denied);
    }

    #[test]
    fn a_growth_counts_only_the_bytes_it_adds() {
        let mut limiter = MemoryLimiter::new(ByteSize::new(65_536 * 4));
        assert!(limiter.memory_growing(0, 65_536 * 2, None).unwrap());
        assert!(
            limiter
                .memory_growing(65_536 * 2, 65_536 * 4, None)
                .unwrap()
        );
        assert!(!limiter.denied);
    }

    #[test]
    fn memories_share_one_ceiling() {
        let mut limiter = MemoryLimiter::new(ByteSize::new(65_536 * 4));
        assert!(limiter.memory_growing(0, 65_536 * 3, None).unwrap());
        assert!(!limiter.memory_growing(0, 65_536 * 2, None).unwrap());
        assert!(limiter.denied);
        // The denied growth was not counted: what is left still fits.
        assert!(limiter.memory_growing(0, 65_536, None).unwrap());
    }

    #[test]
    fn memories_and_tables_share_one_ceiling() {
        let mut limiter = MemoryLimiter::new(ByteSize::new(65_536));
        assert!(limiter.memory_growing(0, 65_536, None).unwrap());
        assert!(!limiter.table_growing(0, 1, None).unwrap());
        assert!(limiter.denied);
    }

    #[test]
    fn table_growth_counts_one_pointer_per_element() {
        let elements = 1024;
        let bytes = elements * size_of::<usize>();
        let mut limiter = MemoryLimiter::new(ByteSize::new(u64::try_from(bytes).unwrap()));
        assert!(limiter.table_growing(0, elements, None).unwrap());
        assert!(!limiter.denied);
        assert!(!limiter.table_growing(elements, elements + 1, None).unwrap());
        assert!(limiter.denied);
    }

    #[test]
    fn a_huge_table_request_saturates_instead_of_wrapping() {
        let mut limiter = MemoryLimiter::new(ByteSize::new(u64::MAX));
        // usize::MAX elements would wrap to a small byte count without saturation; saturated, it is usize::MAX,
        // which a ceiling of u64::MAX (saturated to usize::MAX) still allows, and which any smaller ceiling denies.
        assert!(limiter.table_growing(0, usize::MAX, None).unwrap());
        let mut small = MemoryLimiter::new(ByteSize::new(1 << 20));
        assert!(!small.table_growing(0, usize::MAX, None).unwrap());
    }

    #[test]
    fn every_contract_enum_value_maps_to_its_own_value() {
        let pairs = [
            (types::Capability::Search, Capability::Search),
            (types::Capability::Metadata, Capability::Metadata),
            (types::Capability::Resolve, Capability::Resolve),
            (types::Capability::Radio, Capability::Radio),
            (types::Capability::Artwork, Capability::Artwork),
            (types::Capability::Lyrics, Capability::Lyrics),
            (types::Capability::LinkImport, Capability::LinkImport),
        ];
        for (wit, ours) in pairs {
            assert_eq!(capability(wit), ours);
        }
        assert_eq!(domain(types::Domain::Music), Domain::Music);
        assert_eq!(domain(types::Domain::Video), Domain::Video);
        assert_eq!(domain(types::Domain::Books), Domain::Books);
    }
}
