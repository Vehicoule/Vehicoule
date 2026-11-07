//! The plugins' network, through the host only (docs/plugin-host-spec.md, sections 5 and 6).
//!
//! The host checks every request a plugin makes: `https`, a host the manifest declares, the default port, no
//! credentials in the URL, no `Host` header, and the call's request budget. Only then does it hand the request to the
//! edge's [`Network`], which does the I/O. A refused request never reaches the network.

use std::time::Duration;

use thiserror::Error;

use crate::ByteSize;
use crate::manifest::is_host_name;

/// The network a plugin's checked requests go to, given by the edge to [`PluginHost::new`](crate::PluginHost::new).
///
/// The host has already checked the request (section 5). The implementation must:
///
/// - follow no redirect: a 3xx goes back as it is, and the plugin's next request is checked again;
/// - keep no cookie, cache or credential between requests on the plugin's behalf;
/// - stop reading a body larger than `limits.max_body` and return [`NetworkError::BodyTooLarge`];
/// - give up after `limits.deadline` and return [`NetworkError::Timeout`].
///
/// Tests give a fake that records every request and never opens a socket.
///
/// ```
/// use vehicoule_plugin_host::{HttpRequest, HttpResponse, Network, NetworkError, RequestLimits};
///
/// /// Answers every request with an empty 204.
/// struct Empty;
///
/// impl Network for Empty {
///     fn fetch(&self, _request: &HttpRequest, _limits: RequestLimits) -> Result<HttpResponse, NetworkError> {
///         Ok(HttpResponse::new(204, Vec::new(), Vec::new()))
///     }
/// }
/// ```
pub trait Network: Send + Sync {
    /// Sends `request` and returns the response, within `limits`.
    ///
    /// # Errors
    ///
    /// A [`NetworkError`] when the request timed out, the body was over its limit, or the transport failed.
    fn fetch(
        &self,
        request: &HttpRequest,
        limits: RequestLimits,
    ) -> Result<HttpResponse, NetworkError>;
}

/// An HTTP method a plugin may use.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`.
    Get,
    /// `HEAD`.
    Head,
    /// `POST`.
    Post,
}

/// An `https` URL on the default port, without credentials or fragment, whose host is a valid host name (section 4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HttpsUrl {
    url: String,
    host: String,
}

impl HttpsUrl {
    /// The whole URL, `https://` + host + path and query.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The host, lowercase.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Checks a URL a plugin asked for. Strict on purpose: only visible ASCII, no backslash (which some parsers read
    /// as `/`), no userinfo, no port, a host name and nothing else in the authority. The fragment is dropped, as HTTP
    /// never sends it.
    pub(crate) fn parse(url: &str) -> Result<Self, UrlRefusal> {
        let rest = url.strip_prefix("https://").ok_or(UrlRefusal::Scheme)?;
        if !rest.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') {
            return Err(UrlRefusal::Host);
        }
        let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
        let (authority, tail) = rest
            .find(['/', '?'])
            .and_then(|end| rest.split_at_checked(end))
            .unwrap_or((rest, ""));
        let host = authority.to_ascii_lowercase();
        if !is_host_name(&host) {
            // Userinfo (`@`) and ports (`:`) are not host name characters, so they land here too.
            return Err(UrlRefusal::Host);
        }
        let url = if tail.starts_with('/') {
            format!("https://{host}{tail}")
        } else {
            format!("https://{host}/{tail}")
        };
        Ok(Self { url, host })
    }
}

/// Why a URL was refused, as the contract's `http-error` tells the plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UrlRefusal {
    /// Not `https`.
    Scheme,
    /// Anything else: the host, a port, credentials, a character outside the strict set.
    Host,
}

/// A request that passed the host's checks.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    /// The method.
    pub method: Method,
    /// The URL, already checked.
    pub url: HttpsUrl,
    /// The plugin's headers, in its order; names and values checked, never `Host`.
    pub headers: Vec<(String, String)>,
    /// The body, when the plugin sent one.
    pub body: Option<Vec<u8>>,
}

/// A response, as the network got it: no redirect followed.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// The headers, in the server's order.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// A response from the network.
    #[must_use]
    pub fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }
}

/// The limits of one request, from the call's HTTP budget.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestLimits {
    /// The largest body the plugin may receive.
    pub max_body: ByteSize,
    /// How long the request may take.
    pub deadline: Duration,
}

impl RequestLimits {
    pub(crate) fn new(max_body: ByteSize, deadline: Duration) -> Self {
        Self { max_body, deadline }
    }
}

/// Why the network returned no response.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum NetworkError {
    /// The request took longer than its deadline.
    #[error("the request timed out")]
    Timeout,
    /// The body was larger than the limit; the network stopped reading it.
    #[error("the response body is over its limit")]
    BodyTooLarge,
    /// The connection or the protocol failed.
    #[error("transport failure: {0}")]
    Transport(String),
}

/// Whether a header a plugin sets may be sent: a token name, a value without control characters other than tab, and
/// never `Host`, which could reach another site behind the same address.
pub(crate) fn header_refusal(name: &str, value: &str) -> Option<HeaderRefusal> {
    if name.eq_ignore_ascii_case("host") {
        return Some(HeaderRefusal::Host);
    }
    let token = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    let name_ok = !name.is_empty() && name.bytes().all(token);
    let value_ok = value.chars().all(|c| c == '\t' || !c.is_control());
    (!name_ok || !value_ok).then_some(HeaderRefusal::Malformed)
}

