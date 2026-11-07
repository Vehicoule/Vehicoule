//! Parses any text as a host pattern and matches any host against it: never a panic, and a match only where the
//! rules of docs/plugin-host-spec.md, section 4, allow one.
//!
//! The patterns come from a plugin's manifest and the hosts from its requests, so both are untrusted; a pattern that
//! matched more than it says would open the network to hosts the user never agreed to.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vehicoule_plugin_host::HostPattern;

fuzz_target!(|input: (&str, &str)| {
    let (pattern, host) = input;
    let Ok(parsed) = HostPattern::parse(pattern) else {
        return;
    };
    // A valid pattern is lowercase ASCII: no port, no path, no other character.
    assert!(
        pattern.bytes().all(|b| b.is_ascii_lowercase()
            || b.is_ascii_digit()
            || b == b'-'
            || b == b'.'
            || b == b'*'),
        "accepted `{pattern}`"
    );
    let (name, subdomains) = match pattern.strip_prefix("*.") {
        Some(name) => (name, true),
        None => (pattern, false),
    };
    assert!(
        !name.contains('*'),
        "accepted a wildcard inside `{pattern}`"
    );
    // An exact pattern matches its own name, in any ASCII case; a subdomain pattern never matches the bare name.
    assert_eq!(
        parsed.matches(name),
        !subdomains,
        "`{pattern}` against its own name"
    );
    assert_eq!(
        parsed.matches(&name.to_ascii_uppercase()),
        !subdomains,
        "`{pattern}` against its own name in upper case"
    );

    if !parsed.matches(host) {
        return;
    }
    let host = host.to_ascii_lowercase();
    if subdomains {
        // At least one whole label in front of the name, joined by a dot.
        let prefix = host
            .strip_suffix(name)
            .and_then(|rest| rest.strip_suffix('.'));
        assert!(
            prefix.is_some_and(
                |prefix| !prefix.is_empty() && prefix.split('.').all(|label| !label.is_empty())
            ),
            "`{pattern}` matched `{host}`"
        );
    } else {
        assert_eq!(host, name, "`{pattern}` matched `{host}`");
    }
});
