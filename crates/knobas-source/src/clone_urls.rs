//! The other addresses a git host is cloned from (#683), declared by the
//! adapter and resolved here, so the app never learns which adapter has them.
//!
//! A commit another source names by repository URL and sha
//! ([`CommitHint`](crate::view::CommitHint), a TeamCity change) usually names
//! the repository by the address CI clones it from -- an internal hostname,
//! an SSH remote -- while the mirrored commit's page carries the host's own
//! base URL. A source that mirrors such a host lists those addresses in its
//! config under [`CONFIG_KEY`], and **declares that it does** by naming the
//! key in its descriptor's `config_schema` as an array of strings
//! ([`declared`]). The app reads the key only from sources whose adapter
//! declares it, and rewrites a hint with [`canonical_repo_url`] before the
//! paste resolver. Spelled once here, as [`crate::monitor_target`] is: the
//! alternative is the app knowing that the Gitea adapter keeps a field of
//! this name, which is the per-adapter table spec §3a forbids.
//!
//! Exact otherwise: an address nobody configured is matched as written, and
//! the same owner/repo on another host is another repository (#665).
//!
//! # Where a repository is cloned from (#1197)
//!
//! The other half of the module: the code-host read `clone_urls`
//! ([`role::CODE_HOST`](crate::role::CODE_HOST)'s minimum). *Clone* runs git
//! itself (ADR-0023), and ADR-0023 builds the URL from an address a person
//! typed, never from the mirror. So an adapter does not hand back a URL: it
//! declares, in [`SourceDescriptor::clone_from`](crate::SourceDescriptor::clone_from),
//! **which of its instance's typed addresses** a repository is cloned from --
//! the base URL, or a string its config holds under a key -- and the app
//! resolves them with [`addresses`] and appends the checked `owner/repo.git`.
//! Gitea declares [`CloneFrom::BaseUrl`], which is what the app built from
//! before the read existed.

/// The config key under which a source lists the other addresses its host
/// is cloned from: `["http://gitea:3000", "git@gitea:"]`.
pub const CONFIG_KEY: &str = "clone_urls";

/// Whether an adapter's `config_schema` declares [`CONFIG_KEY`], as an array
/// of strings. Anything else under the key -- another type, or no key -- is
/// not a declaration, and the contract battery refuses the former.
#[must_use]
pub fn declared(config_schema: &serde_json::Value) -> bool {
    config_schema
        .pointer(&format!("/properties/{CONFIG_KEY}"))
        .is_some_and(|p| p["type"] == "array" && p["items"]["type"] == "string")
}

/// Whether `entry` is an address [`canonical_repo_url`] can match: a URL
/// prefix (`http://gitea:3000`, `ssh://git@gitea:2222/`) or an scp-like
/// remote ending in `:` (`git@gitea:`), with no surrounding blanks. An
/// adapter validates its config's entries with this, so what it accepts at
/// save is what resolves later.
#[must_use]
pub fn is_entry(entry: &str) -> bool {
    if entry.trim() != entry {
        return false;
    }
    match entry.split_once("://") {
        Some((scheme, rest)) => !scheme.is_empty() && !rest.trim_matches('/').is_empty(),
        None => {
            let Some(host) = entry.strip_suffix(':') else {
                return false;
            };
            let bare = host.rsplit_once('@').map_or(host, |(_, h)| h);
            // A bare scheme (`http:`) is a URL cut short, not a host: as an
            // scp-like prefix it would match every `http://` hint.
            let scheme = !host.contains('@')
                && matches!(
                    host.to_ascii_lowercase().as_str(),
                    "http" | "https" | "ssh" | "git"
                );
            !bare.is_empty() && !host.contains('/') && !scheme
        }
    }
}

