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
//! it is refused when any address the name resolves to is not
//! [public](is_public). A fixed entry that is an address or `localhost` says
//! where it goes in the install list, and a template's value is one the
//! reader typed, so neither is guarded.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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

/// Whether `ip` is a public address: one the IANA special-purpose address
/// registries (IPv4 and IPv6, as published on 2025-10-09) mark globally
/// reachable, or that no entry of theirs covers. Anything else may be the
/// reader's own machine or network, or is no address a public host
/// answers: loopback, unspecified, private, shared (`100.64/10`, which a
/// tailnet uses), link-local, unique-local, benchmarking, documentation,
/// the IETF's protocol blocks, reserved (`240/4`), multicast, and IPv6
/// outside global unicast (`2000::/3`). std's `IpAddr::is_global` is still
/// unstable, so the registries' tables are here.
///
/// An IPv6 address that carries an IPv4 one is that address (#1641):
/// IPv4-mapped (`::ffff:0:0/96`), NAT64's well-known prefix
/// (`64:ff9b::/96`), whose gateway connects to the address in its low 32
/// bits, and 6to4 (`2002::/16`). A NAT64 network's own prefix (RFC 6052
/// §2.2) is not recognisable from the address and counts as what it is.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match carried_v4(v6) {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

/// The IPv4 blocks the IPv4 special-purpose registry marks not globally
/// reachable, and multicast (`224/4`, RFC 5771). `192.0.0.0/24` stands for
/// every entry inside it but the two in [`PUBLIC_V4`].
const NOT_PUBLIC_V4: [(Ipv4Addr, u32); 14] = [
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 2), 32),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // Multicast, then reserved and the limited broadcast.
    (Ipv4Addr::new(224, 0, 0, 0), 3),
];

/// Globally reachable inside a block of [`NOT_PUBLIC_V4`]: the PCP and
/// TURN anycast addresses.
const PUBLIC_V4: [Ipv4Addr; 2] = [Ipv4Addr::new(192, 0, 0, 9), Ipv4Addr::new(192, 0, 0, 10)];

fn is_public_v4(ip: Ipv4Addr) -> bool {
    PUBLIC_V4.contains(&ip)
        || !NOT_PUBLIC_V4
            .iter()
            .any(|&(block, length)| within(bits_v4(ip), bits_v4(block), length))
}

/// Inside global unicast, the blocks the IPv6 special-purpose registry
/// marks not globally reachable. `2001::/23` stands for every entry
/// inside it but those in [`PUBLIC_V6`] (TEREDO's `2001::/32` among them,
/// which the registry leaves unmarked).
const NOT_PUBLIC_V6: [(Ipv6Addr, u32); 3] = [
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
];

/// Globally reachable inside `2001::/23`: PCP, TURN and DNS-SD SRP
/// anycast, AMT, AS112-v6, ORCHIDv2 and Drone Remote ID.
const PUBLIC_V6: [(Ipv6Addr, u32); 7] = [
    (Ipv6Addr::new(0x2001, 1, 0, 0, 0, 0, 0, 1), 128),
    (Ipv6Addr::new(0x2001, 1, 0, 0, 0, 0, 0, 2), 128),
    (Ipv6Addr::new(0x2001, 1, 0, 0, 0, 0, 0, 3), 128),
    (Ipv6Addr::new(0x2001, 3, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2001, 4, 0x112, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2001, 0x20, 0, 0, 0, 0, 0, 0), 28),
    (Ipv6Addr::new(0x2001, 0x30, 0, 0, 0, 0, 0, 0), 28),
];

/// Global unicast, the only IPv6 space the IPv6 Address Space registry
/// allocates for public hosts. Every special-purpose block outside it
/// (loopback, unspecified, `64:ff9b:1::/48`, `100::/64`, `5f00::/16`,
/// unique-local, link-local, site-local, multicast) is not public.
const GLOBAL_UNICAST: (Ipv6Addr, u32) = (Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0), 3);

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let n = u128::from(ip);
    let inside = |&(block, length): &(Ipv6Addr, u32)| within(n, u128::from(block), length);
    PUBLIC_V6.iter().any(inside) || (inside(&GLOBAL_UNICAST) && !NOT_PUBLIC_V6.iter().any(inside))
}

