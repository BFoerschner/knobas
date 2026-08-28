//! The source configuration (interfaces §4.2: `owners[]`/`repos[]` allowlist,
//! `username`) and the JSON Schema the Add-source form is generated from
//! (spec §3a). No secret ever appears here -- the token lives in the OS
//! keychain (spec §14, interfaces §3).

use knobas_source::SourceError;

/// Everything a Gitea source is configured with. Secrets excluded, always.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GiteaConfig {
    /// Only sync repositories under these owners. Empty = every repository the
    /// token can see.
    pub owners: Vec<String>,
    /// `owner/name` entries. When set, exactly these are synced and no
    /// repository listing is performed at all.
    pub repos: Vec<String>,
    /// The Gitea account this token belongs to. Filled in from
    /// `test_connection`'s `account` at add time; what `@me`-style filters
    /// match `sync.item.author` against (interfaces §2.4 `SearchFilters::mine`).
    pub username: Option<String>,
    /// How many commits per repository knobas mirrors per run. A bound on the
    /// corpus, not a page size: older history stays in Gitea, one click away
    /// through `SyncItem::web_url`.
    pub commits_per_repo: u32,
    /// The same bound for pull requests, newest-updated first.
    pub prs_per_repo: u32,
    /// Whether a pull request's comments are folded into `body_text` (one
    /// extra request per emitted pull request that actually has comments).
    pub include_pr_comments: bool,
    /// Per-instance request rate. Interfaces §4.1 default: 10 req/s.
    pub rate_limit_per_sec: u32,
}

impl Default for GiteaConfig {
    fn default() -> Self {
        Self {
            owners: Vec::new(),
            repos: Vec::new(),
            username: None,
            commits_per_repo: 100,
            prs_per_repo: 200,
            include_pr_comments: true,
            rate_limit_per_sec: 10,
        }
    }
}

impl GiteaConfig {
    /// Parse `source_config.config`, refusing anything the form could not have
    /// produced.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for a blob this adapter's schema does not
    /// describe, a `repos[]` entry that is not `owner/name`, or a rate limit
    /// of zero.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|e| SourceError::protocol(format!("gitea: invalid source config: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), SourceError> {
        for entry in &self.repos {
            let bad = || {
                SourceError::protocol(format!(
                    "gitea: repos entry {entry:?} must be \"owner/name\""
                ))
            };
            let (owner, name) = entry.split_once('/').ok_or_else(bad)?;
            if owner.is_empty() || name.is_empty() || name.contains('/') {
                return Err(bad());
            }
        }
        if self.rate_limit_per_sec == 0 {
            return Err(SourceError::protocol(
                "gitea: rate_limit_per_sec must be at least 1".to_owned(),
            ));
        }
        Ok(())
    }

    /// The burst the shared limiter is given: interfaces §4.1 pairs Gitea's
    /// 10 req/s with a burst of 20, so the burst tracks whatever rate the
    /// source is configured with rather than being a second knob to keep in
    /// step with the first.
    #[must_use]
    pub(crate) fn rate_burst(&self) -> u32 {
        self.rate_limit_per_sec.saturating_mul(2)
    }
}

