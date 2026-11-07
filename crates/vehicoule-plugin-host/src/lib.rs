//! Vehicoule's plugin host.
//!
//! Content comes only from plugins: Wasm components that implement the contract in `wit/plugin.wit`. This crate is
//! an edge of the app ([ADR 0003](https://github.com/Vehicoule/Vehicoule/blob/main/docs/adr/0003-core-architecture.md)):
//! the core never imports it. It runs each plugin in Wasmtime with no WASI, only the contract's imports, and budgets
//! ([ADR 0007](https://github.com/Vehicoule/Vehicoule/blob/main/docs/adr/0007-plugin-host.md)), and reports every
//! failure as a typed error for the core to act on. It never retries and never asks another plugin.
//!
//! It installs a plugin: size ceiling, compilation, imports, `describe` with every request refused, manifest rules,
//! and capabilities matched against exports
//! ([spec](https://github.com/Vehicoule/Vehicoule/blob/main/docs/plugin-host-spec.md), sections 3 to 5). It calls
//! the search capability in a fresh store per call, with fuel, memory and output budgets (sections 3, 6 and 7), and
//! gives plugins the network through a sandbox with its own budgets (sections 5 and 6). Each call returns a report of
//! what it used (section 7). The other capabilities come next.

mod bindings;
mod call;
mod config;
mod host;
mod logs;
mod manifest;
mod media;
mod network;
#[cfg(kani)]
mod proof_input;

pub use call::{CallReport, PluginError, PluginFailure, Reported};
pub use config::{ByteSize, Fuel, HostConfig, HttpBudget, LogBudget, Ticks};
pub use host::{HostError, InstallError, InstalledPlugin, PluginHost, Ticker};
pub use logs::{LogEntry, LogLevel};
pub use manifest::{Capability, Domain, HostPattern, Manifest, ManifestError, PluginId};
pub use media::{
    Album, AlbumRef, Artist, ArtistRef, Artwork, Item, ItemKind, Page, Playlist, Track,
};
pub use network::{
    HttpRequest, HttpResponse, HttpsUrl, Method, Network, NetworkError, RequestLimits,
};