/// Which typed address of an instance a repository is cloned from: the
/// answer an adapter gives to the code-host read `clone_urls` (#1197).
///
/// Plain serde data, as everything crossing the SPI: `"base_url"`, or
/// `{"config": "<key>"}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum CloneFrom {
    /// The instance's base URL.
    BaseUrl,
    /// The string the instance's config holds under this key, which the
    /// adapter's `config_schema` declares as a string property
    /// ([`config_declares`]).
    Config(String),
}

/// Whether `config_schema` declares `key` as a string property: what a
/// [`CloneFrom::Config`] must name, so the address it reads is one the
/// Add-source form asked a person for.
#[must_use]
pub fn config_declares(config_schema: &serde_json::Value, key: &str) -> bool {
    config_schema
        .get("properties")
        .and_then(|p| p.get(key))
        .is_some_and(|p| p["type"] == "string")
}

/// The addresses `from` names on one instance, in the order declared: its
/// `base_url`, and the non-blank strings its `config` holds under the keys
/// named. A key the config does not hold as a string names nothing.
#[must_use]
pub fn addresses(from: &[CloneFrom], base_url: &str, config: &serde_json::Value) -> Vec<String> {
    from.iter()
        .filter_map(|from| match from {
            CloneFrom::BaseUrl => Some(base_url),
            CloneFrom::Config(key) => config.get(key).and_then(serde_json::Value::as_str),
        })
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(str::to_owned)
        .collect()
}

/// One source's other address: the base URL its pages carry, and one
/// [`CONFIG_KEY`] entry naming the same host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneAddress {
    pub base_url: String,
    pub address: String,
}

/// `repo_url` with a configured other address replaced by its source's base
/// URL; unchanged where no address is its prefix.
///
/// A URL address must be followed by `/`; an scp-like one by the path
/// directly. Either matches with or without its user, since the adapter that
/// drew the hint drops credentials (`git@gitea:` also matches
/// `gitea:owner/repo.git`). The host compares case-insensitively, as hosts
/// do; the path is kept as written. An entry [`is_entry`] refuses is skipped.
#[must_use]
pub fn canonical_repo_url(repo_url: &str, addresses: &[CloneAddress]) -> String {
    let repo = repo_url.trim();
    for CloneAddress { base_url, address } in addresses {
        let address = address.trim();
        if !is_entry(address) {
            continue;
        }
        let scp = !address.contains("://");
        let address = if scp {
            address
        } else {
            address.trim_end_matches('/')
        };
        for prefix in [address.to_owned(), without_user(address)] {
            let Some(head) = repo.get(..prefix.len()) else {
                continue;
            };
            if !head.eq_ignore_ascii_case(&prefix) {
                continue;
            }
            let rest = &repo[prefix.len()..];
            let path = if scp {
                rest
            } else if let Some(path) = rest.strip_prefix('/') {
                path
            } else {
                continue;
            };
            return format!("{}/{}", base_url.trim().trim_end_matches('/'), path);
        }
    }
    repo.to_owned()
}

