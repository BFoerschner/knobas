//! What the Add-source form fills in, and what the adapter is allowed to
//! believe about it.
//!
//! Nothing secret lives here (spec §14): the password or PAT is in the OS
//! keychain and reaches the adapter as `SourceInstance::secret`. `username` is
//! not a secret -- it is half of a Basic auth pair and is displayed in the
//! sources view.

use knobas_source::SourceError;

/// Which Jira product this instance is (roadmap §4 gotcha 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// Data Center / Server: REST v2, `/rest/api/2/search`, `startAt` paging.
    #[default]
    Datacenter,
    /// Cloud: REST v3, `/rest/api/3/search/jql`, `nextPageToken`, no `total`.
    /// Parsed and refused in M1 -- see [`JiraConfig::from_json`].
    Cloud,
}

/// One Jira source's configuration blob (`knobas.source_config.config`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct JiraConfig {
    pub flavor: Flavor,
    /// Project keys to sync. Empty means "everything the credential can see".
    /// Mutually exclusive with [`Self::jql_filter`].
    pub projects: Vec<String>,
    /// Any JQL, without an `ORDER BY`; the adapter appends its own.
    pub jql_filter: Option<String>,
    /// The Jira account this credential belongs to. Filled in from
    /// `test_connection`'s `account` by the Add-source dialog, and what
    /// `@me`-style filters match `sync.item.author` against; it is *also* the
    /// login name for
    /// [`AuthMethod::UserPassword`](knobas_source::AuthMethod), whose password
    /// is the keychain secret. A bearer token needs no username to
    /// authenticate, which is why this once said the field was unused for a
    /// PAT -- and why a PAT user was then told to fill it in anyway (#82).
    pub username: Option<String>,
    /// The instance's Epic Link custom field (`customfield_10008` on many DC
    /// instances). Naming it adds it to the requested `fields=` list, so epic
    /// membership survives in `payload` for M2's link work. Discovering the id
    /// automatically needs `GET /rest/api/2/field`, which is outside M1's
    /// endpoint set.
    ///
    /// Still needed after #32 widened `BASE_FIELDS` to include `parent`, and
    /// not made redundant by it: the two spellings belong to different kinds
    /// of project. A next-gen or a recent company-managed project puts epic
    /// membership in `fields.parent`, which every run now fetches; a
    /// **classic** Data Center project puts it in a custom field, which only
    /// this option can name because its id differs per instance.
    ///
    /// Certified end to end, not only in the query string (issue #125):
    /// `knobas-app/tests/adapter_to_mirror.rs`'s
    /// `a_classic_projects_epic_link_reaches_the_stored_payload` configures
    /// this option, syncs against mockd and asserts the epic key on the
    /// `sync.item.payload` column. Until then the round trip could not be run
    /// at all -- mockd served no `customfield_*` and answered 400.
    pub epic_link_field: Option<String>,
    /// `maxResults` per `/search` page (interfaces §4.1 default: 100).
    pub page_size: u32,
    /// Per-instance rate limit (interfaces §4.1 default: 5 req/s, burst 10).
    pub rate_per_sec: u32,
    pub rate_burst: u32,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            flavor: Flavor::Datacenter,
            projects: Vec::new(),
            jql_filter: None,
            username: None,
            epic_link_field: None,
            page_size: 100,
            rate_per_sec: 5,
            rate_burst: 10,
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
        }
    }
}

