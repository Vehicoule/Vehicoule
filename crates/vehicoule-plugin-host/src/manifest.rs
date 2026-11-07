//! The manifest a plugin declares at install (docs/plugin-host-spec.md, section 4).

use std::collections::BTreeSet;

use thiserror::Error;

/// A content domain a plugin serves.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    /// Music.
    Music,
    /// Video.
    Video,
    /// Books.
    Books,
}

/// A service-dependent capability a plugin provides, one exported interface of the contract each.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Search and suggestions.
    Search,
    /// Album, artist and playlist pages.
    Metadata,
    /// Resolution of a track to a playable stream.
    Resolve,
    /// Radio continuation candidates.
    Radio,
    /// Artwork.
    Artwork,
    /// Lyrics.
    Lyrics,
    /// Import of a pasted link.
    LinkImport,
}

impl Capability {
    /// Every capability of contract 0.1.0.
    pub const ALL: [Self; 7] = [
        Self::Search,
        Self::Metadata,
        Self::Resolve,
        Self::Radio,
        Self::Artwork,
        Self::Lyrics,
        Self::LinkImport,
    ];
}

/// Why a manifest was rejected.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ManifestError {
    /// The id is empty or holds something other than lowercase ASCII letters, digits and `-`.
    #[error("invalid plugin id `{0}`: only lowercase ASCII letters, digits and `-` are allowed")]
    InvalidId(String),
    /// The name is empty or holds a control character.
    #[error("invalid plugin name: it must be non-empty and hold no control character")]
    InvalidName,
    /// The version is not a semver version.
    #[error("invalid plugin version `{0}`: not a semver version")]
    InvalidVersion(String),
    /// The plugin serves no domain.
    #[error("the plugin declares no domain")]
    NoDomain,
    /// A host pattern breaks the rules of section 4.
    #[error("invalid host pattern `{0}`")]
    InvalidHostPattern(String),
}

/// A plugin's id: lowercase ASCII letters, digits and `-`, never empty.
///
/// ```
/// use vehicoule_plugin_host::PluginId;
///
/// assert_eq!(PluginId::parse("ytmusic").map(|id| id.as_str().to_owned()), Ok("ytmusic".to_owned()));
/// assert!(PluginId::parse("YT Music").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(String);

