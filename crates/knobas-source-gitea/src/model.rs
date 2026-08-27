//! Partial serde projections of the records this adapter reads.
//!
//! Deliberately partial, and deliberately **not** `deny_unknown_fields`: Gitea
//! adds fields between releases, and the raw record is kept verbatim in
//! `SyncItem::payload` (spec §3a "raw payload kept"), so a field this file does
//! not name is never lost -- it is simply not one the mapping reads. Field
//! names come from Gitea's own OpenAPI document (`Repository`, `Branch`,
//! `PayloadCommit`, `PullRequest`, `Commit`, `Comment`).

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct UserRef {
    #[serde(default)]
    pub login: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Repo {
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<UserRef>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired in task 7: incremental commits per moved branch"
        )
    )]
    pub default_branch: Option<String>,
    /// A repository with no commits at all. Gitea answers `/commits` on one of
    /// these with 409, which Task 7's walk uses this field to avoid asking for.
    #[serde(default)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired in task 7: incremental commits per moved branch"
        )
    )]
    pub empty: bool,
}

impl Repo {
    /// `full_name` split into its two halves, or `None` if the server sent
    /// something this adapter cannot address.
    pub(crate) fn owner_repo(&self) -> Option<(&str, &str)> {
        let (owner, name) = self.full_name.split_once('/')?;
        (!owner.is_empty() && !name.is_empty() && !name.contains('/')).then_some((owner, name))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Branch {
    pub name: String,
    #[serde(default)]
    pub commit: Option<PayloadCommit>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct PayloadCommit {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub author: Option<PayloadUser>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct PayloadUser {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired in task 6: pull requests and their discussion"
    )
)]
pub(crate) struct PullRequest {
    pub number: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub user: Option<UserRef>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// How many comments the discussion has; `0` saves a request.
    #[serde(default)]
    pub comments: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Comment {
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Commit {
    pub sha: String,
    #[serde(default)]
    pub created: Option<DateTime<Utc>>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub author: Option<UserRef>,
    #[serde(default)]
    pub commit: Option<RepoCommit>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RepoCommit {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub author: Option<CommitUser>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct CommitUser {
    #[serde(default)]
    pub name: Option<String>,
    /// Gitea types this as a plain string, not a `date-time`.
    #[serde(default)]
    pub date: Option<String>,
}

impl Commit {
    /// When this commit happened, preferring the list record's own field and
    /// falling back to the embedded author date.
    pub(crate) fn happened_at(&self) -> Option<DateTime<Utc>> {
        self.created.or_else(|| {
            let raw = self.commit.as_ref()?.author.as_ref()?.date.as_deref()?;
            DateTime::parse_from_rfc3339(raw)
                .ok()
                .map(|t| t.with_timezone(&Utc))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record whose `full_name` this adapter cannot split into a key is
    /// skipped rather than turned into an entity id nothing can address.
    #[test]
    fn only_an_addressable_full_name_yields_an_owner_and_a_repository() {
        let repo = |full_name: &str| Repo {
            full_name: full_name.to_owned(),
            description: None,
            owner: None,
            updated_at: None,
            html_url: None,
            default_branch: None,
            empty: false,
        };
        assert_eq!(
            repo("tidewater/payout-service").owner_repo(),
            Some(("tidewater", "payout-service"))
        );
        for bad in [
            "",
            "payout-service",
            "tidewater/",
            "/payout-service",
            "a/b/c",
        ] {
            assert_eq!(repo(bad).owner_repo(), None, "{bad:?}");
        }
    }

    /// The list record's own `created` wins; the embedded author date is the
    /// fallback for the endpoints that omit it. Both are real timestamps here,
    /// and they differ, so a mapping that read the wrong one is visible.
    #[test]
    fn a_commit_prefers_its_own_timestamp_over_the_authors() {
        let raw = serde_json::json!({
            "sha": "c90d11",
            "created": "2026-08-22T11:42:00Z",
            "commit": { "author": { "name": "Mara", "date": "2026-08-20T08:00:00Z" } }
        });
        let commit: Commit = serde_json::from_value(raw).unwrap();
        assert_eq!(
            commit.happened_at().map(|t| t.to_rfc3339()),
            Some("2026-08-22T11:42:00+00:00".to_owned())
        );

        let raw = serde_json::json!({
            "sha": "c90d11",
            "commit": { "author": { "name": "Mara", "date": "2026-08-20T08:00:00Z" } }
        });
        let commit: Commit = serde_json::from_value(raw).unwrap();
        assert_eq!(
            commit.happened_at().map(|t| t.to_rfc3339()),
            Some("2026-08-20T08:00:00+00:00".to_owned())
        );

        // Gitea types the author date as a plain string, so it may be anything.
        let raw =
            serde_json::json!({ "sha": "c90d11", "commit": { "author": { "date": "never" } } });
        let commit: Commit = serde_json::from_value(raw).unwrap();
        assert_eq!(commit.happened_at(), None);
    }

    /// The three fields nothing reads yet are still parsed, because the passes
    /// that will read them are what the shapes were chosen for: `empty` and
    /// `default_branch` steer Task 7's commit walk away from the 409 an empty
    /// repository answers, and `comments` is the count that saves Task 6 a
    /// request per pull request that has no discussion.
    #[test]
    fn the_fields_the_later_passes_steer_on_are_parsed_now() {
        let repo: Repo = serde_json::from_value(serde_json::json!({
            "full_name": "tidewater/fresh", "default_branch": "trunk", "empty": true
        }))
        .unwrap();
        assert_eq!(repo.default_branch.as_deref(), Some("trunk"));
        assert!(repo.empty);

        let pr: PullRequest =
            serde_json::from_value(serde_json::json!({ "number": 142, "comments": 2 })).unwrap();
        assert_eq!(pr.comments, 2);
        // Absent means none, not a parse failure: Gitea omits it on some
        // endpoints, and a default of "some" would cost a request per pull
        // request.
        let pr: PullRequest = serde_json::from_value(serde_json::json!({ "number": 7 })).unwrap();
        assert_eq!(pr.comments, 0);
    }

    /// The projection is partial on purpose: a field Gitea adds tomorrow must
    /// not stop the record deserialising, because the record is what
    /// `SyncItem::payload` keeps verbatim.
    #[test]
    fn an_unknown_field_does_not_stop_a_record_being_read() {
        let repo: Repo = serde_json::from_value(serde_json::json!({
            "full_name": "tidewater/payout-service",
            "something_gitea_1_30_added": 42
        }))
        .expect("unknown fields are ignored");
        assert_eq!(repo.full_name, "tidewater/payout-service");
    }
}
