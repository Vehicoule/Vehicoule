//! Install checks against the test plugins of `fixtures/` (docs/plugin-host-spec.md, section 9).
//!
//! The plugins are built by `scripts/build-plugin-fixtures.sh` into the directory named by
//! `VEHICOULE_PLUGIN_FIXTURES`. A missing directory fails the tests: they are never skipped.

mod common;

use common::{MAX_OUTPUT, MAX_PLUGIN_SIZE, fixture, host, host_on, host_with, ok_network};
use vehicoule_plugin_host::{ByteSize, Capability, Domain, InstallError, ManifestError};

#[test]
fn a_plugin_that_follows_the_contract_installs_with_its_manifest() {
    let plugin = host().unwrap().install(&fixture("good").unwrap()).unwrap();
    let manifest = plugin.manifest();
    assert_eq!(manifest.id().as_str(), "fixture");
    assert_eq!(manifest.name(), "good");
    assert_eq!(manifest.version(), "0.1.0");
    assert_eq!(
        manifest.domains().iter().copied().collect::<Vec<_>>(),
        [Domain::Music]
    );
    assert_eq!(
        manifest.capabilities().iter().copied().collect::<Vec<_>>(),
        [Capability::Search]
    );
    assert!(manifest.allows_host("api.example.org"));
    assert!(manifest.allows_host("a.cdn.example.org"));
    assert!(!manifest.allows_host("cdn.example.org"));
}

#[test]
fn a_plugin_exactly_at_the_size_ceiling_installs() {
    let bytes = fixture("good").unwrap();
    let size = u64::try_from(bytes.len()).unwrap();
    assert!(host_with(size, MAX_OUTPUT).unwrap().install(&bytes).is_ok());
}

#[test]
fn a_plugin_over_the_size_ceiling_never_reaches_the_compiler() {
    // Not Wasm at all: if the compiler ran first, the error would be `Compile`.
    let garbage = vec![0xff_u8; 1025];
    let error = host_with(1024, MAX_OUTPUT)
        .unwrap()
        .install(&garbage)
        .unwrap_err();
    assert!(
        matches!(error, InstallError::TooLarge { size, limit } if size == ByteSize::new(1025) && limit == ByteSize::new(1024)),
        "{error:?}"
    );
    // The same bytes under the ceiling do reach the compiler, which rejects them.
    let error = host_with(2048, MAX_OUTPUT)
        .unwrap()
        .install(&garbage)
        .unwrap_err();
    assert!(matches!(error, InstallError::Compile(_)), "{error:?}");
}

#[test]
fn a_plugin_importing_outside_the_contract_is_rejected() {
    let error = host()
        .unwrap()
        .install(&fixture("extra-import").unwrap())
        .unwrap_err();
    assert!(
        matches!(&error, InstallError::ForbiddenImport(name) if name == "fixture:plugin/outside"),
        "{error:?}"
    );
}

#[test]
fn an_endless_describe_is_stopped_by_fuel() {
    let error = host()
        .unwrap()
        .install(&fixture("endless-loop").unwrap())
        .unwrap_err();
    assert!(matches!(error, InstallError::FuelExhausted), "{error:?}");
}

#[test]
fn a_describe_that_grows_memory_without_bound_is_stopped_at_the_ceiling() {
    let error = host()
        .unwrap()
        .install(&fixture("memory-bomb").unwrap())
        .unwrap_err();
    assert!(matches!(error, InstallError::MemoryLimit), "{error:?}");
}

#[test]
fn a_trap_in_describe_rejects_the_plugin() {
    let error = host()
        .unwrap()
        .install(&fixture("trap").unwrap())
        .unwrap_err();
    assert!(matches!(error, InstallError::Trap(_)), "{error:?}");
}

#[test]
fn a_request_during_install_is_refused_and_the_plugin_sees_it() {
    // The plugin names itself after what its request returned.
    let plugin = host()
        .unwrap()
        .install(&fixture("fetch-in-describe").unwrap())
        .unwrap();
    assert_eq!(plugin.manifest().name(), "refused-host");
}