/// An address without the `user@` before its host.
fn without_user(address: &str) -> String {
    let (scheme, rest) = match address.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest),
        None => (String::new(), address),
    };
    let host_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(host_end);
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{scheme}{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base_url: &str, address: &str) -> CloneAddress {
        CloneAddress {
            base_url: base_url.to_owned(),
            address: address.to_owned(),
        }
    }

    /// A configured other address of a source becomes that source's base URL
    /// -- as a URL prefix or an scp-like remote, with or without its user --
    /// and nothing else is rewritten.
    #[test]
    fn a_clone_url_becomes_its_sources_base_url_and_nothing_else_does() {
        let addresses = [
            at("http://127.0.0.1:3000/", "http://gitea:3000/"),
            at("http://127.0.0.1:3000", "git@gitea:"),
            at("https://git.example", "ssh://git@git.internal:2222"),
            // Refused by `is_entry`, so never matched: it would swallow
            // every http hint.
            at("https://swallowed.example", "http:"),
        ];
        for (hint, want) in [
            (
                "http://gitea:3000/tidewater/a.git",
                "http://127.0.0.1:3000/tidewater/a.git",
            ),
            (
                "HTTP://Gitea:3000/tidewater/a.git",
                "http://127.0.0.1:3000/tidewater/a.git",
            ),
            (
                "git@gitea:tidewater/a.git",
                "http://127.0.0.1:3000/tidewater/a.git",
            ),
            // The adapter dropped the user; the address still matches.
            (
                "gitea:tidewater/a.git",
                "http://127.0.0.1:3000/tidewater/a.git",
            ),
            (
                "ssh://git@git.internal:2222/o/r.git",
                "https://git.example/o/r.git",
            ),
            (
                "ssh://git.internal:2222/o/r.git",
                "https://git.example/o/r.git",
            ),
            // A longer host that merely starts like the address is not it.
            (
                "http://gitea:30000/tidewater/a.git",
                "http://gitea:30000/tidewater/a.git",
            ),
            (
                "http://gitea.other:3000/tidewater/a.git",
                "http://gitea.other:3000/tidewater/a.git",
            ),
            // Another host with the same owner/repo stays as written.
            (
                "http://elsewhere/tidewater/a.git",
                "http://elsewhere/tidewater/a.git",
            ),
        ] {
            assert_eq!(canonical_repo_url(hint, &addresses), want, "{hint}");
        }
        assert_eq!(
            canonical_repo_url("http://gitea:3000/x/y", &[]),
            "http://gitea:3000/x/y"
        );
    }

    /// The grammar an adapter validates with is the one the resolver matches
    /// by: a bare scheme ending in `:` is no scp-like host.
    #[test]
    fn an_entry_is_a_url_or_an_scp_like_remote() {
        for good in [
            "http://gitea:3000",
            "ssh://git@gitea:2222/",
            "git@gitea:",
            "gitea:",
            "git@ssh:",
        ] {
            assert!(is_entry(good), "{good:?}");
        }
        for bad in [
            "gitea",
            "",
            ":",
            "@:",
            "http://",
            "http:",
            "HTTPS:",
            " git@gitea:",
            "git@gitea:owner/",
        ] {
            assert!(!is_entry(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_schema_declares_the_key_as_an_array_of_strings() {
        let schema = |p: serde_json::Value| serde_json::json!({ "properties": { CONFIG_KEY: p } });
        assert!(declared(&schema(
            serde_json::json!({ "type": "array", "items": { "type": "string" } })
        )));
        assert!(!declared(&schema(serde_json::json!({ "type": "string" }))));
        assert!(!declared(&serde_json::json!({ "properties": {} })));
    }

    /// The addresses come from what a person typed, in the order the adapter
    /// declared them; a key the config does not fill names nothing.
    #[test]
    fn clone_addresses_are_the_instances_typed_addresses_in_declared_order() {
        let config =
            serde_json::json!({ "ssh_base": "ssh://git@forge.example", "port": 22, "blank": " " });
        let from = [
            CloneFrom::Config("ssh_base".into()),
            CloneFrom::BaseUrl,
            CloneFrom::Config("port".into()),
            CloneFrom::Config("blank".into()),
            CloneFrom::Config("absent".into()),
        ];
        assert_eq!(
            addresses(&from, "https://forge.example/", &config),
            ["ssh://git@forge.example", "https://forge.example/"]
        );
        assert!(addresses(&[], "https://forge.example", &config).is_empty());
    }

    #[test]
    fn a_clone_address_crosses_as_its_word_or_its_config_key() {
        let wire =
            serde_json::to_value([CloneFrom::BaseUrl, CloneFrom::Config("k".into())]).unwrap();
        assert_eq!(wire, serde_json::json!(["base_url", { "config": "k" }]));
        let schema = serde_json::json!({ "properties": { "k": { "type": "string" }, "n": { "type": "integer" } } });
        assert!(config_declares(&schema, "k"));
        assert!(!config_declares(&schema, "n"));
        assert!(!config_declares(&schema, "absent"));
    }
}
