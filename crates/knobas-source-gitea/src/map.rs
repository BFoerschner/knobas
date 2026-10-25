//! Gitea's records, normalized the way interfaces §4.1 prescribes:
//! `title` = the one-line summary, `body_text` = what FTS indexes, `payload` =
//! the raw record verbatim, `author` = the source's own username string,
//! `updated_at` = the source's own timestamp and never `now()`.

use chrono::{DateTime, Utc};
use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;
use serde_json::Value;

use crate::model;
use crate::{KIND_BRANCH, KIND_COMMIT, KIND_PR, KIND_REPO, KIND_TICKET, keys};

/// Which repository the record being mapped belongs to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RepoRef<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub full_name: &'a str,
    /// The repository's own browser URL, which is what branch links hang off.
    pub html_url: Option<&'a str>,
    /// The repository's default branch, as its record names it (#718).
    pub default_branch: Option<&'a str>,
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

/// The repository a branch or commit sits in, as the stored payload carries it.
///
/// Gitea's branch and commit records do not name their repository, and the
/// repo is their **parent** (ADR-0018), which the descriptor declares at
/// `repository` (`crate::payload_paths`). So the adapter writes it in, in
/// Gitea's own repository spelling -- the `full_name` and `name` a pull
/// request's `base.repo` already carries -- so that one declaration of names
/// reads both, and the full name, first, is the one a path line shows. An
/// object and not a bare `owner/name`, because a parent is an object or a
/// chain of them (contract battery clause 6).
///
/// The object also carries the repository's `default_branch` when the
/// repository record names one (#718), a key `base.repo` carries too.
/// Gitea's branch record does not say whether it is the default, and the
/// Code tile must know, so the descriptor declares a branch's
/// `default_branch` here.
fn repository(at: RepoRef<'_>) -> Value {
    let mut repository = serde_json::json!({ "full_name": at.full_name, "name": at.name });
    if let Some(default_branch) = at.default_branch {
        repository["default_branch"] = default_branch.into();
    }
    repository
}

/// The record verbatim, plus the [`repository`] it sits in where the record
/// does not already name one. The one departure from §3a's "payload =
/// the raw record verbatim", and the one ADR-0018 asks for by name.
fn with_repository(raw: &Value, at: RepoRef<'_>) -> Value {
    let mut payload = raw.clone();
    if let Some(record) = payload.as_object_mut() {
        record.entry("repository").or_insert_with(|| repository(at));
    }
    payload
}

/// Drop timestamps that are not timestamps: Go's zero time (year 1) is how
/// Gitea says "unset" for several fields.
pub(crate) fn real_time(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| t.timestamp() > 0)
}