/// The IPv4 address `ip` carries: IPv4-mapped, NAT64's well-known prefix,
/// or 6to4.
fn carried_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let [a, b, c, d, e, f, g, h] = ip.segments();
    let v4 = |high: u16, low: u16| Some(Ipv4Addr::from((u32::from(high) << 16) | u32::from(low)));
    match [a, b, c, d, e, f] {
        [0, 0, 0, 0, 0, 0xffff] | [0x64, 0xff9b, 0, 0, 0, 0] => v4(g, h),
        [0x2002, ..] => v4(b, c),
        _ => None,
    }
}

/// Whether `n` is inside the block of `length` leading bits at `block`,
/// both read as 128 bits from the left.
fn within(n: u128, block: u128, length: u32) -> bool {
    let host = u128::MAX.checked_shr(length).unwrap_or(0);
    n & !host == block & !host
}

/// An IPv4 address in the leading 32 of 128 bits, for [`within`].
fn bits_v4(ip: Ipv4Addr) -> u128 {
    u128::from(u32::from(ip)) << 96
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

    /// The first and the last address of `block`, a registry's
    /// `address/length`.
    fn ends(block: &str) -> [IpAddr; 2] {
        let (address, length) = block.split_once('/').unwrap();
        let length: u32 = length.parse().unwrap();
        match address.parse::<IpAddr>().unwrap() {
            IpAddr::V4(ip) => {
                let host = u32::MAX.checked_shr(length).unwrap_or(0);
                let first = u32::from(ip) & !host;
                [first, first | host].map(|n| IpAddr::V4(n.into()))
            }
            IpAddr::V6(ip) => {
                let host = u128::MAX.checked_shr(length).unwrap_or(0);
                let first = u128::from(ip) & !host;
                [first, first | host].map(|n| IpAddr::V6(n.into()))
            }
        }
    }

    /// Every block the IANA special-purpose registries mark not globally
    /// reachable, as published on 2025-10-09:
    /// <https://www.iana.org/assignments/iana-ipv4-special-registry/> and
    /// <https://www.iana.org/assignments/iana-ipv6-special-registry/>; with
    /// multicast (`224.0.0.0/4`, RFC 5771; `ff00::/8`, RFC 4291) and the
    /// IPv6 space outside `2000::/3`, the only space the IPv6 Address Space
    /// registry allocates as global unicast. Both ends of each block.
    #[test]
    fn every_block_the_special_purpose_registries_mark_not_globally_reachable_is_refused() {
        for block in [
            // IPv4 special-purpose registry.
            "0.0.0.0/8",
            "0.0.0.0/32",
            "10.0.0.0/8",
            "100.64.0.0/10",
            "127.0.0.0/8",
            "169.254.0.0/16",
            "172.16.0.0/12",
            "192.0.0.0/29",
            "192.0.0.8/32",
            "192.0.0.11/32",
            "192.0.0.170/32",
            "192.0.0.171/32",
            "192.0.0.255/32",
            "192.0.2.0/24",
            "192.88.99.2/32",
            "192.168.0.0/16",
            "198.18.0.0/15",
            "198.51.100.0/24",
            "203.0.113.0/24",
            "240.0.0.0/4",
            "255.255.255.255/32",
            // IPv4 multicast.
            "224.0.0.0/4",
            // IPv6 special-purpose registry.
            "::1/128",
            "::/128",
            "64:ff9b:1::/48",
            "100::/64",
            "100:0:0:1::/64",
            "2001::/32",
            "2001:2::/48",
            "2001:10::/28",
            "2001:db8::/32",
            "3fff::/20",
            "5f00::/16",
            "fc00::/7",
            "fe80::/10",
            // IPv6 multicast, the retired site-local, and space outside
            // global unicast (IPv4-compatible `::a.b.c.d` among it).
            "ff00::/8",
            "fec0::/10",
            "::/96",
            "::/3",
            "4000::/2",
            "8000::/1",
        ] {
            for ip in ends(block) {
                assert!(!is_public(ip), "{block}: {ip}");
            }
        }
    }

    /// An address that carries an IPv4 one is that address: IPv4-mapped
    /// (`::ffff:0:0/96`), NAT64's well-known prefix (`64:ff9b::/96`, RFC
    /// 6052, the low 32 bits) and 6to4 (`2002::/16`, RFC 3056, bits 16 to
    /// 47). The registry marks the last two reachable only as far as the
    /// address inside is.
    #[test]
    fn an_ipv6_address_that_carries_an_ipv4_one_is_that_address() {
        for (carrier, inside) in [
            ("::ffff:10.0.0.1", "10.0.0.1"),
            ("::ffff:127.0.0.1", "127.0.0.1"),
            ("::ffff:169.254.169.254", "169.254.169.254"),
            ("::ffff:1.1.1.1", "1.1.1.1"),
            ("64:ff9b::a00:1", "10.0.0.1"),
            ("64:ff9b::7f00:1", "127.0.0.1"),
            ("64:ff9b::c0a8:101", "192.168.1.1"),
            ("64:ff9b::c612:1", "198.18.0.1"),
            ("64:ff9b::101:101", "1.1.1.1"),
            ("64:ff9b::808:808", "8.8.8.8"),
            ("2002:a00:1::1", "10.0.0.1"),
            ("2002:7f00:1::", "127.0.0.1"),
            ("2002:a9fe:a9fe:ffff::1", "169.254.169.254"),
            ("2002:c000:1::1", "192.0.0.1"),
            ("2002:101:101::1", "1.1.1.1"),
            ("2002:808:808:1::1", "8.8.8.8"),
        ] {
            let inside: IpAddr = inside.parse().unwrap();
            assert_eq!(
                is_public(carrier.parse().unwrap()),
                is_public(inside),
                "{carrier} carries {inside}"
            );
        }
        assert!(!is_public("64:ff9b::a00:1".parse().unwrap()));
        assert!(!is_public("2002:a00:1::1".parse().unwrap()));
        assert!(is_public("64:ff9b::101:101".parse().unwrap()));
        assert!(is_public("2002:101:101::1".parse().unwrap()));
    }

    /// The more specific entries the registries mark globally reachable
    /// inside a block they do not (`192.0.0.0/24`, `2001::/23`), and
    /// public addresses at the edge of each refused block.
    #[test]
    fn a_globally_reachable_address_is_public() {
        for public in [
            "192.0.0.9",
            "192.0.0.10",
            "2001:1::1",
            "2001:1::2",
            "2001:1::3",
            "2001:3::1",
            "2001:3:ffff:ffff:ffff:ffff:ffff:ffff",
            "2001:4:112::1",
            "2001:20::1",
            "2001:2f:ffff:ffff:ffff:ffff:ffff:ffff",
            "2001:30::1",
            "2620:4f:8000::1",
            "192.31.196.1",
            "192.52.193.1",
            "192.175.48.1",
            "192.88.99.1",
            "1.1.1.1",
            "8.8.8.8",
            "1.0.0.0",
            "9.255.255.255",
            "11.0.0.0",
            "100.63.255.255",
            "100.128.0.0",
            "126.255.255.255",
            "128.0.0.0",
            "169.253.255.255",
            "169.255.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "191.255.255.255",
            "192.0.1.0",
            "192.0.3.0",
            "192.167.255.255",
            "192.169.0.0",
            "198.17.255.255",
            "198.20.0.0",
            "198.51.99.255",
            "198.51.101.0",
            "203.0.112.255",
            "203.0.114.0",
            "223.255.255.255",
            "2000::",
            "2001:200::",
            "2001:db7:ffff:ffff:ffff:ffff:ffff:ffff",
            "2001:db9::",
            "2003::",
            "2606:4700::1111",
            "2a00:1450:4001::1",
            "3ffe:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
            "3fff:1000::",
        ] {
            assert!(is_public(public.parse().unwrap()), "{public}");
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