impl JiraConfig {
    /// Parse and validate `knobas.source_config.config` for a Jira instance.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] naming the one thing to fix: an unreadable
    /// blob, an unsupported dialect, or a value outside its range.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        let cfg: Self = serde_json::from_value(value.clone())
            .map_err(|e| SourceError::protocol(format!("jira configuration is not valid: {e}")))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), SourceError> {
        let bad = |m: String| Err(SourceError::protocol(m));
        if self.flavor == Flavor::Cloud {
            return bad(
                "jira flavor \"cloud\" is not implemented yet: Cloud speaks \
                 /rest/api/3/search/jql with nextPageToken paging and accountId users, which is \
                 a different dialect from Data Center's /rest/api/2/search. Set flavor to \
                 \"datacenter\"."
                    .to_owned(),
            );
        }
        if !self.projects.is_empty() && self.jql_filter.is_some() {
            return bad(
                "set either projects or jql_filter, not both -- the filter already decides the \
                 scope"
                    .to_owned(),
            );
        }
        for p in &self.projects {
            if !is_project_key(p) {
                return bad(format!(
                    "{p:?} is not a Jira project key (A-Z, 0-9 and _, starting with a letter, \
                     at most 32 characters)"
                ));
            }
        }
        if let Some(filter) = &self.jql_filter {
            if filter.trim().is_empty() {
                return bad("jql_filter is blank".to_owned());
            }
            if mentions_order_by(filter) {
                return bad(
                    "jql_filter must not carry an ORDER BY: the adapter appends \
                     ORDER BY updated ASC, which is what makes startAt paging safe"
                        .to_owned(),
                );
            }
        }
        if let Some(field) = &self.epic_link_field
            && (field.is_empty() || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return bad(format!(
                "{field:?} is not a bare Jira field id (letters, digits and _), and it is \
                 pasted into the fields= query parameter"
            ));
        }
        if self.page_size == 0 || self.page_size > 1000 {
            return bad(format!(
                "page_size {} is outside 1..=1000 (Jira DC clamps at its own \
                 jira.search.views.default.max anyway)",
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

/// Whether `filter` carries an `ORDER BY` **clause**.
///
/// Not a substring scan: `summary ~ "order by phone"` is a legitimate filter,
/// and refusing it would make a perfectly good search unconfigurable with a
/// message about ordering that the user cannot act on. Quoted literals are
/// stripped first -- text inside them is data, not syntax -- and the match is
/// then token-bounded, so `reorder by` is not an ordering either.
fn mentions_order_by(filter: &str) -> bool {
    let mut outside = String::with_capacity(filter.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in filter.chars() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == open {
                    quote = None;
                }
            }
            // JQL quotes strings with either " or '.
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None => outside.push(ch),
        }
    }
    let lowered = outside.to_ascii_lowercase();
    let tokens: Vec<&str> = lowered
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .collect();
    tokens.windows(2).any(|pair| pair == ["order", "by"])
}

/// Jira project keys are uppercase, start with a letter, and may carry digits
/// and underscores. Validating here is also what makes quoting them into JQL
/// safe: a key that passes this cannot contain a quote or a parenthesis.
fn is_project_key(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    s.len() <= 32 && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn err(value: serde_json::Value) -> String {
        JiraConfig::from_json(&value)
            .expect_err("must be rejected")
            .to_string()
    }

    /// The Add-source form can submit an empty object; the defaults are the
    /// Data Center dialect and the interfaces doc's tuning (§4.1: 5 req/s,
    /// burst 10, page size 100).
    #[test]
    fn an_empty_object_is_the_datacenter_default() {
        let cfg = JiraConfig::from_json(&json!({})).unwrap();
        assert_eq!(cfg.flavor, Flavor::Datacenter);
        assert_eq!(cfg.page_size, 100);
        assert_eq!((cfg.rate_per_sec, cfg.rate_burst), (5, 10));
        assert_eq!(
            (cfg.connect_timeout_secs, cfg.request_timeout_secs),
            (10, 30)
        );
        assert!(cfg.projects.is_empty());
        assert!(cfg.jql_filter.is_none());
    }

    /// Cloud is a *configured* flavor with no implementation behind it. It must
    /// fail at build time with a message that says which dialect is missing --
    /// not mid-sync against a 410 from an endpoint that no longer exists.
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
        assert!(err(json!({ "projekts": ["PAY"] })).contains("projekts"));
    }

    #[test]
    fn projects_and_a_filter_are_mutually_exclusive() {
        let msg = err(json!({ "projects": ["PAY"], "jql_filter": "labels = sepa" }));
        assert!(msg.contains("not both"), "{msg}");
    }

    /// The adapter appends `ORDER BY updated ASC`, which is what makes
    /// `startAt` paging safe. A filter carrying its own ordering would produce
    /// `… ORDER BY priority ORDER BY updated ASC` -- invalid JQL, and a 400 the
    /// user cannot explain.
    #[test]
    fn a_filter_with_its_own_ordering_is_refused() {
        for bad in [
            "project = PAY order by created DESC",
            "project = PAY ORDER BY created",
            "project = PAY OrDeR    By created",
            // The clause after a quoted literal is still a clause.
            "summary ~ \"order by phone\" order by created",
        ] {
            let msg = err(json!({ "jql_filter": bad }));
            assert!(msg.contains("ORDER BY"), "{bad:?}: {msg}");
        }
    }

    /// The refusal is about the ORDER BY *clause*, not the letters. A filter
    /// searching for the words is a legitimate filter, and rejecting it would
    /// be unfixable from the user's side -- the message would talk about
    /// ordering they never asked for.
    #[test]
    fn a_filter_merely_mentioning_the_words_is_accepted() {
        for good in [
            "summary ~ \"order by phone\"",
            "description ~ 'the order by which we ship'",
            "labels = reorder AND status != Done",
            "summary ~ \"order\" AND labels = by",
        ] {
            JiraConfig::from_json(&json!({ "jql_filter": good }))
                .unwrap_or_else(|e| panic!("{good:?} should be accepted: {e}"));
        }
    }

    #[test]
    fn project_keys_must_look_like_project_keys() {
        for bad in ["pay", "", "PAY-231", "PAY OPS", "\"; drop"] {
            let msg = err(json!({ "projects": [bad] }));
            assert!(msg.contains("project key"), "{bad:?}: {msg}");
        }
        JiraConfig::from_json(&json!({ "projects": ["PAY", "OPS", "TW_2"] })).unwrap();
    }

    /// The field id is pasted into the `fields=` query parameter, so it has to
    /// be a field id and not a second parameter.
    #[test]
    fn the_epic_link_field_must_be_a_bare_field_id() {
        assert!(err(json!({ "epic_link_field": "customfield_10008,*all" })).contains("field id"));
        JiraConfig::from_json(&json!({ "epic_link_field": "customfield_10008" })).unwrap();
    }

    #[test]
    fn out_of_range_tuning_is_refused() {
        assert!(err(json!({ "page_size": 0 })).contains("page_size"));
        assert!(err(json!({ "page_size": 5000 })).contains("page_size"));
        assert!(err(json!({ "rate_per_sec": 0 })).contains("rate_per_sec"));
    }

    /// User+password auth needs a username, and it is *not* a secret: it lives
    /// in the config blob in Postgres while the password lives in the keychain
    /// (interfaces doc §3).
    #[test]
    fn a_username_is_ordinary_configuration() {
        let cfg = JiraConfig::from_json(&json!({ "username": "mara.lindqvist" })).unwrap();
        assert_eq!(cfg.username.as_deref(), Some("mara.lindqvist"));
    }
}
