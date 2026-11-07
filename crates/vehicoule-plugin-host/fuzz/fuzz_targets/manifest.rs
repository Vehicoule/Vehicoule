//! Validates any manifest a plugin could declare: never a panic, and a manifest accepted once is accepted again from
//! its own fields, with the same result (docs/plugin-host-spec.md, section 4).

#![no_main]

use libfuzzer_sys::fuzz_target;
use vehicoule_plugin_host::{Capability, Domain, Manifest};

/// The domains and capabilities as bit sets, so the fuzzer reaches every combination, empty ones included.
const DOMAINS: [Domain; 3] = [Domain::Music, Domain::Video, Domain::Books];

fn pick<T: Copy>(all: &[T], bits: u8) -> Vec<T> {
    all.iter()
        .enumerate()
        .filter(|(i, _)| {
            bits.checked_shr(u32::try_from(*i).unwrap_or(u32::MAX))
                .unwrap_or(0)
                & 1
                == 1
        })
        .map(|(_, item)| *item)
        .collect()
}

fuzz_target!(|input: (&str, &str, &str, u8, u8, Vec<&str>, &str)| {
    let (id, name, version, domain_bits, capability_bits, hosts, host) = input;
    let domains = pick(&DOMAINS, domain_bits);
    let capabilities = pick(&Capability::ALL, capability_bits);
    let Ok(manifest) = Manifest::new(
        id,
        name,
        version,
        domains.iter().copied(),
        capabilities.iter().copied(),
        hosts.iter(),
    ) else {
        return;
    };
    assert_eq!(manifest.id().as_str(), id);
    assert_eq!(manifest.name(), name);
    assert_eq!(manifest.version(), version);
    assert!(
        !manifest.domains().is_empty(),
        "accepted a manifest with no domain"
    );
    assert_eq!(manifest.http_hosts().len(), hosts.len());
    assert!(
        !name.is_empty() && !name.chars().any(char::is_control),
        "accepted the name {name:?}"
    );

    let again = Manifest::new(
        manifest.id().as_str(),
        manifest.name(),
        manifest.version(),
        manifest.domains().iter().copied(),
        manifest.capabilities().iter().copied(),
        hosts.iter(),
    );
    assert_eq!(again.as_ref(), Ok(&manifest));
    assert_eq!(
        manifest.allows_host(host),
        manifest
            .http_hosts()
            .iter()
            .any(|pattern| pattern.matches(host))
    );
});
