//! The incremental position: one watermark set per repository, in a versioned
//! envelope (interfaces §4.1 -- "an unrecognised version means full sync").

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

/// Bump when the meaning of a field changes, or when what the adapter emits
/// for an unchanged record changes; an older knobas then reads the cursor as
/// unrecognised and syncs in full, which is the safe direction.
///
/// **2** since issue #595 changed what a branch and a commit store: the payload
/// gained a `repository: { full_name, name }` object, which `under:` reads, and
/// `body_text` lost the repository's name. No field of this envelope changed
/// meaning, but an incremental run re-emits only branches whose head moved and
/// commits past the watermark, so an install upgraded in place would keep the
/// old payload on every row it had already mirrored (issue #610). Bumping is
/// the whole upgrade plan, as it was for Jira's #345: a stored version-1 cursor
/// stops parsing, and each Gitea source does one backfill (a full sync), which
/// re-emits every branch and the commits inside `commits_per_repo` in the new
/// shape and comes back with a version-2 cursor. Commits beyond the newest
/// `commits_per_repo` are never fetched again, so they keep the old payload and
/// `under:` misses them. That limit comes from the budget (which is why ADR-0003
/// records `commit` as non-exhaustive), and rewriting those rows would need its
/// own ruling.
///
/// **3** since issue #718 added the repository's `default_branch` to that
/// `repository` object, which the Code tile reads off a branch, and
/// [`RepoCursor::default_branch`] beside it. Same upgrade plan: a version-2
/// cursor stops parsing and the first run is a backfill. A commit beyond the
/// budget keeps the version-2 object, which nothing reads a default off.
pub(crate) const CURSOR_VERSION: u32 = 3;

/// Where the next run resumes, per repository.
///
/// # Why per-branch head shas rather than one digest
///
/// Interfaces §4.2 sketches this envelope with a `branches_hash`. This adapter
/// stores the heads themselves instead -- `{"main": "<sha>", …}` -- because the
/// cursor is adapter-defined (§4.1, ruling B2) and the heads buy two things a
/// digest cannot:
///
/// * **Commits are fetched only for branches that moved.** Gitea's branch
///   listing has no incremental filter, so every run sees every head anyway; a
///   digest only says "something in this repository changed", which would mean
///   re-walking every branch's commits on every push. Twenty repositories with
///   eight branches each is 200 requests a run instead of about 40.
/// * **A deleted branch can be tombstoned mid-cycle.** A name that was in the
///   cursor and is not in the listing is gone, and `SyncItem { deleted: true }`
///   says so now rather than at the next full sync -- links point at branches
///   (spec §5a), and a dead link is worse than a missing one.
///
/// The cost is roughly 55 bytes per branch in `source_config.cursor`, bounded
/// by the `owners`/`repos` allowlist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct GiteaCursor {
    pub v: u32,
    /// When the repository set in this cursor was last observed. Diagnostic in
    /// M1; reserved for skipping the listing on close-together runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos_listed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<String, RepoCursor>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RepoCursor {
    /// The `updated_at` the repository entity was last emitted with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_updated_at: Option<DateTime<Utc>>,
    /// [`knobas_source::fingerprint`] of the repository record last emitted (#764):
    /// an edit inside the second the previous run read it leaves
    /// `repo_updated_at` where it was, and this is what still sees it.
    /// Absent in a cursor from before #764, which re-emits the repository
    /// once rather than costing a version bump.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_fingerprint: Option<String>,
    /// The newest pull-request `updated_at` delivered so far, with
    /// everything before it delivered too. While [`Self::pulls_catching_up`]
    /// is set, records between the two are still to read (#1624); this field
    /// does not move until they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulls_updated_to: Option<DateTime<Utc>>,
    /// The pull requests sitting exactly on that instant, already delivered:
    /// number -> [`crate::fingerprint::of_pull`] of the record as delivered.
    ///
    /// Gitea's timestamps have one-second resolution, so a strict `>` boundary
    /// would drop a pull request updated in the same second as the newest one.
    /// The walk therefore re-reads the watermark second, and this map is what
    /// keeps that re-read from re-delivering them on every idle run.
    ///
    /// **Keyed by number, valued by fingerprint (#764).** Until #764 this was
    /// `pulls_at_watermark`, a list of numbers, and a pull request merged in
    /// the same second the previous run read it matched its number and was
    /// never mirrored as merged. Renamed rather than retyped so that no version
    /// bump is needed: a stored cursor's `pulls_at_watermark` is an unknown
    /// field now and is ignored, so the run after the upgrade re-delivers the
    /// handful of pull requests on each watermark once, and every other
    /// position in the cursor stands.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pulls_seen_at_watermark: BTreeMap<u64, String>,
    /// An incremental pull-request walk the budget (or the page cap) stopped
    /// before it reached its watermark (#1624). Absent in every cursor from
    /// before #1624 and whenever no walk was cut short, so no version bump.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulls_catching_up: Option<PullsCatchUp>,
    /// The newest issue `updated_at` delivered so far (#1206), and the issues
    /// on that instant as delivered: the pull requests' boundary, for the
    /// same one-second reason. Absent in a cursor from before #1206, whose
    /// next run reads every issue once: a ticket kind is new, so nothing it
    /// re-delivers was delivered before, and no version bump is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues_updated_to: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub issues_seen_at_watermark: BTreeMap<u64, String>,
    /// The newest commit time delivered so far, as `pulls_updated_to`, and
    /// likewise held while [`Self::commits_catching_up`] is set (#1624).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commits_since: Option<DateTime<Utc>>,
    /// The object ids on that instant, already delivered -- the same boundary
    /// argument, and also what makes Gitea's inclusive `since=` filter safe to
    /// use as-is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits_at_watermark: Vec<String>,
    /// An incremental commit walk the budget (or a page cap) stopped before
    /// it reached its watermark (#1624). As [`Self::pulls_catching_up`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commits_catching_up: Option<CommitsCatchUp>,
    /// Branch name -> head object id, as of the last run that walked this
    /// repository.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub branches: BTreeMap<String, String>,
    /// The repository's default branch as of the last run that walked it
    /// (#718). Every branch's stored `repository` names it, so when it
    /// changes every branch is re-emitted, though no head moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