/// Why a header was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderRefusal {
    /// A `Host` header.
    Host,
    /// A name or value HTTP does not allow.
    Malformed,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn parse(url: &str) -> Result<String, UrlRefusal> {
        HttpsUrl::parse(url).map(|u| u.as_str().to_owned())
    }

    #[test]
    fn an_https_url_on_a_host_name_is_kept_with_its_path_and_query() {
        assert_eq!(
            parse("https://api.example.org/v1/search?q=daft&limit=5"),
            Ok("https://api.example.org/v1/search?q=daft&limit=5".to_owned())
        );
        let url = HttpsUrl::parse("https://API.Example.org/x").unwrap();
        assert_eq!(url.host(), "api.example.org");
        assert_eq!(url.as_str(), "https://api.example.org/x");
    }

    #[test]
    fn an_empty_path_becomes_the_root() {
        assert_eq!(
            parse("https://api.example.org"),
            Ok("https://api.example.org/".to_owned())
        );
        assert_eq!(
            parse("https://api.example.org?q=1"),
            Ok("https://api.example.org/?q=1".to_owned())
        );
    }

    #[test]
    fn the_fragment_is_dropped() {
        assert_eq!(
            parse("https://api.example.org/a?b=c#frag"),
            Ok("https://api.example.org/a?b=c".to_owned())
        );
        // A fragment cannot hide an authority: what follows `#` is gone before the host is read.
        assert_eq!(
            parse("https://api.example.org#@evil.org/"),
            Ok("https://api.example.org/".to_owned())
        );
    }

    #[test]
    fn only_https_is_allowed() {
        for url in [
            "http://api.example.org/",
            "HTTPS://api.example.org/",
            "ftp://api.example.org/",
            "//api.example.org/",
            "api.example.org/",
            "",
        ] {
            assert_eq!(parse(url), Err(UrlRefusal::Scheme), "{url}");
        }
    }

    #[test]
    fn ip_addresses_ports_credentials_and_odd_characters_are_refused() {
        for url in [
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://api.example.org:443/",
            "https://api.example.org:8443/",
            "https://user@api.example.org/",
            "https://user:pass@api.example.org/",
            "https://evil.org\\@api.example.org/",
            "https://api.example.org\\.evil.org/",
            "https://api.example.org /",
            "https://api.example.org/a b",
            "https://api.example.org/\u{e9}",
            "https://api.example.org./",
            "https://api..example.org/",
            "https://-api.example.org/",
            "https://api%2eexample.org/",
            "https:///path",
            "https://",
        ] {
            assert_eq!(parse(url), Err(UrlRefusal::Host), "{url}");
        }
    }

    #[test]
    fn a_host_header_or_a_malformed_header_is_refused() {
        assert_eq!(header_refusal("Host", "x"), Some(HeaderRefusal::Host));
        assert_eq!(header_refusal("HOST", "x"), Some(HeaderRefusal::Host));
        assert_eq!(
            header_refusal("X-Bad\r\nInjected", "x"),
            Some(HeaderRefusal::Malformed)
        );
        assert_eq!(
            header_refusal("X-Ok", "a\r\nInjected: 1"),
            Some(HeaderRefusal::Malformed)
        );
        assert_eq!(
            header_refusal("X-Ok", "a\0"),
            Some(HeaderRefusal::Malformed)
        );
        assert_eq!(header_refusal("", "x"), Some(HeaderRefusal::Malformed));
        assert_eq!(header_refusal("X Ok", "x"), Some(HeaderRefusal::Malformed));
        assert_eq!(header_refusal("X-Ok", "a\tb c\u{e9}"), None);
        assert_eq!(header_refusal("Accept-Language", "fr-FR"), None);
    }

    proptest! {
        #[test]
        fn a_kept_url_is_https_on_its_valid_host_and_nothing_else(url in "https://\\PC{0,80}") {
            if let Ok(parsed) = HttpsUrl::parse(&url) {
                let host = parsed.host().to_owned();
                let full = parsed.as_str().to_owned();
                let prefix = format!("https://{host}/");
                let after_host = full.strip_prefix(&format!("https://{host}")).map(str::to_owned);
                prop_assert!(is_host_name(&host), "host {host:?} from {url:?}");
                prop_assert!(full.starts_with(&prefix), "{full:?} from {url:?}");
                prop_assert!(!full.contains('#') && !full.contains('\\'), "{full:?} from {url:?}");
                prop_assert_eq!(after_host.map(|rest| rest.starts_with('/')), Some(true));
            }
        }

        #[test]
        fn a_kept_url_parses_again_to_itself(path in "[a-z0-9/?=&%._~-]{0,40}") {
            let url = format!("https://api.example.org/{path}");
            let once = HttpsUrl::parse(&url).map(|u| u.as_str().to_owned());
            let twice = once.clone().and_then(|u| HttpsUrl::parse(&u).map(|u| u.as_str().to_owned()));
            prop_assert_eq!(once, twice);
        }
    }
}
