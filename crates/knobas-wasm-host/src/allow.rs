//! The hosts an external adapter may reach (ADR-0028, issue #1203).
//!
//! A manifest's `hosts` entry is one of two forms:
//!
//! * **a fixed host**, `api.example.com` or `api.example.com:8443`: that host,
//!   on the port given, or on the request scheme's default port (443 for
//!   `https`, 80 for `http`) when none is;
//! * **a template**, `{key}`, the whole entry: the value of `key` for each
//!   source, where `key` is `base_url` (the source's address) or a property
//!   the descriptor's `config_schema` declares. The value is a URL, whose
//!   host and port (stated or the scheme's default) are allowed, or a bare
//!   host as above.
//!
//! Nothing else is an entry: no scheme on a fixed host, no path, no
//! wildcard, no account (`user@`), no template inside a larger entry. A
//! template that resolves to nothing usable (an empty or missing value, one
//! that is not a string or not a host) allows nothing.
//!
//! **Matching is on the resolved host, never on text.** Both the entry and
//! the request's authority go through the URL parser, which lower-cases a
//! name, encodes an international one and reads every IPv4 spelling as its
//! address, and then compare equal or not at all: `api.example.com` does
//! not allow `api.example.com.evil.test` or `evil-api.example.com`.
//!
//! **A fixed name may not lead home** (#1244). The transport resolves a
//! name when the request leaves, so the owner of `api.attacker.example` can
//! point it at the reader's own machine or network (DNS rebinding). A
//! request that only a fixed host *name* allows is [guarded](Allowlist::guards):
//! it is refused when any address the name resolves to [is
//! local](is_local). A fixed entry that is an address or `localhost` says
//! where it goes in the install list, and a template's value is one the
//! reader typed, so neither is guarded.

use std::net::{IpAddr, Ipv4Addr};

use url::Host;

/// The template key that names the source's address.
pub const BASE_URL_KEY: &str = "base_url";

/// One `hosts` entry, as a manifest writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPattern {
    /// A fixed host, with its port when the entry names one.
    Fixed(Allowed),
    /// `{key}`: the value of a config key, or of `base_url`.
    Template(String),
}

/// One host a request may go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowed {
    /// `None` allows `http` and `https`; a URL value allows its own only.
    scheme: Option<String>,
    host: Host<String>,
    /// `None` is the request scheme's default port.
    port: Option<u16>,
}

impl HostPattern {
    /// Read one `hosts` entry.
    ///
    /// # Errors
    ///
    /// Why the entry is not one of the two forms.
    pub fn parse(entry: &str) -> Result<Self, String> {
        if let Some(key) = entry.strip_prefix('{').and_then(|e| e.strip_suffix('}')) {
            let ok = !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            return if ok {
                Ok(Self::Template(key.to_owned()))
            } else {
                Err(format!(
                    "host template {entry:?} does not name one config key"
                ))
            };
        }
        if entry.contains(['{', '}']) {
            return Err(format!(
                "host {entry:?}: a template is a whole entry, `{{key}}`"
            ));
        }
        Allowed::host_and_port(entry).map(Self::Fixed)
    }

    /// The config key a template names, `None` for a fixed host.
    #[must_use]
    pub fn template_key(&self) -> Option<&str> {
        match self {
            Self::Fixed(_) => None,
            Self::Template(key) => Some(key),
        }
    }
}

impl Allowed {
    /// A bare `host` or `host:port`.
    fn host_and_port(entry: &str) -> Result<Self, String> {
        if entry.is_empty() || entry.contains(['/', '@', '?', '#', '\\', ' ', '*']) {
            return Err(format!(
                "host {entry:?} must be a host name, with a port if it needs one"
            ));
        }
        // Through the URL parser, so the entry is normalised as a request is.
        let url = url::Url::parse(&format!("https://{entry}/"))
            .map_err(|e| format!("host {entry:?} is not a host: {e}"))?;
        let host = url
            .host()
            .ok_or_else(|| format!("host {entry:?} is not a host"))?
            .to_owned();
        // `Url::port` hides a scheme's default port, so read the entry under
        // two schemes: `:443` shows under `http`, `:80` under `https`.
        let port = url.port().or_else(|| {
            url::Url::parse(&format!("http://{entry}/"))
                .ok()
                .and_then(|u| u.port())
        });
        Ok(Self {
            scheme: None,
            host,
            port,
        })
    }

    /// What a template's value allows: a URL's host and port, or a bare host.
    fn from_value(value: &str) -> Option<Self> {
        if value.contains("://") {
            let url = url::Url::parse(value).ok()?;
            if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() {
                return None;
            }
            return Some(Self {
                scheme: Some(url.scheme().to_owned()),
                host: url.host()?.to_owned(),
                port: url.port_or_known_default(),
            });
        }
        Self::host_and_port(value).ok()
    }

    /// Whether a request to `host` on `port` over `scheme` is this one.
    fn matches(&self, target: &Target) -> bool {
        self.scheme.as_ref().is_none_or(|s| *s == target.scheme)
            && self.host == target.host
            && self.port.unwrap_or(target.default_port) == target.port
    }
}