#[test]
fn a_declared_capability_that_is_not_exported_is_rejected() {
    let error = host()
        .unwrap()
        .install(&fixture("declares-missing-export").unwrap())
        .unwrap_err();
    assert!(
        matches!(
            error,
            InstallError::CapabilityMismatch {
                capability: Capability::Resolve,
                declared: true
            }
        ),
        "{error:?}"
    );
}

#[test]
fn an_exported_capability_that_is_not_declared_is_rejected() {
    let error = host()
        .unwrap()
        .install(&fixture("undeclared-export").unwrap())
        .unwrap_err();
    assert!(
        matches!(
            error,
            InstallError::CapabilityMismatch {
                capability: Capability::Search,
                declared: false
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_manifest_with_a_forbidden_host_pattern_is_rejected() {
    let error = host()
        .unwrap()
        .install(&fixture("bad-host-pattern").unwrap())
        .unwrap_err();
    assert!(
        matches!(&error, InstallError::InvalidManifest(ManifestError::InvalidHostPattern(p)) if p == "*"),
        "{error:?}"
    );
}

#[test]
fn installs_are_independent() {
    // A plugin that failed leaves nothing behind: the same host installs a good one right after.
    let host = host().unwrap();
    assert!(host.install(&fixture("endless-loop").unwrap()).is_err());
    assert!(host.install(&fixture("memory-bomb").unwrap()).is_err());
    assert!(host.install(&fixture("good").unwrap()).is_ok());
}

#[test]
fn memories_that_each_fit_but_together_exceed_the_ceiling_are_stopped() {
    // Two memories of 300 pages (18.75 MiB) each: either fits under the 32 MiB ceiling, both do not. The plugin's
    // `describe` traps, so a host that bounded each memory alone would report `Trap` instead.
    let error = host()
        .unwrap()
        .install(&fixture("multi-memory").unwrap())
        .unwrap_err();
    assert!(matches!(error, InstallError::MemoryLimit), "{error:?}");
}

#[test]
fn an_installed_plugin_debugs_as_its_manifest() {
    let plugin = host().unwrap().install(&fixture("good").unwrap()).unwrap();
    let debug = format!("{plugin:?}");
    assert!(
        debug.starts_with("InstalledPlugin { manifest: Manifest {"),
        "{debug}"
    );
    assert!(debug.contains("\"fixture\""), "{debug}");
}

#[test]
fn a_manifest_over_the_output_ceiling_is_rejected() {
    // A 70,000-byte name, plus `fixture`, `0.1.0`, the two host patterns (2 + 15 + 17) and one domain.
    let error = host()
        .unwrap()
        .install(&fixture("huge-manifest").unwrap())
        .unwrap_err();
    assert!(
        matches!(
            error,
            InstallError::OutputTooLarge { size, limit }
                if size == ByteSize::new(70_000 + 7 + 5 + 2 + 15 + 17 + 1) && limit == ByteSize::new(MAX_OUTPUT)
        ),
        "{error:?}"
    );
}

#[test]
fn a_manifest_exactly_at_the_output_ceiling_installs() {
    // `fixture` (7), `good` (4), `0.1.0` (5), the two host patterns (2 + 15 + 17), one domain and one capability: 52.
    let bytes = fixture("good").unwrap();
    assert!(
        host_with(MAX_PLUGIN_SIZE, 52)
            .unwrap()
            .install(&bytes)
            .is_ok()
    );
    let error = host_with(MAX_PLUGIN_SIZE, 51)
        .unwrap()
        .install(&bytes)
        .unwrap_err();
    assert!(
        matches!(error, InstallError::OutputTooLarge { size, limit } if size == ByteSize::new(52) && limit == ByteSize::new(51)),
        "{error:?}"
    );
}

#[test]
fn no_request_during_install_reaches_the_network() {
    let network = ok_network();
    let plugin = host_on(network.clone())
        .unwrap()
        .install(&fixture("fetch-in-describe").unwrap())
        .unwrap();
    assert_eq!(plugin.manifest().name(), "refused-host");
    assert!(network.requests().is_empty(), "{:?}", network.requests());
}
