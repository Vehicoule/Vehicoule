//! Vehicoule's plugin host.
//!
//! Content comes only from plugins: Wasm components that implement the contract in `wit/plugin.wit`. This crate is
//! an edge of the app ([ADR 0003](https://github.com/Vehicoule/Vehicoule/blob/main/docs/adr/0003-core-architecture.md)):
//! the core never imports it. It runs each plugin in Wasmtime with no WASI, only the contract's imports, and budgets
//! ([ADR 0007](https://github.com/Vehicoule/Vehicoule/blob/main/docs/adr/0007-plugin-host.md)), and reports every
//! failure as a typed error for the core to act on. It never retries and never asks another plugin.
//!
//! This first part installs a plugin: size ceiling, compilation, imports, `describe` with every request refused,
//! manifest rules, and capabilities matched against exports
//! ([spec](https://github.com/Vehicoule/Vehicoule/blob/main/docs/plugin-host-spec.md), sections 3 to 5). Capability
//! calls come next.

mod bindings;
mod config;
mod host;
mod manifest;

pub use config::{ByteSize, Fuel, HostConfig};
pub use host::{HostError, InstallError, InstalledPlugin, PluginHost};
pub use manifest::{Capability, Domain, HostPattern, Manifest, ManifestError, PluginId};
