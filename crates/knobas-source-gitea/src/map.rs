//! Gitea's records, normalized the way interfaces §4.1 prescribes:
//! `title` = the one-line summary, `body_text` = what FTS indexes, `payload` =
//! the raw record verbatim, `author` = the source's own username string,
//! `updated_at` = the source's own timestamp and never `now()`.

use chrono::{DateTime, Utc};
use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;
use serde_json::Value;

use crate::model;
use crate::{KIND_BRANCH, KIND_COMMIT, KIND_PR, KIND_REPO, keys};

/// Which repository the record being mapped belongs to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RepoRef<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub full_name: &'a str,
    /// The repository's own browser URL, which is what branch links hang off.
    pub html_url: Option<&'a str>,
}

/// Join the non-empty parts into the blob FTS indexes (interfaces §4.1,
/// mirroring `knobas-source-mock`).
fn body_text(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Drop timestamps that are not timestamps: Go's zero time (year 1) is how
/// Gitea says "unset" for several fields.
pub(crate) fn real_time(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| t.timestamp() > 0)
}

/// The one-line summary of a commit message.
fn subject(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim().to_owned()
}

/// The characters that would end a URL path early. Branch names may contain
/// them; `/` must survive, because Gitea's branch URLs are nested.
fn url_path(name: &str) -> String {
    name.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
}

pub(crate) fn repo_item(
    source_id: &str,
    raw: &Value,
    repo: &model::Repo,
    at: RepoRef<'_>,
) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(source_id, &keys::repo_key(at.owner, at.name)),
        kind: KIND_REPO.to_owned(),
        title: repo.full_name.clone(),
        body_text: body_text(
            [Some(repo.full_name.clone()), repo.description.clone()]
                .into_iter()
                .flatten(),
        ),
        author: repo
            .owner
            .as_ref()
            .and_then(|o| non_blank(o.login.as_deref())),
        updated_at: real_time(repo.updated_at),
        payload: raw.clone(),
        web_url: non_blank(repo.html_url.as_deref()),
        deleted: false,
    }
}

pub(crate) fn branch_item(
    source_id: &str,
    raw: &Value,
    at: RepoRef<'_>,
    branch: &model::Branch,
) -> SyncItem {
    let head = branch.commit.as_ref();
    let author = head
        .and_then(|c| c.author.as_ref())
        .and_then(|a| non_blank(a.username.as_deref()).or_else(|| non_blank(a.name.as_deref())));
    SyncItem {
        entity: EntityRef::new(
            source_id,
            &keys::branch_key(at.owner, at.name, &branch.name),
        ),
        kind: KIND_BRANCH.to_owned(),
        title: branch.name.clone(),
        body_text: body_text(
            [
                Some(branch.name.clone()),
                Some(at.full_name.to_owned()),
                head.and_then(|c| c.message.clone()),
            ]
            .into_iter()
            .flatten(),
        ),
        author,
        updated_at: real_time(head.and_then(|c| c.timestamp)),
        payload: raw.clone(),
        web_url: at.html_url.map(|base| {
            format!(
                "{}/src/branch/{}",
                base.trim_end_matches('/'),
                url_path(&branch.name)
            )
        }),
        deleted: false,
    }
}

/// A branch that was in the cursor and is not in the listing any more.
///
/// The payload is synthesised -- there is no record to keep, because the record
/// is what disappeared. The title is the name, so the mirror can still show
/// what vanished.
pub(crate) fn branch_tombstone(source_id: &str, at: RepoRef<'_>, branch: &str) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(source_id, &keys::branch_key(at.owner, at.name, branch)),
        kind: KIND_BRANCH.to_owned(),
        title: branch.to_owned(),
        body_text: body_text([branch.to_owned(), at.full_name.to_owned()]),
        author: None,
        updated_at: None,
        payload: serde_json::json!({
            "name": branch,
            "repository": at.full_name,
            "deleted": true
        }),
        web_url: None,
        deleted: true,
    }
}

