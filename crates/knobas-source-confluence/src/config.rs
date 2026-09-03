//! What the Add-source form fills in, and what the adapter is allowed to
//! believe about it.
//!
//! Nothing secret lives here (spec §14): the password or PAT is in the OS
//! keychain and reaches the adapter as `SourceInstance::secret`. `username` is
//! not a secret -- it is half of a Basic auth pair and is displayed in the
//! sources view.

use knobas_source::SourceError;

/// The most results Confluence returns from `/rest/api/content/search` **when
/// a body is expanded**.
///
/// Asking for more is not an error and not honoured either: the server clamps
/// silently, so a page size of 200 would mean four times as many requests
/// reported as one. Enforced here so the number the user sees in the form is
/// the number the server will use.
pub(crate) const MAX_PAGE_SIZE: u32 = 50;

/// Which Confluence product this instance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// Data Center / Server: REST v1, `/rest/api/content/search`, CQL,
    /// `_links.next` paging.
    #[default]
    Datacenter,
    /// Cloud: `/wiki/api/v2/…`, cursor paging, `accountId` users. Parsed and
    /// refused -- see [`ConfluenceConfig::from_json`].
    Cloud,
}

/// One Confluence source's configuration blob (`knobas.source_config.config`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfluenceConfig {
    pub flavor: Flavor,
    /// Space keys to sync. **Empty means every space the account can see**,
    /// which is the criterion's own wording: an unconfigured source mirrors
    /// the whole wiki rather than nothing.
    pub spaces: Vec<String>,
    /// The Confluence account this credential belongs to.
    ///
    /// The cross-adapter identity convention (#82): filled in from
    /// `test_connection`'s `account` by the Add-source dialog, and what
    /// `@me`-style filters match `sync.item.author` against. It is *also* the
    /// login name for
    /// [`AuthMethod::UserPassword`](knobas_source::AuthMethod), whose password
    /// is the keychain secret -- a bearer token needs no username to
    /// authenticate, and that is precisely why #82 had to say the field is
    /// still wanted.
    pub username: Option<String>,
    /// Results per `/rest/api/content/search` page, at most [`MAX_PAGE_SIZE`].
    pub page_size: u32,
    /// Per-instance rate limit (contract §4.1 shape; Confluence is the same
    /// class of product as Jira, so the same 5 req/s burst 10).
    pub rate_per_sec: u32,
    pub rate_burst: u32,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

impl Default for ConfluenceConfig {
    fn default() -> Self {
        Self {
            flavor: Flavor::Datacenter,
            spaces: Vec::new(),
            username: None,
            page_size: MAX_PAGE_SIZE,
            rate_per_sec: 5,
            rate_burst: 10,
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
        }
    }
}

impl ConfluenceConfig {
    /// Parse and validate `knobas.source_config.config` for a Confluence
    /// instance.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] naming the one thing to fix: an unreadable
    /// blob, an unsupported dialect, or a value outside its range.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        let cfg: Self = serde_json::from_value(value.clone()).map_err(|e| {
            SourceError::protocol(format!("confluence configuration is not valid: {e}"))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), SourceError> {
        let bad = |m: String| Err(SourceError::protocol(m));
        if self.flavor == Flavor::Cloud {
            return bad(
                "confluence flavor \"cloud\" is not implemented yet: Cloud speaks \
                 /wiki/api/v2 with cursor paging and accountId users, which is a different \
                 dialect from Data Center's /rest/api/content/search. Set flavor to \
                 \"datacenter\"."
                    .to_owned(),
            );
        }
        for key in &self.spaces {
            if !is_space_key(key) {
                return bad(format!(
                    "{key:?} is not a Confluence space key (letters, digits, '_', '-' and '.', \
                     optionally starting with '~' for a personal space, at most 255 characters)"
                ));
            }
        }
        if self.page_size == 0 || self.page_size > MAX_PAGE_SIZE {
            return bad(format!(
                "page_size {} is outside 1..={MAX_PAGE_SIZE}: Confluence clamps a content \
                 search to {MAX_PAGE_SIZE} results once a body is expanded, and this adapter \
                 always expands one",
                self.page_size
            ));
        }
        if self.rate_per_sec == 0 {
            return bad("rate_per_sec must be at least 1".to_owned());
        }
        if self.rate_burst == 0 {
            return bad("rate_burst must be at least 1".to_owned());
        }
        if self.connect_timeout_secs == 0 || self.request_timeout_secs == 0 {
            return bad("timeouts must be at least 1 second".to_owned());
        }
        Ok(())
    }
}

