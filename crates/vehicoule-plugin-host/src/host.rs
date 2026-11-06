//! Installing a plugin: size, compilation, imports, `describe`, manifest, exports (docs/plugin-host-spec.md,
//! sections 3 to 5).

use thiserror::Error;
use wasmtime::component::{Component, HasSelf, InstancePre, Linker};
use wasmtime::{Engine, ResourceLimiter, Store, Trap};

use crate::bindings::base::vehicoule::plugin::{http, log, types};
use crate::bindings::{artwork, base, link_import, lyrics, metadata, radio, resolve, search};
use crate::{ByteSize, Capability, Domain, HostConfig, Manifest, ManifestError};

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
    /// The plugin asked for more memory than the ceiling.
    #[error("the plugin went over its memory ceiling")]
    MemoryLimit,
    /// The plugin trapped (crashed).
    #[error("the plugin trapped: {0}")]
    Trap(String),
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

/// A plugin that passed every install check.
#[derive(Debug)]
pub struct InstalledPlugin {
    manifest: Manifest,
    // Kept for the capability calls of the next step (docs/plugin-host-spec.md, section 11).
    _component: Component,
}

impl InstalledPlugin {
    /// The plugin's validated manifest, for the edge to store and show to the user.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// The state of one store: one call, then dropped (section 3).
struct CallState {
    limiter: MemoryLimiter,
}

impl types::Host for CallState {}

impl http::Host for CallState {
    /// During install every request is refused (section 3): the plugin has no grant yet.
    fn fetch(&mut self, _request: http::Request) -> Result<http::Response, http::HttpError> {
        Err(http::HttpError::RefusedHost)
    }
}

impl log::Host for CallState {
    /// Install-time logs have no sink yet; they are dropped, never shown.
    fn log(&mut self, _level: log::Level, _message: String) {}
}

/// Bounds linear memory and tables, and remembers that a growth was denied so the failure is reported as a memory
/// limit rather than as the trap the plugin raises when its allocator fails.
#[derive(Debug)]
pub(crate) struct MemoryLimiter {
    max_bytes: usize,
    denied: bool,
}

impl MemoryLimiter {
    pub(crate) fn new(max: ByteSize) -> Self {
        Self {
            max_bytes: max.saturating_usize(),
            denied: false,
        }
    }

    fn allow(&mut self, desired_bytes: usize) -> bool {
        let allowed = desired_bytes <= self.max_bytes;
        if !allowed {
            self.denied = true;
        }
        allowed
    }
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(self.allow(desired))
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Wasmtime stores one pointer per table element (`ResourceLimiter::table_growing`).
        Ok(self.allow(desired.saturating_mul(size_of::<usize>())))
    }
}

/// Runs plugins. One per app; installs and calls are independent of each other.
pub struct PluginHost {
    engine: Engine,
    linker: Linker<CallState>,
    config: HostConfig,
}

impl PluginHost {
    /// A host enforcing `config`.
    ///
    /// # Errors
    ///
    /// [`HostError::Engine`] when Wasmtime refuses the configuration.
    ///
    /// ```
    /// use vehicoule_plugin_host::{ByteSize, Fuel, HostConfig, PluginHost};
    ///
    /// let host = PluginHost::new(HostConfig::new(ByteSize::new(1 << 20), Fuel::new(1_000_000), ByteSize::new(16 << 20)));
    /// assert!(host.is_ok());
    /// ```
    pub fn new(config: HostConfig) -> Result<Self, HostError> {
        let mut wasm = wasmtime::Config::new();
        wasm.consume_fuel(true);
        let engine = Engine::new(&wasm).map_err(|e| HostError::Engine(e.to_string()))?;
        let mut linker = Linker::new(&engine);
        base::Base::add_to_linker::<CallState, HasSelf<CallState>>(&mut linker, |state| state)
            .map_err(|e| HostError::Engine(e.to_string()))?;
        Ok(Self {
            engine,
            linker,
            config,
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
        Ok(InstalledPlugin {
            manifest,
            _component: component,
        })
    }

    /// Calls `describe` in a fresh store, with every request refused and the call's budgets.
    fn describe(
        &self,
        pre: &InstancePre<CallState>,
    ) -> Result<base::exports::vehicoule::plugin::plugin::Manifest, InstallError> {
        let mut store = Store::new(
            &self.engine,
            CallState {
                limiter: MemoryLimiter::new(self.config.max_memory),
            },
        );
        store.limiter(|state| &mut state.limiter);
        store
            .set_fuel(self.config.fuel_per_call.get())
            .map_err(|e| InstallError::Runtime(e.to_string()))?;
        let plugin = base::BasePre::new(pre.clone())
            .and_then(|pre| pre.instantiate(&mut store))
            .map_err(|e| failure(&e, &store))?;
        plugin
            .vehicoule_plugin_plugin()
            .call_describe(&mut store)
            .map_err(|e| failure(&e, &store))
    }
}

/// Names a failure of instantiation or of a call, the memory limit first since a denied growth surfaces as a trap.
fn failure(error: &wasmtime::Error, store: &Store<CallState>) -> InstallError {
    if store.data().limiter.denied {
        return InstallError::MemoryLimit;
    }
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => InstallError::FuelExhausted,
        Some(trap) => InstallError::Trap(trap.to_string()),
        None => InstallError::Runtime(error.to_string()),
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