pub(crate) fn pr_item(
    source_id: &str,
    raw: &Value,
    at: RepoRef<'_>,
    pr: &model::PullRequest,
    comments: &[model::Comment],
) -> SyncItem {
    let title = pr
        .title
        .clone()
        .unwrap_or_else(|| format!("#{}", pr.number));
    SyncItem {
        entity: EntityRef::new(source_id, &keys::pr_key(at.owner, at.name, pr.number)),
        kind: KIND_PR.to_owned(),
        title: title.clone(),
        body_text: body_text(
            [Some(title), pr.body.clone()]
                .into_iter()
                .chain(comments.iter().map(|c| c.body.clone()))
                .flatten(),
        ),
        author: pr.user.as_ref().and_then(|u| non_blank(u.login.as_deref())),
        updated_at: real_time(pr.updated_at).or_else(|| real_time(pr.created_at)),
        payload: raw.clone(),
        web_url: non_blank(pr.html_url.as_deref()),
        deleted: false,
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired in task 7: incremental commits per moved branch"
    )
)]
pub(crate) fn commit_item(
    source_id: &str,
    raw: &Value,
    at: RepoRef<'_>,
    commit: &model::Commit,
) -> SyncItem {
    let message = commit
        .commit
        .as_ref()
        .and_then(|c| c.message.clone())
        .unwrap_or_default();
    let title = if message.trim().is_empty() {
        commit.sha.chars().take(12).collect()
    } else {
        subject(&message)
    };
    let author = commit
        .author
        .as_ref()
        .and_then(|u| non_blank(u.login.as_deref()))
        .or_else(|| {
            commit
                .commit
                .as_ref()
                .and_then(|c| c.author.as_ref())
                .and_then(|a| non_blank(a.name.as_deref()))
        });
    SyncItem {
        entity: EntityRef::new(source_id, &keys::commit_key(at.owner, at.name, &commit.sha)),
        kind: KIND_COMMIT.to_owned(),
        title,
        body_text: body_text([message, at.full_name.to_owned()]),
        author,
        updated_at: real_time(commit.happened_at()),
        payload: raw.clone(),
        web_url: non_blank(commit.html_url.as_deref()),
        deleted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "gitea";

    fn at() -> RepoRef<'static> {
        RepoRef {
            owner: "tidewater",
            name: "payout-service",
            full_name: "tidewater/payout-service",
            html_url: Some("https://gitea.example/tidewater/payout-service"),
        }
    }

    fn raw_repo() -> Value {
        serde_json::json!({
            "full_name": "tidewater/payout-service",
            "owner": { "login": "tidewater" },
            "description": "Payout processing service",
            "html_url": "https://gitea.example/tidewater/payout-service",
            "default_branch": "main",
            "updated_at": "2026-08-22T11:42:00Z",
            "stars_count": 3
        })
    }

    #[test]
    fn a_repository_becomes_a_searchable_item() {
        let raw = raw_repo();
        let repo: crate::model::Repo = serde_json::from_value(raw.clone()).unwrap();
        let item = repo_item(SOURCE, &raw, &repo, at());
        assert_eq!(item.entity.to_string(), "gitea:tidewater/payout-service");
        assert_eq!(item.kind, crate::KIND_REPO);
        assert_eq!(item.title, "tidewater/payout-service");
        assert_eq!(
            item.body_text,
            "tidewater/payout-service\n\nPayout processing service"
        );
        assert_eq!(item.author.as_deref(), Some("tidewater"));
        assert_eq!(
            item.updated_at.unwrap().to_rfc3339(),
            "2026-08-22T11:42:00+00:00"
        );
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://gitea.example/tidewater/payout-service")
        );
        assert!(!item.deleted);
        // Spec §3a: the raw record is kept verbatim, fields this adapter does
        // not read included -- that is what lets a later mapping re-project
        // existing data without re-syncing.
        assert_eq!(item.payload, raw);
        assert_eq!(item.payload["stars_count"], 3);
    }

    #[test]
    fn a_branch_carries_its_repository_and_head_message() {
        let raw = serde_json::json!({
            "name": "feature/PAY-231-sepa-retry",
            "commit": {
                "id": "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                "message": "PAY-231: jitter in backoff, cap at 5 attempts",
                "timestamp": "2026-08-22T11:42:00Z",
                "author": { "name": "Mara Lindqvist", "username": "mara" }
            }
        });
        let branch: crate::model::Branch = serde_json::from_value(raw.clone()).unwrap();
        let item = branch_item(SOURCE, &raw, at(), &branch);
        assert_eq!(
            item.entity.to_string(),
            "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"
        );
        assert_eq!(item.kind, crate::KIND_BRANCH);
        assert_eq!(item.title, "feature/PAY-231-sepa-retry");
        // Two repositories both have a `main`; the searchable text is what
        // tells them apart.
        assert!(item.body_text.contains("tidewater/payout-service"));
        assert!(item.body_text.contains("jitter in backoff"));
        assert_eq!(item.author.as_deref(), Some("mara"));
        assert_eq!(
            item.updated_at.unwrap().to_rfc3339(),
            "2026-08-22T11:42:00+00:00"
        );
        assert_eq!(
            item.web_url.as_deref(),
            Some(
                "https://gitea.example/tidewater/payout-service/src/branch/feature/PAY-231-sepa-retry"
            )
        );
        assert_eq!(item.payload, raw);
        assert!(!item.deleted);
    }

    /// Gitea leaves `username` empty for a commit whose email matches no
    /// account. The record's own name is still the source's word for who did
    /// it; inventing one is what §4.1 forbids, using theirs is not.
    #[test]
    fn a_branch_falls_back_to_the_committer_name() {
        let raw = serde_json::json!({
            "name": "main",
            "commit": { "id": "1111", "message": "x", "timestamp": "2026-08-21T16:00:00Z",
                        "author": { "name": "Outside Contributor", "username": "" } }
        });
        let branch: crate::model::Branch = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(
            branch_item(SOURCE, &raw, at(), &branch).author.as_deref(),
            Some("Outside Contributor")
        );
    }

    /// A branch URL is derived from the repository's own `html_url`, never from
    /// the configured base URL -- Gitea may be served under a path prefix and
    /// its `ROOT_URL` is the only thing that knows.
    #[test]
    fn a_branch_url_escapes_what_would_break_it() {
        let raw = serde_json::json!({ "name": "wip/fix #12", "commit": { "id": "1" } });
        let branch: crate::model::Branch = serde_json::from_value(raw.clone()).unwrap();
        let item = branch_item(SOURCE, &raw, at(), &branch);
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://gitea.example/tidewater/payout-service/src/branch/wip/fix%20%2312")
        );
        // A repository the listing gave no browser URL for has no branch URL
        // either, rather than one composed from the API base.
        let nowhere = RepoRef {
            html_url: None,
            ..at()
        };
        assert!(
            branch_item(SOURCE, &raw, nowhere, &branch)
                .web_url
                .is_none()
        );
    }

    #[test]
    fn a_deleted_branch_keeps_its_name_so_the_ui_can_show_what_vanished() {
        let item = branch_tombstone(SOURCE, at(), "fix/PAY-228-partial-refund-drift");
        assert!(item.deleted);
        assert_eq!(
            item.entity.to_string(),
            "gitea:tidewater/payout-service@refs/heads/fix/PAY-228-partial-refund-drift"
        );
        assert_eq!(item.title, "fix/PAY-228-partial-refund-drift");
        assert_eq!(item.kind, crate::KIND_BRANCH);
        assert!(item.web_url.is_none(), "there is nothing left to open");
    }

    #[test]
    fn a_pull_request_indexes_its_discussion() {
        let raw = serde_json::json!({
            "number": 142,
            "title": "SEPA retry with exponential backoff",
            "body": "Retries transient PSP errors.",
            "user": { "login": "mara" },
            "html_url": "https://gitea.example/tidewater/payout-service/pulls/142",
            "created_at": "2026-08-21T17:41:00Z",
            "updated_at": "2026-08-22T10:20:00Z",
            "comments": 2
        });
        let pr: crate::model::PullRequest = serde_json::from_value(raw.clone()).unwrap();
        let comments: Vec<crate::model::Comment> = serde_json::from_value(serde_json::json!([
            { "body": "Should the jitter be bounded?" },
            { "body": "Bounded to +/-10 % in c90d11." }
        ]))
        .unwrap();
        let item = pr_item(SOURCE, &raw, at(), &pr, &comments);
        assert_eq!(
            item.entity.to_string(),
            "gitea:tidewater/payout-service#142"
        );
        assert_eq!(item.kind, crate::KIND_PR);
        assert_eq!(item.title, "SEPA retry with exponential backoff");
        assert_eq!(
            item.body_text,
            "SEPA retry with exponential backoff\n\nRetries transient PSP errors.\n\nShould the \
             jitter be bounded?\n\nBounded to +/-10 % in c90d11."
        );
        assert_eq!(item.author.as_deref(), Some("mara"));
        // `updated_at` wins over `created_at`, and both are present and
        // different so reading the wrong one is visible.
        assert_eq!(
            item.updated_at.unwrap().to_rfc3339(),
            "2026-08-22T10:20:00+00:00"
        );
        // The payload stays the record Gitea sent: comments are searchable, not
        // grafted onto it (spec §3a "raw payload kept").
        assert_eq!(item.payload, raw);
    }

    /// A pull request that has never been edited carries a Go zero
    /// `updated_at` on some Gitea versions; its creation time is still a real
    /// timestamp and is what "recently updated" should sort it by.
    #[test]
    fn a_pull_request_with_no_update_falls_back_to_when_it_was_opened() {
        let raw = serde_json::json!({
            "number": 7, "created_at": "2026-08-21T17:41:00Z",
            "updated_at": "0001-01-01T00:00:00Z"
        });
        let pr: crate::model::PullRequest = serde_json::from_value(raw.clone()).unwrap();
        let item = pr_item(SOURCE, &raw, at(), &pr, &[]);
        assert_eq!(
            item.updated_at.unwrap().to_rfc3339(),
            "2026-08-21T17:41:00+00:00"
        );
        // With no title of its own it is still addressable on screen.
        assert_eq!(item.title, "#7");
    }

    #[test]
    fn a_commit_titles_itself_with_its_subject_line() {
        let raw = serde_json::json!({
            "sha": "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
            "created": "2026-08-22T11:42:00Z",
            "html_url": "https://gitea.example/tidewater/payout-service/commit/c90d11a",
            "author": { "login": "mara" },
            "commit": { "message": "PAY-231: jitter in backoff\n\nCap at 5 attempts.\n" }
        });
        let commit: crate::model::Commit = serde_json::from_value(raw.clone()).unwrap();
        let item = commit_item(SOURCE, &raw, at(), &commit);
        assert_eq!(
            item.entity.to_string(),
            "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192"
        );
        assert_eq!(item.kind, crate::KIND_COMMIT);
        assert_eq!(item.title, "PAY-231: jitter in backoff");
        // The whole message is searchable -- ticket keys turn up in trailers
        // and bodies, and M2's suggestion engine reads exactly this text.
        assert!(item.body_text.contains("Cap at 5 attempts."));
        assert!(item.body_text.contains("tidewater/payout-service"));
        assert_eq!(item.author.as_deref(), Some("mara"));
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://gitea.example/tidewater/payout-service/commit/c90d11a")
        );
    }

    /// Go serialises an unset time as year 1 rather than as null. Left alone it
    /// would sort ahead of everything in "recent items" for ever.
    #[test]
    fn the_go_zero_time_is_no_timestamp_at_all() {
        let zero = DateTime::parse_from_rfc3339("0001-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(real_time(Some(zero)), None);
        let real = DateTime::parse_from_rfc3339("2026-08-22T11:42:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(real_time(Some(real)), Some(real));
        assert_eq!(real_time(None), None);
        // The Unix epoch itself is the other zero value a Go service emits.
        let epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(real_time(Some(epoch)), None);
    }
}