/// Where an incremental pull-request walk the budget stopped picks up
/// (#1624, spec #1618 ruling A6).
///
/// The walk reads newest-updated first, so the records it did not reach are
/// the **older** ones, between the watermark it was heading for and the
/// oldest record it examined. Moving the watermark to that oldest record
/// would have the next walk stop exactly where the unread ones begin, and a
/// watermark only advances as work completes (`CONTEXT.md`). So the watermark
/// stays where it was, and this records the rest: `below`, the oldest record
/// examined, which the next run continues below, and `newest`, the newest
/// record examined, which becomes the watermark once the walk reaches it.
/// Each instant carries the records already delivered on it, as the
/// watermark does.
///
/// A run holding one walks only the window from `below` down to the
/// watermark, under the same budget: anything newer was delivered by the walk
/// that was cut short or changed since, and is read once the window closes,
/// in the same run when the budget allows. `page` is where the cut fell, so
/// that run does not re-read every page above it (see `sync::walk_pulls`).
/// `below` is `None` only where the cut walk examined nothing, and then the
/// window has no top.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PullsCatchUp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seen_at_below: BTreeMap<u64, String>,
    pub newest: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seen_at_newest: BTreeMap<u64, String>,
    pub page: u32,
}

/// Where an incremental commit walk the budget stopped picks up (#1624). As
/// [`PullsCatchUp`], per branch: `branches` are the ones the walk had not
/// finished, in walk order. The first was cut short and is read from `below`
/// down to the watermark; the rest were never started and are read from
/// their heads down to it.
///
/// `page` is the page of the first branch's listing the cut fell on (#2039).
/// Gitea bounds a catch-up's listing with `until=`, so its first page is
/// already the window's and the page goes unused. Forgejo 17 ignores
/// `until=` and lists the branch from its head, and there the next run goes
/// to this page once its first page shows nothing but commits above the
/// window, rather than re-reading every page the cut walk delivered (see
/// `sync::walk_commits`). Absent in a catch-up written before #2039, which
/// is read from the branch head as before, so no version bump.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CommitsCatchUp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen_at_below: Vec<String>,
    pub newest: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen_at_newest: Vec<String>,
    pub branches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

impl GiteaCursor {
    /// A position that knows nothing: everything is fetched in full.
    pub(crate) fn empty() -> Self {
        Self {
            v: CURSOR_VERSION,
            repos_listed_at: None,
            repos: BTreeMap::new(),
        }
    }

    /// The envelope a run that emitted something writes.
    pub(crate) fn fresh() -> Self {
        Self {
            repos_listed_at: Some(Utc::now()),
            ..Self::empty()
        }
    }

