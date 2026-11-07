//! Kani proofs of the request checks (ADR 0014): the URL filter and the header check, for every byte sequence up to
//! the length each proof states. The property tests of `network.rs` cover longer strings.

use super::*;
use crate::manifest::are_host_name_labels;
use crate::proof_input::AnyBytes;

/// What the filter keeps: `https://`, then a host that is valid host name labels once lowercased, holds no `@`, `:`,
/// `/`, `?`, `#` or `\`, and ends at the first `/` or `?`; the part HTTP sends ends at the first `#`; every byte after
/// the scheme is visible ASCII and none is `\`. Anything without the scheme is refused as a scheme, anything else as a
/// host. URLs up to 20 bytes, so up to 12 after `https://`.
#[kani::proof]
#[kani::unwind(22)]
fn a_url_is_kept_only_with_a_clean_host() {
    let url = AnyBytes::<20>::new();
    let url = url.get();
    match UrlParts::of(url) {
        Ok(parts) => {
            assert!(url.starts_with(SCHEME));
            assert!(
                SCHEME.len() <= parts.host_end
                    && parts.host_end <= parts.end
                    && parts.end <= url.len()
            );
            let after = &url[SCHEME.len()..];
            assert!(after.iter().all(|b| b.is_ascii_graphic() && *b != b'\\'));
            let host = &url[SCHEME.len()..parts.host_end];
            assert!(host.iter().all(|b| !b"@:/?#\\".contains(b)));
            assert!(
                host.len() <= 253 && are_host_name_labels(host.iter().map(u8::to_ascii_lowercase))
            );
            assert!(parts.host_end == parts.end || matches!(url[parts.host_end], b'/' | b'?'));
            assert!(url[SCHEME.len()..parts.end].iter().all(|b| *b != b'#'));
            assert!(parts.end == url.len() || url[parts.end] == b'#');
        }
        Err(UrlRefusal::Scheme) => assert!(!url.starts_with(SCHEME)),
        Err(UrlRefusal::Host) => assert!(url.starts_with(SCHEME)),
    }
}

/// An accepted header has a non-empty name of token bytes (no `:`, space, CR, LF or NUL) that is not `host` in any
/// case, and a value without CR, LF, NUL, DEL or a C1 control; `host` in any case is refused as such. Names up to 6
/// bytes, values up to 4.
#[kani::proof]
#[kani::unwind(18)]
fn an_accepted_header_can_neither_inject_a_line_nor_name_a_host() {
    let name = AnyBytes::<6>::new();
    let name = name.get();
    let value = AnyBytes::<4>::new();
    let value = value.get();
    let refusal = header_refusal_bytes(name, value);
    if name.eq_ignore_ascii_case(b"host") {
        assert_eq!(refusal, Some(HeaderRefusal::Host));
    }
    if refusal.is_none() {
        assert!(!name.is_empty());
        assert!(!name.eq_ignore_ascii_case(b"host"));
        assert!(
            name.iter()
                .all(|b| b.is_ascii_graphic() && !b":()<>@,;\\\"/[]?={}".contains(b))
        );
        assert!(value.iter().all(|b| !matches!(b, b'\r' | b'\n' | 0 | 0x7f)));
        assert!(
            !value
                .windows(2)
                .any(|w| w[0] == 0xc2 && (0x80..=0x9f).contains(&w[1]))
        );
    }
}