/// Whether `s` may be pasted into a CQL `space in (…)` list.
///
/// Validating here is also what makes quoting them into CQL safe: a key that
/// passes this cannot contain a quote, a parenthesis, a comma or whitespace,
/// so [`crate::cql::build_cql`] never escapes anything and never has to.
///
/// The grammar is Confluence's own, widened only where the product is: a
/// global space key is alphanumeric, and a **personal** space key is the
/// owner's username behind a `~`, which may carry `.`, `-` and `_`.
fn is_space_key(s: &str) -> bool {
    let body = s.strip_prefix('~').unwrap_or(s);
    !body.is_empty()
        && s.len() <= 255
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn err(value: serde_json::Value) -> String {
        ConfluenceConfig::from_json(&value)
            .expect_err("must be rejected")
            .to_string()
    }

    /// The Add-source form can submit an empty object; the defaults are the
    /// Data Center dialect, every visible space, and the server's own cap.
    #[test]
    fn an_empty_object_is_the_datacenter_default() {
        let cfg = ConfluenceConfig::from_json(&json!({})).unwrap();
        assert_eq!(cfg.flavor, Flavor::Datacenter);
        assert_eq!(cfg.page_size, MAX_PAGE_SIZE);
        assert_eq!((cfg.rate_per_sec, cfg.rate_burst), (5, 10));
        assert_eq!(
            (cfg.connect_timeout_secs, cfg.request_timeout_secs),
            (10, 30)
        );
        assert!(
            cfg.spaces.is_empty(),
            "no space list means every space the account can see"
        );
        assert_eq!(cfg.username, None);
    }

    /// Cloud is a *configured* flavor with no implementation behind it. It
    /// must fail when the source is saved, with a message naming the dialect
    /// that is missing -- not mid-sync against a 404 from a path this product
    /// never had.
    #[test]
    fn the_cloud_flavor_is_refused_by_name() {
        let msg = err(json!({ "flavor": "cloud" }));
        assert!(msg.contains("cloud"), "{msg}");
        assert!(msg.contains("datacenter"), "{msg}");
    }

    /// The form is generated from `config_schema`, so a key that is not in the
    /// schema is a bug somewhere -- surfacing it beats reading it back as a
    /// silent `None`.
    #[test]
    fn an_unknown_key_is_refused() {
        assert!(err(json!({ "space_keys": ["ENG"] })).contains("space_keys"));
    }

    /// The keys are pasted into a CQL `space in (…)` list, so what this
    /// accepts is exactly what needs no escaping there.
    #[test]
    fn space_keys_must_look_like_space_keys() {
        for bad in [
            "",
            "~",
            "ENG SPACE",
            "\") or type=blogpost or space in (\"",
            "ENG,OPS",
            "ENG'",
        ] {
            let msg = err(json!({ "spaces": [bad] }));
            assert!(msg.contains("space key"), "{bad:?}: {msg}");
        }
        // Global spaces, a lowercase key, and a personal space.
        ConfluenceConfig::from_json(&json!({ "spaces": ["ENG", "ops", "TW2", "~mara.lindqvist"] }))
            .unwrap();
    }

    /// Confluence clamps a body-expanding content search to fifty results and
    /// says nothing about it, so a larger page size is not a bigger page --
    /// it is the same page with a wrong number beside it.
    #[test]
    fn a_page_size_past_the_servers_own_cap_is_refused() {
        assert!(err(json!({ "page_size": 0 })).contains("page_size"));
        let msg = err(json!({ "page_size": 200 }));
        assert!(msg.contains("page_size"), "{msg}");
        assert!(msg.contains("50"), "the message names the cap: {msg}");
        ConfluenceConfig::from_json(&json!({ "page_size": 50 })).unwrap();
    }

    #[test]
    fn out_of_range_tuning_is_refused() {
        assert!(err(json!({ "rate_per_sec": 0 })).contains("rate_per_sec"));
        assert!(err(json!({ "rate_burst": 0 })).contains("rate_burst"));
        assert!(err(json!({ "request_timeout_secs": 0 })).contains("timeouts"));
    }

    /// #82: `username` is the identity field on every adapter, and it is *not*
    /// a secret -- it lives in the config blob in Postgres while the password
    /// lives in the keychain (contract §3).
    #[test]
    fn a_username_is_ordinary_configuration() {
        let cfg = ConfluenceConfig::from_json(&json!({ "username": "mara.lindqvist" })).unwrap();
        assert_eq!(cfg.username.as_deref(), Some("mara.lindqvist"));
    }
}