    /// Read the cursor the engine handed us, or `None` if there is no usable
    /// position in it -- absent, unreadable, or written by another version.
    ///
    /// **`None` and not `empty()`.** An unusable cursor and a valid one that
    /// happens to know nothing are the same *value*, and `run` has to tell them
    /// apart: a run holding no recovered position is a full sync in the only
    /// sense that matters to ruling B4's extension (a skipped repository has no
    /// stored state to fall back on), even though the engine handed it a
    /// non-`None` cursor. Returning the distinction in the type is what stops
    /// that from being re-derived by comparing against `empty()`, which is a
    /// value coincidence rather than a fact about the parse.
    pub(crate) fn parse(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        match serde_json::from_str::<Self>(raw) {
            Ok(cursor) if cursor.v == CURSOR_VERSION => Some(cursor),
            Ok(cursor) => {
                tracing::warn!(
                    version = cursor.v,
                    "gitea: unrecognised cursor version, syncing in full"
                );
                None
            }
            Err(error) => {
                tracing::warn!(%error, "gitea: unreadable cursor, syncing in full");
                None
            }
        }
    }

    /// This repository's position, or a blank one if it has never been synced.
    pub(crate) fn repo(&self, full_name: &str) -> RepoCursor {
        self.repos.get(full_name).cloned().unwrap_or_default()
    }

    /// The stored entry for `name`, matched the way the `repos[]` allowlist
    /// matches (case-insensitively), returned **under the key it is stored
    /// with**.
    ///
    /// The keys in this map are Gitea's own `full_name`; the names a skipped
    /// allowlist entry is known by are what the user typed. Carrying a skipped
    /// entry forward has to find it despite that difference and must not write
    /// it back under a second spelling, or one repository would occupy two
    /// entries and neither would be the one the walk looks up.
    pub(crate) fn entry_like(&self, name: &str) -> Option<(&String, &RepoCursor)> {
        self.repos
            .iter()
            .find(|(stored, _)| stored.eq_ignore_ascii_case(name))
    }