/// Where one request goes, read from its scheme and authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub scheme: String,
    pub host: Host<String>,
    pub port: u16,
    default_port: u16,
}

impl Target {
    /// Read a request's destination. `None` for anything that is not plain
    /// `http(s)` to a host: an account in the authority is refused too.
    #[must_use]
    pub fn parse(scheme: &str, authority: &str) -> Option<Self> {
        if !matches!(scheme, "http" | "https") || authority.contains(['@', '/', '\\']) {
            return None;
        }
        let url = url::Url::parse(&format!("{scheme}://{authority}/")).ok()?;
        Some(Self {
            scheme: scheme.to_owned(),
            host: url.host()?.to_owned(),
            port: url.port_or_known_default()?,
            default_port: if scheme == "https" { 443 } else { 80 },
        })
    }

    /// The origin of a source's address, `None` when it is not an
    /// `http(s)` URL with a host.
    #[must_use]
    pub fn origin_of(base_url: &str) -> Option<Self> {
        let url = url::Url::parse(base_url).ok()?;
        if !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let authority = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str()?),
            None => url.host_str()?.to_owned(),
        };
        Self::parse(url.scheme(), &authority)
    }
}

/// The hosts one source of an adapter may reach: its manifest's entries,
/// each template resolved over that source's address and config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist {
    /// The manifest's fixed entries.
    fixed: Vec<Allowed>,
    /// Its templates' values: addresses the reader typed.
    from_templates: Vec<Allowed>,
}

impl Allowlist {
    /// Resolve `patterns` for one source. A template resolves only over
    /// `base_url` and the keys in `declared`; any other key allows nothing.
    #[must_use]
    pub fn resolve(
        patterns: &[HostPattern],
        base_url: &str,
        config: &serde_json::Value,
        declared: &[String],
    ) -> Self {
        let mut list = Self::default();
        for pattern in patterns {
            match pattern {
                HostPattern::Fixed(allowed) => list.fixed.push(allowed.clone()),
                HostPattern::Template(key) if !resolvable(key, declared) => {}
                HostPattern::Template(key) => {
                    let value = if key == BASE_URL_KEY {
                        Some(base_url)
                    } else {
                        config.get(key).and_then(serde_json::Value::as_str)
                    };
                    list.from_templates
                        .extend(value.and_then(Allowed::from_value));
                }
            }
        }
        list
    }

    /// Whether a request to `target` may leave.
    #[must_use]
    pub fn allows(&self, target: &Target) -> bool {
        self.fixed
            .iter()
            .chain(&self.from_templates)
            .any(|allowed| allowed.matches(target))
    }

    /// Whether a request to `target` must not reach a local address: only a
    /// fixed entry allows it, and that entry is a name other than
    /// `localhost`.
    #[must_use]
    pub fn guards(&self, target: &Target) -> bool {
        matches!(&target.host, Host::Domain(name) if name != "localhost")
            && !self
                .from_templates
                .iter()
                .any(|allowed| allowed.matches(target))
    }
}

/// Whether a request to `ip` stays on the reader's machine or network:
/// loopback, unspecified (`0.0.0.0`, `::`, which reach this machine),
/// private (`10/8`, `172.16/12`, `192.168/16`, the carrier-grade `100.64/10`
/// a tailnet uses), link-local (`169.254/16`, `fe80::/10`) or unique-local
/// (`fc00::/7`, and the retired site-local `fec0::/10`). An IPv6 address
/// that carries an IPv4 one (`::ffff:127.0.0.1`) is that address.
#[must_use]
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_local_v4(v4),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.to_ipv4().is_some_and(is_local_v4)
                || v6.is_loopback()
                || v6.is_unspecified()
                || first & 0xffc0 == 0xfe80
                || first & 0xffc0 == 0xfec0
                || first & 0xfe00 == 0xfc00
        }
    }
}

fn is_local_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 0
        || a == 127
        || a == 10
        || (a == 172 && b & 0xf0 == 16)
        || (a == 192 && b == 168)
        || (a == 169 && b == 254)
        || (a == 100 && b & 0xc0 == 64)
}

/// Whether a template over `key` may resolve: `base_url`, or a key in
/// `declared`.
#[must_use]
pub fn resolvable(key: &str, declared: &[String]) -> bool {
    key == BASE_URL_KEY || declared.iter().any(|d| d == key)
}