impl PluginId {
    /// Validates an id.
    ///
    /// # Errors
    ///
    /// [`ManifestError::InvalidId`] when the id is empty or holds another character.
    pub fn parse(id: &str) -> Result<Self, ManifestError> {
        let valid = !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if valid {
            Ok(Self(id.to_owned()))
        } else {
            Err(ManifestError::InvalidId(id.to_owned()))
        }
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A host a plugin may reach: an exact host name, or every subdomain of one (`*.example.org`, which does not match
/// `example.org` itself).
///
/// ```
/// use vehicoule_plugin_host::HostPattern;
///
/// let pattern = HostPattern::parse("*.example.org").unwrap();
/// assert!(pattern.matches("cdn.example.org"));
/// assert!(!pattern.matches("example.org"));
/// assert!(HostPattern::parse("10.0.0.1").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostPattern {
    /// The host name, lowercase, without the `*.` prefix.
    name: String,
    subdomains: bool,
}

impl HostPattern {
    /// Validates a pattern: a lowercase host name whose last label holds a letter (so no IP address), optionally
    /// prefixed by `*.`. No port, no bare `*`, no wildcard elsewhere.
    ///
    /// # Errors
    ///
    /// [`ManifestError::InvalidHostPattern`] when the pattern breaks any of these rules.
    pub fn parse(pattern: &str) -> Result<Self, ManifestError> {
        let (name, subdomains) = match pattern.strip_prefix("*.") {
            Some(rest) => (rest, true),
            None => (pattern, false),
        };
        if is_host_name(name) {
            Ok(Self {
                name: name.to_owned(),
                subdomains,
            })
        } else {
            Err(ManifestError::InvalidHostPattern(pattern.to_owned()))
        }
    }

    /// Whether `host` is allowed by this pattern, compared without regard to ASCII case.
    #[must_use]
    pub fn matches(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        if !self.subdomains {
            return host == self.name;
        }
        // At least one label and its dot before the name: `a.example.org`, never `.example.org`.
        host.strip_suffix(self.name.as_str())
            .and_then(|prefix| prefix.strip_suffix('.'))
            .is_some_and(is_host_name_prefix)
    }
}

/// A host name of section 4: labels of 1 to 63 lowercase ASCII letters, digits or `-`, not starting or ending with
/// `-`, joined by dots, at most 253 bytes, the last label holding a letter.
pub(crate) fn is_host_name(name: &str) -> bool {
    name.len() <= 253
        && name.split('.').all(is_label)
        && name.rsplit('.').next().is_some_and(has_letter)
}

/// The labels in front of a subdomain pattern's name, with the same rules as a host name but no letter required.
fn is_host_name_prefix(prefix: &str) -> bool {
    prefix.split('.').all(is_label)
}

fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn has_letter(label: &str) -> bool {
    label.bytes().any(|b| b.is_ascii_lowercase())
}

/// A validated manifest. The edge stores it at install and gives it back; the host enforces it, never a later
/// `describe`.
///
/// ```
/// use vehicoule_plugin_host::{Capability, Domain, Manifest};
///
/// let manifest = Manifest::new(
///     "ytmusic",
///     "YouTube Music",
///     "0.1.0",
///     [Domain::Music],
///     [Capability::Search, Capability::Resolve],
///     ["music.youtube.com", "*.googlevideo.com"],
/// )
/// .unwrap();
/// assert!(manifest.allows_host("rr1.googlevideo.com"));
/// assert!(!manifest.allows_host("example.org"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    id: PluginId,
    name: String,
    version: String,
    domains: BTreeSet<Domain>,
    capabilities: BTreeSet<Capability>,
    http_hosts: Vec<HostPattern>,
}

impl Manifest {
    /// Validates a manifest.
    ///
    /// # Errors
    ///
    /// The first rule of section 4 the manifest breaks, as a [`ManifestError`].
    pub fn new<S: AsRef<str>>(
        id: &str,
        name: &str,
        version: &str,
        domains: impl IntoIterator<Item = Domain>,
        capabilities: impl IntoIterator<Item = Capability>,
        http_hosts: impl IntoIterator<Item = S>,
    ) -> Result<Self, ManifestError> {
        let id = PluginId::parse(id)?;
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(ManifestError::InvalidName);
        }
        if semver::Version::parse(version).is_err() {
            return Err(ManifestError::InvalidVersion(version.to_owned()));
        }
        let domains: BTreeSet<Domain> = domains.into_iter().collect();
        if domains.is_empty() {
            return Err(ManifestError::NoDomain);
        }
        let http_hosts = http_hosts
            .into_iter()
            .map(|pattern| HostPattern::parse(pattern.as_ref()))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id,
            name: name.to_owned(),
            version: version.to_owned(),
            domains,
            capabilities: capabilities.into_iter().collect(),
            http_hosts,
        })
    }

    /// The plugin's id.
    #[must_use]
    pub fn id(&self) -> &PluginId {
        &self.id
    }

    /// The name shown to the user.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The version shown to the user, a semver version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The domains the plugin serves, never empty.
    #[must_use]
    pub fn domains(&self) -> &BTreeSet<Domain> {
        &self.domains
    }

    /// The capabilities the plugin provides; the host checked that they are exactly its exports.
    #[must_use]
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.capabilities
    }

    /// The hosts the plugin may reach.
    #[must_use]
    pub fn http_hosts(&self) -> &[HostPattern] {
        &self.http_hosts
    }

    /// Whether one of the declared patterns allows `host`.
    #[must_use]
    pub fn allows_host(&self, host: &str) -> bool {
        self.http_hosts.iter().any(|pattern| pattern.matches(host))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn valid(id: &str, version: &str, hosts: &[&str]) -> Result<Manifest, ManifestError> {
        Manifest::new(
            id,
            "Name",
            version,
            [Domain::Music],
            [Capability::Search],
            hosts,
        )
    }

    #[test]
    fn a_valid_manifest_keeps_every_field() {
        let manifest = Manifest::new(
            "deezer-2",
            "Deezer",
            "1.2.3-beta.1",
            [Domain::Music, Domain::Music],
            [Capability::Search, Capability::Artwork],
            ["api.deezer.com"],
        )
        .unwrap();
        assert_eq!(manifest.id().as_str(), "deezer-2");
        assert_eq!(manifest.name(), "Deezer");
        assert_eq!(manifest.version(), "1.2.3-beta.1");
        assert_eq!(
            manifest.domains().iter().copied().collect::<Vec<_>>(),
            [Domain::Music]
        );
        assert_eq!(
            manifest.capabilities().iter().copied().collect::<Vec<_>>(),
            [Capability::Search, Capability::Artwork]
        );
        assert_eq!(
            manifest.http_hosts(),
            [HostPattern::parse("api.deezer.com").unwrap()]
        );
    }

    #[test]
    fn ids_outside_the_alphabet_are_rejected() {
        for id in ["", "Deezer", "dee zer", "dée", "a_b", "a.b", "a/b"] {
            assert_eq!(
                PluginId::parse(id),
                Err(ManifestError::InvalidId(id.to_owned())),
                "{id:?}"
            );
        }
        for id in ["a", "0", "-", "a-1", "lrclib"] {
            assert!(PluginId::parse(id).is_ok(), "{id:?}");
        }
    }

    #[test]
    fn names_must_be_present_and_printable() {
        for name in ["", "a\nb", "\u{7}"] {
            let result = Manifest::new("a", name, "1.0.0", [Domain::Music], [], ["a.org"]);
            assert_eq!(result, Err(ManifestError::InvalidName), "{name:?}");
        }
        assert!(Manifest::new("a", "Ok name", "1.0.0", [Domain::Music], [], ["a.org"]).is_ok());
    }

    #[test]
    fn versions_must_be_semver() {
        for version in ["", "1", "1.0", "v1.0.0", "01.0.0", "1.0.0.0"] {
            assert_eq!(
                valid("a", version, &[]),
                Err(ManifestError::InvalidVersion(version.to_owned())),
                "{version:?}"
            );
        }
        assert!(valid("a", "0.0.1", &[]).is_ok());
    }

    #[test]
    fn a_plugin_must_serve_a_domain() {
        let result = Manifest::new("a", "A", "1.0.0", [], [Capability::Search], ["a.org"]);
        assert_eq!(result, Err(ManifestError::NoDomain));
    }

    #[test]
    fn the_first_bad_host_pattern_is_reported() {
        assert_eq!(
            valid("a", "1.0.0", &["a.org", "*", "b.org"]),
            Err(ManifestError::InvalidHostPattern("*".to_owned()))
        );
    }

    #[test]
    fn forbidden_patterns_are_rejected() {
        for pattern in [
            "",
            "*",
            "*.",
            "**.a.org",
            "a.*.org",
            "*a.org",
            "a.org:443",
            "https://a.org",
            "a.org/",
            "A.org",
            "10.0.0.1",
            "[::1]",
            "a..org",
            ".a.org",
            "a.org.",
            "-a.org",
            "a-.org",
            "a.123",
            &format!("{}.org", "a".repeat(64)),
            &format!("{}org", "a.".repeat(126)),
        ] {
            assert_eq!(
                HostPattern::parse(pattern),
                Err(ManifestError::InvalidHostPattern(pattern.to_owned())),
                "{pattern:?}"
            );
        }
    }

    #[test]
    fn limits_are_inclusive() {
        let longest_label = format!("{}.org", "a".repeat(63));
        assert!(HostPattern::parse(&longest_label).is_ok());
        // 124 labels "a." plus "org": 248 + 3 = 251 bytes; one more label reaches 253.
        let long_name = format!("{}ab.org", "a.".repeat(124));
        assert_eq!(long_name.len(), 254);
        assert!(HostPattern::parse(&long_name).is_err());
        let longest_name = format!("{}b.org", "a.".repeat(124));
        assert_eq!(longest_name.len(), 253);
        assert!(HostPattern::parse(&longest_name).is_ok());
    }

    #[test]
    fn an_exact_pattern_matches_only_its_host() {
        let pattern = HostPattern::parse("api.example.org").unwrap();
        assert!(pattern.matches("api.example.org"));
        assert!(pattern.matches("API.Example.ORG"));
        assert!(!pattern.matches("x.api.example.org"));
        assert!(!pattern.matches("example.org"));
        assert!(!pattern.matches("api.example.org.evil.net"));
        assert!(!pattern.matches("evilapi.example.org"));
    }

    #[test]
    fn a_subdomain_pattern_matches_subdomains_only() {
        let pattern = HostPattern::parse("*.example.org").unwrap();
        assert!(pattern.matches("a.example.org"));
        assert!(pattern.matches("a.b.example.org"));
        assert!(!pattern.matches("example.org"));
        assert!(!pattern.matches(".example.org"));
        assert!(!pattern.matches("evilexample.org"));
        assert!(!pattern.matches("a..example.org"));
        assert!(!pattern.matches("a.example.org.evil.net"));
    }

    #[test]
    fn allows_host_checks_every_pattern() {
        let manifest = valid("a", "1.0.0", &["a.org", "*.b.org"]).unwrap();
        assert!(manifest.allows_host("a.org"));
        assert!(manifest.allows_host("x.b.org"));
        assert!(!manifest.allows_host("b.org"));
        assert!(!valid("a", "1.0.0", &[]).unwrap().allows_host("a.org"));
    }

    fn label() -> impl Strategy<Value = String> {
        "[a-z0-9]([a-z0-9-]{0,10}[a-z0-9])?"
    }

    fn host_name() -> impl Strategy<Value = String> {
        (proptest::collection::vec(label(), 0..4), "[a-z]{1,6}").prop_map(|(mut labels, last)| {
            labels.push(last);
            labels.join(".")
        })
    }

    proptest! {
        #[test]
        fn every_host_name_is_matched_by_its_exact_pattern(name in host_name()) {
            let pattern = HostPattern::parse(&name).unwrap();
            let subdomain = format!("x.{name}");
            prop_assert!(pattern.matches(&name));
            prop_assert!(!pattern.matches(&subdomain));
        }

        #[test]
        fn a_subdomain_pattern_never_matches_its_own_name_or_a_foreign_host(
            name in host_name(),
            sub in label(),
            other in host_name(),
        ) {
            let pattern = HostPattern::parse(&format!("*.{name}")).unwrap();
            let subdomain = format!("{sub}.{name}");
            let foreign = format!("{sub}.{other}");
            let suffix = format!(".{name}");
            prop_assert!(pattern.matches(&subdomain));
            prop_assert!(!pattern.matches(&name));
            prop_assert_eq!(pattern.matches(&foreign), foreign.ends_with(&suffix));
        }

        #[test]
        fn ipv4_addresses_are_never_patterns(a in 0u8.., b in 0u8.., c in 0u8.., d in 0u8..) {
            let address = format!("{a}.{b}.{c}.{d}");
            let wildcard = format!("*.{address}");
            prop_assert!(HostPattern::parse(&address).is_err());
            prop_assert!(HostPattern::parse(&wildcard).is_err());
        }

        #[test]
        fn a_port_or_a_path_is_never_a_pattern(name in host_name(), port in 0u16.., path in "[a-z]{0,4}") {
            let with_port = format!("{name}:{port}");
            let with_path = format!("{name}/{path}");
            prop_assert!(HostPattern::parse(&with_port).is_err());
            prop_assert!(HostPattern::parse(&with_path).is_err());
        }
    }
}
