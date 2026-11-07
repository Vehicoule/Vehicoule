//! Parses any text as a host pattern and matches any host against it: never a panic, and a match only where the
//! rules of docs/plugin-host-spec.md, section 4, allow one.
//!
//! The patterns come from a plugin's manifest and the hosts from its requests, so both are untrusted; a pattern that
//! matched more than it says would open the network to hosts the user never agreed to.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vehicoule_plugin_host::HostPattern;

/// The host names of section 4, written again here without the host's code so the two can disagree: labels of 1 to 63
/// lowercase ASCII letters, digits or `-`, not starting or ending with `-`, joined by single dots, at most 253 bytes in
/// all, the last label holding a letter (so no IP address). The rules are those `HostPattern::parse` documents.
fn is_host_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }
    let mut label_len = 0_usize;
    let mut previous = b'.';
    let mut last_label_has_letter = false;
    for b in name.bytes() {
        match b {
            b'.' => {
                if label_len == 0 || previous == b'-' {
                    return false;
                }
                label_len = 0;
                last_label_has_letter = false;
            }
            b'a'..=b'z' | b'0'..=b'9' | b'-' => {
                if label_len == 0 && b == b'-' {
                    return false;
                }
                label_len = label_len.saturating_add(1);
                if label_len > 63 {
                    return false;
                }
                last_label_has_letter |= b.is_ascii_lowercase();
            }
            _ => return false,
        }
        previous = b;
    }
    label_len > 0 && previous != b'-' && last_label_has_letter
}

fuzz_target!(|input: (&str, &str)| {
    let (pattern, host) = input;
    // A pattern is accepted exactly when it is a host name, optionally behind `*.`: no IP address, no port, no bare
    // `*`, no wildcard elsewhere.
    let valid = is_host_name(pattern.strip_prefix("*.").unwrap_or(pattern));
    let parsed = HostPattern::parse(pattern);
    assert_eq!(parsed.is_ok(), valid, "`{pattern}`");
    let Ok(parsed) = parsed else {
        return;
    };
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