/// The property names a descriptor's `config_schema` declares.
#[must_use]
pub fn declared_keys(config_schema: &serde_json::Value) -> Vec<String> {
    config_schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[&str], base_url: &str, config: serde_json::Value) -> Allowlist {
        let patterns: Vec<HostPattern> = entries
            .iter()
            .map(|e| HostPattern::parse(e).unwrap())
            .collect();
        Allowlist::resolve(&patterns, base_url, &config, &["mirror".to_owned()])
    }

    fn allows(list: &Allowlist, url: &str) -> bool {
        let (scheme, rest) = url.split_once("://").unwrap();
        Target::parse(scheme, rest).is_some_and(|t| list.allows(&t))
    }

    #[test]
    fn a_fixed_host_allows_that_host_and_no_name_that_merely_starts_or_ends_with_it() {
        let list = list(&["api.example.com"], "", serde_json::json!({}));
        assert!(allows(&list, "https://api.example.com"));
        assert!(allows(&list, "https://API.Example.com:443"));
        assert!(allows(&list, "http://api.example.com"));
        assert!(!allows(&list, "https://api.example.com.evil.test"));
        assert!(!allows(&list, "https://evil-api.example.com"));
        assert!(!allows(&list, "https://x.api.example.com"));
        assert!(!allows(&list, "https://api.example.com:8443"));
        assert!(!allows(&list, "https://api.example.com@evil.test"));
    }

    #[test]
    fn an_address_is_compared_as_the_address_it_resolves_to() {
        let list = list(&["127.0.0.1:8080"], "", serde_json::json!({}));
        assert!(allows(&list, "http://127.0.0.1:8080"));
        assert!(allows(&list, "http://2130706433:8080"));
        assert!(allows(&list, "http://0x7f.0.0.1:8080"));
        assert!(!allows(&list, "http://127.0.0.1:8081"));
        assert!(!allows(&list, "http://127.0.0.2:8080"));
    }

    #[test]
    fn a_template_resolves_over_the_address_and_declared_keys_only() {
        let config = serde_json::json!({
            "mirror": "https://mirror.example:9443/x",
            "secret_place": "https://undeclared.example",
        });
        let list = list(
            &["{base_url}", "{mirror}", "{secret_place}"],
            "https://jira.example/prefix",
            config,
        );
        assert!(allows(&list, "https://jira.example"));
        assert!(
            !allows(&list, "http://jira.example"),
            "the address pins its port"
        );
        assert!(!allows(&list, "http://jira.example:443"), "and its scheme");
        assert!(allows(&list, "https://mirror.example:9443"));
        assert!(!allows(&list, "https://undeclared.example"));
    }

    #[test]
    fn a_template_whose_value_is_no_host_allows_nothing() {
        for value in [
            serde_json::json!(""),
            serde_json::json!(7),
            serde_json::json!("ftp://x.example"),
            serde_json::json!("https://user@x.example"),
            serde_json::json!("x.example/path"),
        ] {
            let list = list(&["{mirror}"], "", serde_json::json!({ "mirror": value }));
            assert_eq!(list, Allowlist::default(), "{value}");
        }
    }

    #[test]
    fn local_addresses_are_loopback_private_link_local_and_unique_local() {
        for local in [
            "127.0.0.1",
            "127.255.0.9",
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.254",
            "::1",
            "::",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
        ] {
            assert!(is_local(local.parse().unwrap()), "{local}");
        }
        for public in [
            "1.1.1.1",
            "203.0.113.7",
            "172.15.255.255",
            "172.32.0.1",
            "192.167.1.1",
            "169.253.1.1",
            "100.63.255.255",
            "100.128.0.1",
            "11.0.0.1",
            "126.255.255.255",
            "2606:4700::1111",
            "fbff::1",
            "fe00::1",
            "ff02::1",
            "::ffff:1.1.1.1",
        ] {
            assert!(!is_local(public.parse().unwrap()), "{public}");
        }
    }

    fn guards(list: &Allowlist, url: &str) -> bool {
        let (scheme, rest) = url.split_once("://").unwrap();
        list.guards(&Target::parse(scheme, rest).unwrap())
    }

    #[test]
    fn only_a_fixed_name_is_guarded() {
        let list = list(
            &[
                "api.example.com",
                "127.0.0.1:8080",
                "[::1]:8080",
                "localhost:9000",
                "{mirror}",
            ],
            "",
            serde_json::json!({ "mirror": "http://gitea.lan:3000" }),
        );
        assert!(guards(&list, "https://api.example.com"));
        assert!(!guards(&list, "http://127.0.0.1:8080"));
        assert!(!guards(&list, "http://[::1]:8080"));
        assert!(!guards(&list, "http://localhost:9000"));
        assert!(!guards(&list, "http://gitea.lan:3000"));
    }

    #[test]
    fn a_name_the_reader_typed_too_is_not_guarded() {
        let both = list(
            &["gitea.lan:3000", "{base_url}"],
            "http://gitea.lan:3000",
            serde_json::json!({}),
        );
        assert!(!guards(&both, "http://gitea.lan:3000"));
        let fixed_only = list(
            &["gitea.lan:3000"],
            "http://gitea.lan:3000",
            serde_json::json!({}),
        );
        assert!(guards(&fixed_only, "http://gitea.lan:3000"));
    }

    #[test]
    fn an_entry_is_a_host_or_a_whole_template() {
        for bad in [
            "https://api.example.com",
            "api.example.com/v1",
            "*.example.com",
            "{tenant}.example.com",
            "{}",
            "{a b}",
            "user@api.example.com",
            "",
        ] {
            assert!(HostPattern::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            HostPattern::parse("{base_url}"),
            Ok(HostPattern::Template("base_url".to_owned()))
        );
    }
}
