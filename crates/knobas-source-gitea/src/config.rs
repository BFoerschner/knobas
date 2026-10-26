//! The source configuration (interfaces §4.2: `owners[]`/`repos[]` allowlist,
//! `username`) and the JSON Schema the Add-source form is generated from
//! (spec §3a). No secret ever appears here -- the token lives in the OS
//! keychain (spec §14, interfaces §3).

use knobas_source::SourceError;

/// The most repositories one run walks at once (#2159): the top of the range
/// the Add-source form offers.
pub(crate) const MAX_CONCURRENT_REPOS: u32 = 32;

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
    /// Whether a pull request's comments are folded into `body_text`.
    ///
    /// One request per emitted pull request that actually has comments, and
    /// none for one that has none. One and not two, however long the
    /// discussion: Gitea's `issueGetComments` is not paged, so there is never a
    /// second page to ask for (issue #131, `client::issue_comments`).
    ///
    /// It is also the lever a source whose Gitea *does* truncate a discussion
    /// is left with, and the failure `sync::fetch_comments` raises names it.
    pub include_pr_comments: bool,
    /// Per-instance request rate. Interfaces §4.1 default: 10 req/s.
    pub rate_limit_per_sec: u32,
    /// How many repositories one run walks at once (#2159), 1 to
    /// [`MAX_CONCURRENT_REPOS`]. Inside a repository the walk stays one
    /// request after another; this only lets repositories overlap, and the
    /// shared limiter `rate_limit_per_sec` sets stays the one bound on the
    /// request rate. 1 walks them one after another, for an instance that
    /// cannot take more than one request at a time.
    pub concurrent_repos: u32,
    /// Other addresses CI clones this server from (#683): a URL prefix such
    /// as `http://gitea:3000`, or an scp-like remote such as `git@gitea:`.
    /// The SPI's [`knobas_source::clone_urls`] key, declared in
    /// [`config_schema`]: this adapter never reads it, and the app resolves
    /// with it a commit another source names by repository and sha (a
    /// TeamCity change) to the commit this source mirrors.
    pub clone_urls: Vec<String>,
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
            concurrent_repos: 8,
            clone_urls: Vec::new(),
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
    /// of zero, or a `concurrent_repos` outside 1 to [`MAX_CONCURRENT_REPOS`].
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
        for entry in &self.clone_urls {
            if !knobas_source::clone_urls::is_entry(entry) {
                return Err(SourceError::protocol(format!(
                    "gitea: clone_urls entry {entry:?} must be a URL such as \"http://gitea:3000\" \
                     or an scp-like remote such as \"git@gitea:\""
                )));
            }
        }
        if self.rate_limit_per_sec == 0 {
            return Err(SourceError::protocol(
                "gitea: rate_limit_per_sec must be at least 1".to_owned(),
            ));
        }
        if !(1..=MAX_CONCURRENT_REPOS).contains(&self.concurrent_repos) {
            return Err(SourceError::protocol(format!(
                "gitea: concurrent_repos must be 1 to {MAX_CONCURRENT_REPOS}"
            )));
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
            },
            "concurrent_repos": {
                "type": "integer", "minimum": 1, "maximum": MAX_CONCURRENT_REPOS, "default": 8,
                "title": "Repositories at once",
                "description": "How many repositories a sync walks at the same time. Set 1 for an instance that struggles with parallel requests."
            },
            "clone_urls": {
                "type": "array", "items": { "type": "string" }, "default": [],
                "title": "Clone addresses",
                "description": "Other addresses CI clones this server from, e.g. http://gitea:3000 or git@gitea:. A build's commits named by one of them open the commit knobas mirrors."
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

    /// #683: the schema declares the SPI's clone-address key, so the app
    /// reads this source's `clone_urls` -- the field the config parses.
    #[test]
    fn the_schema_declares_its_clone_urls() {
        assert!(knobas_source::clone_urls::declared(&config_schema()));
        let cfg = GiteaConfig::from_json(&serde_json::json!({
            knobas_source::clone_urls::CONFIG_KEY: ["git@gitea:"]
        }))
        .unwrap();
        assert_eq!(cfg.clone_urls, ["git@gitea:"]);
    }

    /// #683: the other addresses CI clones from, as a URL or an scp-like
    /// remote; anything else is refused at save time, not missed later.
    #[test]
    fn a_clone_url_is_a_url_or_an_scp_like_remote() {
        let cfg = GiteaConfig::from_json(&serde_json::json!({
            "clone_urls": ["http://gitea:3000", "ssh://git@gitea:2222/", "git@gitea:"]
        }))
        .unwrap();
        assert_eq!(cfg.clone_urls.len(), 3);
        for bad in [
            "gitea",
            "",
            ":",
            "http://",
            " git@gitea:",
            "git@gitea:owner/",
            "http:",
        ] {
            let err =
                GiteaConfig::from_json(&serde_json::json!({ "clone_urls": [bad] })).unwrap_err();
            assert!(
                matches!(err, SourceError::Protocol { ref message, .. } if message.contains("clone_urls")),
                "{bad:?} -> {err:?}"
            );
        }
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

    /// #2159: how many repositories a run walks at once. 1 is today's walk,
    /// one repository after another, for an instance that cannot take more;
    /// 32 is the top of the range, and the schema offers the same range the
    /// parser accepts.
    #[test]
    fn concurrent_repos_runs_from_one_to_thirty_two() {
        assert_eq!(GiteaConfig::default().concurrent_repos, 8);
        for good in [1, 8, 32] {
            let cfg =
                GiteaConfig::from_json(&serde_json::json!({ "concurrent_repos": good })).unwrap();
            assert_eq!(cfg.concurrent_repos, good);
        }
        for bad in [0, 33] {
            let err = GiteaConfig::from_json(&serde_json::json!({ "concurrent_repos": bad }))
                .unwrap_err();
            assert!(
                matches!(err, SourceError::Protocol { ref message, .. } if message.contains("concurrent_repos")),
                "{bad} -> {err:?}"
            );
        }
        let schema = &config_schema()["properties"]["concurrent_repos"];
        assert_eq!(
            (&schema["minimum"], &schema["maximum"]),
            (&serde_json::json!(1), &serde_json::json!(32))
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