/// The one-line summary of a commit message: its first line that is not
/// blank, so the mirror and a source view title a commit the same (#678).
pub(crate) fn subject(message: &str) -> String {
    message
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The characters that would end a URL path early. Branch names may contain
/// them; `/` must survive, because Gitea's branch URLs are nested.
pub(crate) fn url_path(name: &str) -> String {
    name.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
}

/// A repository's record, with its teams beside it where Gitea listed them
/// (#966): `teams` is the team records the listing answered, less any that
/// holds every repository of its organization (*Owners*), which says nothing
/// about this one. Absent where Gitea would not say ([`None`]), so a reader
/// misses rather than reading "no team".
fn with_teams(raw: &Value, teams: Option<&[Value]>) -> Value {
    let mut payload = raw.clone();
    if let (Some(teams), Some(record)) = (teams, payload.as_object_mut()) {
        let own = teams
            .iter()
            .filter(|team| team.get("includes_all_repositories") != Some(&Value::Bool(true)))
            .cloned()
            .collect();
        record.insert("teams".to_owned(), Value::Array(own));
    }
    payload
}

pub(crate) fn repo_item(
    source_id: &str,
    raw: &Value,
    repo: &model::Repo,
    at: RepoRef<'_>,
    teams: Option<&[Value]>,
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
        payload: with_teams(raw, teams),
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
        // Not the repo's name: being inside a repo is a filter, not a word
        // (ADR-0018). `under:` reads the stored `repository` instead.
        body_text: body_text(
            [
                Some(branch.name.clone()),
                head.and_then(|c| c.message.clone()),
            ]
            .into_iter()
            .flatten(),
        ),
        author,
        updated_at: real_time(head.and_then(|c| c.timestamp)),
        payload: with_repository(raw, at),
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
        body_text: body_text([branch.to_owned()]),
        author: None,
        updated_at: None,
        // The same `repository` object a live branch carries: the parent
        // declaration reads both, and clause 6 checks both.
        payload: serde_json::json!({
            "name": branch,
            "repository": repository(at),
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
        payload: with_reviewers_listed(raw),
        web_url: non_blank(pr.html_url.as_deref()),
        deleted: false,
    }
}

/// A pull request's record with `requested_reviewers: null` written as `[]`,
/// the one departure from the record verbatim a pull request has (#2098).
///
/// For a pull request nobody was asked to review, Gitea 1.27.3's `/pulls`
/// listing answers `null` and its read of the one pull request `[]`;
/// Forgejo 17 answers `[]` to both (measured by the live battery's clause
/// 13). The run reads the listing and a refresh the one pull request, so
/// without this the stored record, at a path the descriptor declares, would
/// depend on which read brought it. Both say "nobody".
fn with_reviewers_listed(raw: &Value) -> Value {
    let mut payload = raw.clone();
    if let Some(reviewers) = payload.get_mut("requested_reviewers")
        && reviewers.is_null()
    {
        *reviewers = Value::Array(Vec::new());
    }
    payload
}

/// The scope of the label an issue's priority is read from (#1206), as
/// Gitea's own *Default* label set spells it: `Priority/High`.
const PRIORITY_SCOPE: &str = "priority";

/// The priority an issue's labels give it: the value of its first scoped
/// label `Priority/<value>`, the scope matched case-insensitively. `None`
/// without one.
pub(crate) fn priority(labels: &[model::Label]) -> Option<String> {
    labels.iter().find_map(|label| {
        let (scope, value) = label.name.as_deref()?.split_once('/')?;
        let value = value.trim();
        (scope.trim().eq_ignore_ascii_case(PRIORITY_SCOPE) && !value.is_empty())
            .then(|| value.to_owned())
    })
}

/// An issue as a ticket (#1206): the record verbatim, plus a `knobas` object
/// holding what the ticket system reads and Gitea has no field for -- its own
/// key, the priority its labels give, and a type and a parent it never has
/// (`crate::ticket_paths`). Written over whatever the record holds under
/// that name, so no field Gitea adds can be read as one of them.
pub(crate) fn ticket_item(
    source_id: &str,
    raw: &Value,
    at: RepoRef<'_>,
    issue: &model::Issue,
    comments: &[model::Comment],
) -> SyncItem {
    let title = issue
        .title
        .clone()
        .unwrap_or_else(|| format!("#{}", issue.number));
    let key = keys::issue_key(at.owner, at.name, issue.number);
    let mut payload = raw.clone();
    if let Some(record) = payload.as_object_mut() {
        record.insert(
            "knobas".to_owned(),
            serde_json::json!({
                "key": key,
                "priority": priority(issue.labels.as_deref().unwrap_or_default()),
                "issue_type": null,
                "parent": null,
            }),
        );
    }
    SyncItem {
        entity: EntityRef::new(source_id, &key),
        kind: KIND_TICKET.to_owned(),
        title: title.clone(),
        body_text: body_text(
            [Some(title), issue.body.clone()]
                .into_iter()
                .chain(comments.iter().map(|c| c.body.clone()))
                .flatten(),
        ),
        author: issue
            .user
            .as_ref()
            .and_then(|u| non_blank(u.login.as_deref())),
        updated_at: real_time(issue.updated_at).or_else(|| real_time(issue.created_at)),
        payload,
        web_url: non_blank(issue.html_url.as_deref()),
        deleted: false,
    }
}

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
        body_text: body_text([message]),
        author,
        updated_at: real_time(commit.happened_at()),
        payload: with_repository(raw, at),
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
            default_branch: Some("main"),
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
        let item = repo_item(SOURCE, &raw, &repo, at(), None);
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
        // Being inside a repo is a filter, not a word (ADR-0018): the branch's
        // text no longer carries the repo's name, so a search for the repo
        // finds the repo and not every branch in it. `under:` and the stored
        // repository below are what tell two `main`s apart now.
        assert!(!item.body_text.contains("tidewater/payout-service"));
        assert!(!item.body_text.contains("payout-service"));
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
        // Gitea's branch record does not name its repository, so the adapter
        // writes it in, in Gitea's own repository spelling -- full name first
        // -- for the declared parent to read. Everything else is the record
        // verbatim.
        assert_eq!(item.payload["repository"], repository_object());
        let mut verbatim = item.payload.clone();
        verbatim.as_object_mut().unwrap().remove("repository");
        assert_eq!(verbatim, raw);
        assert!(!item.deleted);
    }

    /// The repository a branch or commit sits in, as the stored payload must
    /// carry it: `full_name`, `name` and `default_branch`, the keys a pull
    /// request's `base.repo` already carries.
    fn repository_object() -> Value {
        serde_json::json!({
            "full_name": "tidewater/payout-service",
            "name": "payout-service",
            "default_branch": "main"
        })
    }

    /// A repository whose record names no default branch writes none (#718):
    /// the declared `default_branch` then misses, and the branch is drawn as
    /// any other rather than taken for a default it may not be.
    #[test]
    fn a_repository_naming_no_default_branch_writes_none() {
        let raw = serde_json::json!({ "name": "main", "commit": { "id": "1" } });
        let branch: crate::model::Branch = serde_json::from_value(raw.clone()).unwrap();
        let unnamed = RepoRef {
            default_branch: None,
            ..at()
        };
        assert_eq!(
            branch_item(SOURCE, &raw, unnamed, &branch).payload["repository"],
            serde_json::json!({
                "full_name": "tidewater/payout-service",
                "name": "payout-service"
            })
        );
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
        // The same repository object a live branch carries, not a bare
        // string: clause 6's type check reads the tombstone too.
        assert_eq!(item.payload["repository"], repository_object());
        assert!(!item.body_text.contains("payout-service"));
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
        // grafted onto it (spec §3a "raw payload kept"). A pull request names
        // its repository already, at `base.repo`, so nothing is written in.
        assert_eq!(item.payload, raw);
        assert!(!item.body_text.contains("payout-service"));
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
        // The repo is the commit's parent, not a word in it (ADR-0018).
        assert!(!item.body_text.contains("payout-service"));
        // A commit's record does not name its repository; the stored payload
        // does, for the declared parent to read.
        assert_eq!(item.payload["repository"], repository_object());
        assert_eq!(item.payload["sha"], raw["sha"]);
        assert_eq!(item.author.as_deref(), Some("mara"));
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://gitea.example/tidewater/payout-service/commit/c90d11a")
        );
    }

    /// git allows a message that opens on blank lines; its subject is the
    /// first line with something on it, here as in every source view (#678).
    #[test]
    fn a_subject_skips_the_blank_lines_a_message_opens_on() {
        assert_eq!(subject("\n  \nPAY-231: jitter\n\nbody"), "PAY-231: jitter");
        assert_eq!(subject("  one line  "), "one line");
        assert_eq!(subject("\n\n"), "");
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

    /// An issue as Gitea 1.27.3's `/issues` listing serves it (measured
    /// 2026-10-19), as far as the ticket reads it.
    fn raw_issue(labels: Value) -> Value {
        serde_json::json!({
            "number": 7, "title": "Payout CSV drops the currency column",
            "body": "Seen on the EUR batch.", "state": "open", "comments": 1,
            "user": { "login": "mara" }, "assignee": { "login": "tom" },
            "labels": labels, "pull_request": null,
            "html_url": "https://gitea.example/tidewater/payout-service/issues/7",
            "repository": { "id": 1, "name": "payout-service", "owner": "tidewater",
                            "full_name": "tidewater/payout-service" },
            "created_at": "2026-08-20T09:00:00Z", "updated_at": "2026-08-22T09:00:00Z"
        })
    }

    #[test]
    fn an_issue_becomes_a_ticket_under_its_own_key() {
        let raw =
            raw_issue(serde_json::json!([{ "name": "Priority/High" }, { "name": "Kind/Bug" }]));
        let issue: crate::model::Issue = serde_json::from_value(raw.clone()).unwrap();
        let comments = [crate::model::Comment {
            body: Some("Reproduced on main.".to_owned()),
        }];
        let item = ticket_item(SOURCE, &raw, at(), &issue, &comments);
        assert_eq!(
            item.entity.to_string(),
            "gitea:tidewater/payout-service/issues/7"
        );
        assert_eq!(item.kind, "ticket");
        assert_eq!(item.title, "Payout CSV drops the currency column");
        assert_eq!(
            item.body_text,
            "Payout CSV drops the currency column\n\nSeen on the EUR batch.\n\nReproduced on main."
        );
        assert_eq!(item.author.as_deref(), Some("mara"));
        assert_eq!(
            item.updated_at.unwrap().to_rfc3339(),
            "2026-08-22T09:00:00+00:00"
        );
        assert_eq!(
            item.payload["knobas"],
            serde_json::json!({
                "key": "tidewater/payout-service/issues/7",
                "priority": "High",
                "issue_type": null,
                "parent": null
            })
        );
        // Everything else is the record verbatim.
        assert_eq!(item.payload["assignee"], raw["assignee"]);
        assert!(!item.deleted);
    }

    /// The priority is a scoped label's value, the scope in any case; an
    /// issue without one has none, and so does a bare `priority` label.
    #[test]
    fn a_priority_is_read_off_a_scoped_label_or_is_none() {
        let labels = |names: &[&str]| -> Vec<crate::model::Label> {
            names
                .iter()
                .map(|n| crate::model::Label {
                    name: Some((*n).to_owned()),
                })
                .collect()
        };
        assert_eq!(
            priority(&labels(&["Kind/Bug", "priority/Low"])).as_deref(),
            Some("Low")
        );
        assert_eq!(
            priority(&labels(&["PRIORITY / Critical"])).as_deref(),
            Some("Critical")
        );
        assert_eq!(
            priority(&labels(&["Kind/Bug", "priority", "Priority/"])),
            None
        );
        assert_eq!(priority(&[]), None);
    }
}
