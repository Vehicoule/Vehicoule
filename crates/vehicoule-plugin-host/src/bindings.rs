//! Host-side bindings of the contract, generated from `wit/plugin.wit` by Wasmtime's `bindgen!`.
//!
//! One module per world: `base` for what every plugin imports and exports, one per capability world so the host can
//! check, from the generated lookups, which capability interfaces a plugin exports (docs/plugin-host-spec.md,
//! section 8).
//!
//! A world that the host calls into maps the contract's imported interfaces to `base`'s with `with`, so one set of
//! Rust types stands for the contract's types whichever world a value comes from.

pub(crate) mod base {
    wasmtime::component::bindgen!({ path: "../../wit", world: "base" });
}

pub(crate) mod search {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "search-plugin",
        with: {
            "vehicoule:plugin/types": crate::bindings::base::vehicoule::plugin::types,
            "vehicoule:plugin/http": crate::bindings::base::vehicoule::plugin::http,
            "vehicoule:plugin/log": crate::bindings::base::vehicoule::plugin::log,
        },
    });
}

pub(crate) mod metadata {
    wasmtime::component::bindgen!({ path: "../../wit", world: "metadata-plugin" });
}

pub(crate) mod resolve {
    wasmtime::component::bindgen!({ path: "../../wit", world: "resolve-plugin" });
}

pub(crate) mod radio {
    wasmtime::component::bindgen!({ path: "../../wit", world: "radio-plugin" });
}

pub(crate) mod artwork {
    wasmtime::component::bindgen!({ path: "../../wit", world: "artwork-plugin" });
}

pub(crate) mod lyrics {
    wasmtime::component::bindgen!({ path: "../../wit", world: "lyrics-plugin" });
}

pub(crate) mod link_import {
    wasmtime::component::bindgen!({ path: "../../wit", world: "link-import-plugin" });
}
