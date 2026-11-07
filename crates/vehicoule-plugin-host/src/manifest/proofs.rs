//! Kani proofs of host names and host patterns (ADR 0014): a pattern lets through its own host names and no other,
//! for every byte sequence up to the length each proof states. The property tests of `manifest.rs` cover longer ones.

use super::*;
use crate::proof_input::AnyBytes;

/// A host name is labels of lowercase letters, digits and `-`, joined by single dots, none starting or ending with
/// `-`, the last holding a letter. Up to 16 bytes.
#[kani::proof]
#[kani::unwind(18)]
fn a_host_name_is_dot_separated_labels_ending_in_a_letter_label() {
    let name = AnyBytes::<16>::new();
    let name = name.get();
    if is_host_name_bytes(name) {
        assert!(!name.is_empty());
        assert!(
            name.iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'-')
        );
        assert!(!matches!(name.first(), Some(b'.' | b'-')));
        assert!(!matches!(name.last(), Some(b'.' | b'-')));
        assert!(
            !name
                .windows(2)
                .any(|w| matches!(w, [b'.', b'.' | b'-'] | [b'-', b'.']))
        );
        let last = name.rsplit(|b| *b == b'.').next().unwrap_or(&[]);
        assert!(last.iter().any(u8::is_ascii_lowercase));
    }
}

/// An exact pattern matches its own name, in any case, and nothing else. Names up to 6 bytes, hosts up to 6.
#[kani::proof]
#[kani::unwind(8)]
fn an_exact_pattern_matches_its_name_and_nothing_else() {
    let name = AnyBytes::<6>::new();
    let name = name.get();
    kani::assume(is_host_name_bytes(name));
    let host = AnyBytes::<6>::new();
    let host = host.get();
    assert_eq!(
        pattern_matches(name, false, host),
        host.eq_ignore_ascii_case(name)
    );
}

/// `*.d` matches only a host that ends, in any case, with a dot and `d`, with at least one label before; never `d`
/// itself; and it matches `a.d`. Names up to 4 bytes, hosts up to 8.
#[kani::proof]
#[kani::unwind(10)]
fn a_subdomain_pattern_matches_subdomains_only() {
    let name = AnyBytes::<4>::new();
    let name = name.get();
    kani::assume(is_host_name_bytes(name));
    let host = AnyBytes::<8>::new();
    let host = host.get();
    if pattern_matches(name, true, host) {
        assert!(host.len() >= name.len() + 2);
        let (front, back) = host.split_at(host.len() - name.len());
        assert!(back.eq_ignore_ascii_case(name));
        assert_eq!(front.last(), Some(&b'.'));
        assert!(front[0] != b'.' && front[0] != b'-');
        assert!(
            front
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'.' || *b == b'-')
        );
    }
    assert!(!pattern_matches(name, true, name));
    let mut sub = [b'a', b'.', 0, 0, 0, 0];
    sub[2..2 + name.len()].copy_from_slice(name);
    assert!(pattern_matches(name, true, &sub[..2 + name.len()]));
}