/// The JSON Schema the Add-source form is generated from (spec §3a).
#[must_use]
pub fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "owners": {
                "type": "array", "items": { "type": "string" }, "default": [],
                "title": "Owners",
                "description": "Only sync repositories under these owners. Empty syncs every repository the token can see."
            },
            "repos": {
                "type": "array", "items": { "type": "string", "pattern": "^[^/]+/[^/]+$" }, "default": [],
                "title": "Repositories",
                "description": "owner/name. When set, only these are synced and no repository listing is done."
            },
            "username": {
                "type": ["string", "null"], "default": null,
                "title": "Username",
                "description": "Your Gitea account. Filled in by Test connection; used for @me filters."
            },
            "commits_per_repo": {
                "type": "integer", "minimum": 0, "maximum": 1000, "default": 100,
                "title": "Commits per repository",
                "description": "How much commit history to mirror per repository, newest first."
            },
            "prs_per_repo": {
                "type": "integer", "minimum": 0, "maximum": 1000, "default": 200,
                "title": "Pull requests per repository",
                "description": "How many pull requests to mirror per repository, most recently updated first."
            },
            "include_pr_comments": {
                "type": "boolean", "default": true,
                "title": "Search pull-request comments",
                "description": "Fold review discussion into the searchable text. One extra request per pull request that has comments."
            },
            "rate_limit_per_sec": {
                "type": "integer", "minimum": 1, "maximum": 100, "default": 10,
                "title": "Requests per second",
                "description": "Per-source request rate."
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// `source_config.config` defaults to `'{}'` (migration 0002), so an empty
    /// object has to mean "every repository the token can see, default budgets".
    #[test]
    fn an_empty_object_is_the_default_configuration() {
        let cfg = GiteaConfig::from_json(&serde_json::json!({})).unwrap();
        assert_eq!(cfg, GiteaConfig::default());
        assert!(cfg.owners.is_empty() && cfg.repos.is_empty());
        assert_eq!((cfg.commits_per_repo, cfg.prs_per_repo), (100, 200));
        assert!(cfg.include_pr_comments);
        assert_eq!(cfg.rate_limit_per_sec, 10);
        // §4.1 pairs Gitea's 10 req/s with a burst of 20.
        assert_eq!(cfg.rate_burst(), 20);
    }

    #[test]
    fn a_full_configuration_parses() {
        let cfg = GiteaConfig::from_json(&serde_json::json!({
            "owners": ["tidewater"],
            "repos": ["tidewater/payout-service"],
            "username": "mara",
            "commits_per_repo": 25,
            "prs_per_repo": 50,
            "include_pr_comments": false,
            "rate_limit_per_sec": 4
        }))
        .unwrap();
        assert_eq!(cfg.owners, vec!["tidewater".to_owned()]);
        assert_eq!(cfg.repos, vec!["tidewater/payout-service".to_owned()]);
        assert_eq!(cfg.username.as_deref(), Some("mara"));
        assert_eq!((cfg.commits_per_repo, cfg.prs_per_repo), (25, 50));
        assert!(!cfg.include_pr_comments);
        assert_eq!((cfg.rate_limit_per_sec, cfg.rate_burst()), (4, 8));
    }

    /// A typo in a hand-edited config must fail loudly rather than read back as
    /// a default the user did not choose.
    #[test]
    fn an_unknown_field_is_refused() {
        let err = GiteaConfig::from_json(&serde_json::json!({ "owner": "tidewater" })).unwrap_err();
        assert!(
            matches!(err, SourceError::Protocol { message: ref m, .. } if m.contains("owner")),
            "{err:?}"
        );
    }

    #[test]
    fn a_repository_entry_must_be_owner_slash_name() {
        for bad in ["payout-service", "tidewater/", "/payout-service", "a/b/c"] {
            let err = GiteaConfig::from_json(&serde_json::json!({ "repos": [bad] })).unwrap_err();
            assert!(
                matches!(err, SourceError::Protocol { .. }),
                "{bad:?} -> {err:?}"
            );
        }
        // And the shape it exists to accept still parses.
        GiteaConfig::from_json(&serde_json::json!({ "repos": ["tidewater/payout-service"] }))
            .expect("owner/name is the accepted form");
    }

    #[test]
    fn a_zero_rate_limit_is_refused() {
        let err =
            GiteaConfig::from_json(&serde_json::json!({ "rate_limit_per_sec": 0 })).unwrap_err();
        assert!(matches!(err, SourceError::Protocol { .. }), "{err:?}");
        assert_eq!(
            GiteaConfig::from_json(&serde_json::json!({ "rate_limit_per_sec": 1 }))
                .unwrap()
                .rate_limit_per_sec,
            1
        );
    }

    /// The Add-source form is *generated* from `config_schema` (spec §3a), so a
    /// property the parser does not know is a field the user fills in and
    /// knobas throws away -- and a field the parser knows but the schema omits
    /// is one the form never offers.
    #[test]
    fn the_form_schema_and_the_parser_describe_the_same_fields() {
        let schema = config_schema();
        let in_form: BTreeSet<&str> = schema["properties"]
            .as_object()
            .expect("config_schema has an object of properties")
            .keys()
            .map(String::as_str)
            .collect();
        let serialized = serde_json::to_value(GiteaConfig::default()).unwrap();
        let in_parser: BTreeSet<&str> = serialized
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(in_form, in_parser);
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }

    /// Every default the schema advertises has to be the default the parser
    /// actually applies: the form pre-fills from the schema, so a disagreement
    /// is a source configured with a value the user was shown and never got.
    #[test]
    fn the_schema_defaults_are_the_parsers_defaults() {
        let schema = config_schema();
        let serialized = serde_json::to_value(GiteaConfig::default()).unwrap();
        for (field, value) in serialized.as_object().unwrap() {
            assert_eq!(
                &schema["properties"][field]["default"], value,
                "schema default for {field:?} disagrees with GiteaConfig::default()"
            );
        }
    }
}
