//! The incremental position: one watermark set per repository, in a versioned
//! envelope (interfaces §4.1 -- "an unrecognised version means full sync").

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

/// Bump when the meaning of a field changes; an older knobas then reads the
/// cursor as unrecognised and syncs in full, which is the safe direction.
pub(crate) const CURSOR_VERSION: u32 = 1;

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
    /// The newest pull-request `updated_at` delivered so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulls_updated_to: Option<DateTime<Utc>>,
    /// The pull requests sitting exactly on that instant, already delivered.
    ///
    /// Gitea's timestamps have one-second resolution, so a strict `>` boundary
    /// would drop a pull request updated in the same second as the newest one;
    /// remembering the handful of numbers at the boundary closes that hole
    /// without re-delivering them on every idle run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pulls_at_watermark: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commits_since: Option<DateTime<Utc>>,
    /// The object ids on that instant, already delivered -- the same boundary
    /// argument, and also what makes Gitea's inclusive `since=` filter safe to
    /// use as-is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits_at_watermark: Vec<String>,
    /// Branch name -> head object id, as of the last run that walked this
    /// repository.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub branches: BTreeMap<String, String>,
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
                pulls_updated_to: Some(at("2026-08-22T13:50:00Z")),
                pulls_at_watermark: vec![144],
                commits_since: Some(at("2026-08-22T11:42:00Z")),
                commits_at_watermark: vec!["c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192".to_owned()],
                branches: BTreeMap::from([(
                    "main".to_owned(),
                    "1111111111111111111111111111111111111111".to_owned(),
                )]),
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
        assert!(json.starts_with(r#"{"v":1"#), "{json}");
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
        assert_eq!(repo.pulls_at_watermark, vec![144]);
        assert_eq!(repo.commits_since, Some(at("2026-08-22T11:42:00Z")));
        assert_eq!(
            repo.commits_at_watermark,
            vec!["c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192".to_owned()]
        );
        assert_eq!(
            repo.branches.get("main").map(String::as_str),
            Some("1111111111111111111111111111111111111111")
        );
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
        assert!(!json.contains("pulls_at_watermark"), "{json}");
        assert!(!json.contains("branches"), "{json}");
        assert!(!json.contains("repos_listed_at"), "{json}");
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