    pub(crate) fn to_json(&self) -> String {
        // Every field is plain data and every map is ordered, so this cannot
        // fail and cannot vary between two calls.
        serde_json::to_string(self).expect("a GiteaCursor serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GiteaCursor {
        let mut cursor = GiteaCursor::empty();
        cursor.repos.insert(
            "tidewater/payout-service".to_owned(),
            RepoCursor {
                repo_updated_at: Some(at("2026-08-22T11:42:00Z")),
                repo_fingerprint: Some("00000000000000aa".to_owned()),
                pulls_updated_to: Some(at("2026-08-22T13:50:00Z")),
                pulls_seen_at_watermark: BTreeMap::from([(144, "00000000000000bb".to_owned())]),
                pulls_catching_up: Some(PullsCatchUp {
                    below: Some(at("2026-08-20T09:00:00Z")),
                    seen_at_below: BTreeMap::from([(120, "00000000000000dd".to_owned())]),
                    newest: at("2026-08-22T14:10:00Z"),
                    seen_at_newest: BTreeMap::from([(150, "00000000000000ee".to_owned())]),
                    page: 4,
                }),
                issues_updated_to: Some(at("2026-08-22T12:00:00Z")),
                issues_seen_at_watermark: BTreeMap::from([(7, "00000000000000cc".to_owned())]),
                commits_since: Some(at("2026-08-22T11:42:00Z")),
                commits_at_watermark: vec!["c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192".to_owned()],
                commits_catching_up: Some(CommitsCatchUp {
                    below: Some(at("2026-08-21T08:00:00Z")),
                    seen_at_below: vec!["2222222222222222222222222222222222222222".to_owned()],
                    newest: at("2026-08-22T11:50:00Z"),
                    seen_at_newest: vec!["3333333333333333333333333333333333333333".to_owned()],
                    branches: vec!["main".to_owned(), "release/1.4".to_owned()],
                    page: Some(7),
                }),
                branches: BTreeMap::from([(
                    "main".to_owned(),
                    "1111111111111111111111111111111111111111".to_owned(),
                )]),
                default_branch: Some("trunk".to_owned()),
            },
        );
        cursor
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn a_cursor_round_trips() {
        let cursor = sample();
        assert_eq!(GiteaCursor::parse(Some(&cursor.to_json())), Some(cursor));
    }

    /// The engine compares cursors as strings, so serialising the same position
    /// twice has to produce the same bytes -- which is why every map here is a
    /// `BTreeMap`.
    #[test]
    fn serialisation_is_stable() {
        assert_eq!(sample().to_json(), sample().to_json());
        let json = sample().to_json();
        assert!(json.starts_with(r#"{"v":3"#), "{json}");
    }

    /// Every watermark field has to survive the trip: a field dropped in
    /// serialisation reads back as "never seen", and the next run re-emits
    /// everything it covers. The sample carries a *distinct* value in each one
    /// so a field read into the wrong slot fails too.
    #[test]
    fn every_watermark_survives_the_round_trip() {
        let back = GiteaCursor::parse(Some(&sample().to_json())).expect("the sample is usable");
        let repo = back.repo("tidewater/payout-service");
        assert_eq!(repo.repo_updated_at, Some(at("2026-08-22T11:42:00Z")));
        assert_eq!(repo.pulls_updated_to, Some(at("2026-08-22T13:50:00Z")));
        assert_eq!(repo.repo_fingerprint.as_deref(), Some("00000000000000aa"));
        assert_eq!(
            repo.pulls_seen_at_watermark,
            BTreeMap::from([(144, "00000000000000bb".to_owned())])
        );
        assert_eq!(repo.commits_since, Some(at("2026-08-22T11:42:00Z")));
        assert_eq!(
            repo.commits_at_watermark,
            vec!["c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192".to_owned()]
        );
        assert_eq!(
            repo.branches.get("main").map(String::as_str),
            Some("1111111111111111111111111111111111111111")
        );
        assert_eq!(repo.default_branch.as_deref(), Some("trunk"));
        // #1624's catch-ups, each instant and set in its own slot.
        let pulls = repo.pulls_catching_up.expect("the pull catch-up survives");
        assert_eq!(
            (pulls.below, pulls.newest, pulls.page),
            (
                Some(at("2026-08-20T09:00:00Z")),
                at("2026-08-22T14:10:00Z"),
                4
            )
        );
        assert_eq!(pulls.seen_at_below.keys().collect::<Vec<_>>(), vec![&120]);
        assert_eq!(pulls.seen_at_newest.keys().collect::<Vec<_>>(), vec![&150]);
        let commits = repo
            .commits_catching_up
            .expect("the commit catch-up survives");
        assert_eq!(
            (commits.below, commits.newest),
            (Some(at("2026-08-21T08:00:00Z")), at("2026-08-22T11:50:00Z"))
        );
        assert_eq!(commits.seen_at_below[0].as_bytes()[0], b'2');
        assert_eq!(commits.seen_at_newest[0].as_bytes()[0], b'3');
        assert_eq!(commits.branches, vec!["main", "release/1.4"]);
        assert_eq!(commits.page, Some(7));
    }

    /// A commit catch-up written before #2039 carries no `page`. It still
    /// parses, with no page, which the walk reads from the branch head as it
    /// did before: the cursor version stays where it is.
    #[test]
    fn a_commit_catch_up_from_before_its_page_still_parses() {
        let raw = r#"{"v":3,"repos":{"tidewater/payout-service":{"commits_catching_up":{"below":"2026-08-21T08:00:00Z","newest":"2026-08-22T11:50:00Z","branches":["main"]}}}}"#;
        let cursor = GiteaCursor::parse(Some(raw)).expect("a version-3 cursor is usable");
        let catch_up = cursor
            .repo("tidewater/payout-service")
            .commits_catching_up
            .expect("the catch-up is read");
        assert_eq!(catch_up.page, None);
        assert_eq!(catch_up.branches, vec!["main"]);
    }

    /// Empty collections are omitted: the cursor is a text column that grows
    /// with the repository count, and a first sync of thirty repositories
    /// should not carry thirty empty arrays.
    #[test]
    fn empty_fields_are_omitted() {
        let mut cursor = GiteaCursor::empty();
        cursor
            .repos
            .insert("tidewater/ops-runbooks".to_owned(), RepoCursor::default());
        let json = cursor.to_json();
        assert!(!json.contains("pulls_seen_at_watermark"), "{json}");
        assert!(!json.contains("repo_fingerprint"), "{json}");
        assert!(!json.contains("branches"), "{json}");
        assert!(!json.contains("repos_listed_at"), "{json}");
        assert!(!json.contains("catching_up"), "{json}");
    }

    /// A cursor written by a later shape, or by something else entirely, means
    /// "sync in full" -- never a half-understood position.
    ///
    /// `None`, not `empty()`: `sync::run` reads the absence to decide whether
    /// this run has any state to fall back on, and a value that merely *equals*
    /// `empty()` would not tell it that. See `parse`'s docs.
    #[test]
    fn an_unusable_cursor_means_full_sync() {
        for raw in [
            r#"{"v":4,"repos":{}}"#,
            r#"{"v":2,"repos":{}}"#,
            "not json",
            "",
            "[]",
            r#"{"repos":{}}"#,
        ] {
            assert_eq!(GiteaCursor::parse(Some(raw)), None, "{raw:?}");
        }
        assert_eq!(GiteaCursor::parse(None), None);
        // And the version this knobas writes is still understood, so the guard
        // above is a version check rather than a blanket refusal.
        assert_eq!(
            GiteaCursor::parse(Some(&sample().to_json())),
            Some(sample())
        );
    }

    /// Issue #610: a cursor stored by a knobas from before #595's payload
    /// change -- a real position, every field readable -- is still no position
    /// at all, so the run after an upgrade is a full sync and re-emits every
    /// branch and the budgeted commits in the new shape.
    #[test]
    fn a_version_one_cursor_from_before_the_payload_change_means_full_sync() {
        let mut stored = sample();
        stored.v = 1;
        let raw = stored.to_json();
        assert!(raw.starts_with(r#"{"v":1,"#), "{raw}");
        assert_eq!(GiteaCursor::parse(Some(&raw)), None);
    }

    /// `entry_like` exists so a **refused `repos[]` entry keeps its position**
    /// (`sync::run`), and the allowlist is matched case-insensitively
    /// (`sync::push_selected`), so `Tidewater/Payout-Service` is a supported
    /// spelling of a repository Gitea calls `tidewater/payout-service`.
    ///
    /// Both halves of that doc are pinned here, because each fails differently
    /// and silently:
    ///
    /// * **matches case-insensitively** -- an exact-match lookup finds nothing
    ///   for a mixed-case entry, so the carry-forward writes nothing and the
    ///   repository loses its watermarks anyway. The fix for that would be
    ///   green in every end-to-end test, because every fixture's allowlist is
    ///   already exact-case.
    /// * **returns the *stored* spelling** -- re-inserting under the queried
    ///   spelling leaves one repository holding two cursor entries, and the
    ///   walk looks up neither of them by the name it has.
    ///
    /// A unit test and not a fixture: the wiremock fake matches request paths
    /// case-sensitively, so a mixed-case end-to-end run would be exercising the
    /// fake's routing rather than this lookup.
    #[test]
    fn a_refused_allowlist_entry_is_found_whatever_case_the_user_typed() {
        let cursor = sample();
        let stored = "tidewater/payout-service";

        for typed in [
            "tidewater/payout-service",
            "Tidewater/Payout-Service",
            "TIDEWATER/PAYOUT-SERVICE",
        ] {
            let (key, position) = cursor
                .entry_like(typed)
                .unwrap_or_else(|| panic!("{typed:?} must find the stored entry"));
            assert_eq!(
                key, stored,
                "{typed:?} must come back under the spelling the cursor stores, \
                 or the carry-forward writes a second entry for one repository"
            );
            // And it is the real position, not a blank one: this is the state
            // the whole carry-forward exists to preserve.
            assert_eq!(position, &cursor.repo(stored));
            assert!(position.repo_updated_at.is_some());
        }

        // Case-insensitivity is not case-blindness: a different repository is
        // still a different repository.
        assert!(cursor.entry_like("tidewater/payout-services").is_none());
        assert!(cursor.entry_like("elsewhere/unrelated").is_none());
    }

    /// A repository with no entry yet is fetched in full -- which is how a
    /// repository added upstream is picked up (interfaces §4.2).
    #[test]
    fn an_unknown_repository_starts_from_nothing() {
        assert_eq!(sample().repo("tidewater/ledger-api"), RepoCursor::default());
        assert!(
            sample()
                .repo("tidewater/payout-service")
                .pulls_updated_to
                .is_some()
        );
    }

    #[test]
    fn a_fresh_cursor_records_when_the_repository_set_was_seen() {
        assert!(GiteaCursor::fresh().repos_listed_at.is_some());
        assert_eq!(GiteaCursor::fresh().v, CURSOR_VERSION);
    }
}
