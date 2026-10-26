//! The incremental position: `{"v":4,…}`, opaque to everything but this crate.
//!
//! ```text
//! {"v":4,
//!  "updated_to":"2026-08-22T11:48:00Z",   // the newest `updated` delivered
//!  "tz_offset_secs":7200,                 // the zone that watermark was queried in
//!  "seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z","h":"3f0c1a92be44d7e5"}]}
//! ```
//!
//! `seen` is the price of correctness. JQL resolves to the minute, so the next
//! query must start *before* the watermark (interfaces §4.2: the two-minute
//! overlap is mandatory) -- and everything in that window comes back. Dropping
//! the records already delivered turns re-delivery from a contract violation
//! (battery clause 2: an idle incremental emits nothing) into an invisible
//! detail.
//!
//! # `h` is the identity; `u` is only the window (issue #345)
//!
//! An entry carries both and they do different jobs, which is why neither can
//! be dropped:
//!
//! * **`h`** -- [`knobas_source::fingerprint`] of the raw `/search` record -- is what
//!   [`JiraCursor::already_delivered`] matches on. It replaced `u` in that role
//!   in cursor version 2. `/search` reports `updated` to the *second* while an
//!   issue changes to the millisecond, so two changes inside one second shared
//!   a `(key, updated)` pair: with a run in between them, the second change was
//!   dropped before the sink and lost for ever, because `updated` never moves
//!   again on its own. Measured on Jira DC 10.3.24 (#345): `GET issue/{key}`
//!   reports `22:54:59.036` where `/search` reports `22:54:59.000` for the same
//!   instant, and a comment through knobas followed within the second by a
//!   reassignment in Jira reaches it, because every landed write syncs.
//! * **`u`** is what [`JiraCursor::advanced`] filters the set on, so it holds
//!   the overlap window and nothing more. A fingerprint carries no date, so without
//!   `u` there would be nothing to bound the set by and it would grow until the
//!   cap dropped entries the next query still returns.

use chrono::{DateTime, Utc};
use knobas_source::watermark::{self, Advanced, Bounds, Entry, Jql};

/// Bump when the envelope's shape changes: an unrecognised version reads as
/// "no cursor", which is a full sync.
///
/// **2** since issue #345 gave `seen` a fingerprint. That path is the whole upgrade
/// plan and it needs no migration: every stored version-1 cursor stops parsing
/// the moment this build runs, each affected source does one full sync, and it
/// comes back with a version-2 cursor. A full sync is the correct recovery
/// here for its own reason as well as for convenience -- a mirror that has been
/// running on version 1 may be holding rows whose last change was dropped by
/// the bug this version fixes, and nothing cheaper than re-reading the source
/// finds them.
///
/// **3** since #598 stores a classic project's epic as `epic: {"key": …}`
/// beside the record (the deputy's ruling of 2026-10-12 on #598). A stored
/// item only gains it when it is emitted again, and an unchanged issue never
/// is -- the lesson of #610 -- so the bump is what makes every upgraded
/// source re-read every issue once, the same full sync #345's bump made. Jira
/// is `full_sync_exhaustive`, so that one run re-emits the whole corpus.
///
/// **4** since #811 stores every ticket's sprints as `sprints: [...]` beside
/// the record, for #598's reason: an unchanged issue is never emitted again,
/// so without the bump no stored ticket would gain the key.
pub(crate) const CURSOR_VERSION: u8 = 4;

// The overlap, the daylight-saving widening and the seen cap are
// `knobas_source::watermark`'s (#1622), shared with Confluence. Interfaces
// §4.2: "**JQL time resolution is one minute**, so an exact-boundary
// watermark drops items. Re-delivery is free -- upserts are idempotent."

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct JiraCursor {
    pub v: u8,
    /// The newest `fields.updated` this source has delivered, or `None` for a
    /// source that has never delivered anything.
    pub updated_to: Option<DateTime<Utc>>,
    /// The server's UTC offset when that watermark was taken.
    pub tz_offset_secs: i32,
    /// What was delivered inside the overlap window, newest first.
    pub seen: Vec<Seen>,
    /// The Jira views the last run delivered (#877), each with the
    /// fingerprint of what it delivered: an unchanged view is not delivered
    /// again, and one no longer here is delivered as gone. Left out of the
    /// JSON when empty, so a source with no boards keeps the bytes it had --
    /// and an older cursor without the field reads as one with no views.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<SeenView>,
    /// The last run that read the boards (#1700, ruling
    /// D6). Optional with a default, so `CURSOR_VERSION` stays 4 and an
    /// envelope written before the field reads as "never read".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_read: Option<BoardRead>,
    /// The ticket scope the watermark was taken under (#2181): the JQL
    /// clause `projects` or `jql_filter` render to, empty for none. A run
    /// under another scope -- a project added, a filter changed -- walks the
    /// new one in full, because the watermark says nothing about tickets the
    /// old scope never asked for. Optional, so `CURSOR_VERSION` stays 4; an
    /// envelope written before the field has no recorded scope and matches
    /// none, so every source walks in full once after the upgrade, which also
    /// brings in what a scope widened before this build missed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// When the boards were last read, and under which *Boards and filters*
/// setting: a changed setting is read at once, not an hour on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BoardRead {
    pub at: DateTime<Utc>,
    pub scope: String,
}

/// One delivered Jira view: its item key and the fingerprint of its payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SeenView {
    pub k: String,
    pub h: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Seen {
    /// The issue key.
    pub k: String,
    /// Its `fields.updated`, which bounds the set to the overlap window.
    pub u: DateTime<Utc>,
    /// [`knobas_source::fingerprint`] of the raw record -- the identity.
    pub h: String,
}

/// The identity is the key and the fingerprint (#345); `u` only bounds the
/// window.
impl Entry for Seen {
    type Version = str;
    fn key(&self) -> &str {
        &self.k
    }
    fn stamp(&self) -> DateTime<Utc> {
        self.u
    }
    fn is_version(&self, fingerprint: &str) -> bool {
        self.h == fingerprint
    }
}

impl JiraCursor {
    /// A cursor for a source that has never delivered anything.
    pub(crate) fn empty(tz_offset_secs: i32) -> Self {
        Self {
            v: CURSOR_VERSION,
            updated_to: None,
            tz_offset_secs,
            seen: Vec::new(),
            views: Vec::new(),
            board_read: None,
            scope: None,
        }
    }

    /// This cursor's issue half, kept only when it was taken under `scope`
    /// (#2181): under another, the watermark and the overlap's `seen` are
    /// dropped so the run walks the scope in full. The views and the board
    /// stamp stay -- they carry their own setting.
    pub(crate) fn under(mut self, scope: &str) -> Self {
        if self.scope.as_deref() != Some(scope) {
            self.updated_to = None;
            self.seen.clear();
            self.scope = Some(scope.to_owned());
        }
        self
    }

    /// `None` for anything this version does not recognise -- which the caller
    /// reads as "full sync".
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let cursor: Self = serde_json::from_str(raw).ok()?;
        (cursor.v == CURSOR_VERSION).then_some(cursor)
    }

    pub(crate) fn encode(&self) -> String {
        // The one sanctioned `expect` in this crate: a cursor is four plain
        // fields with no map keys and no non-finite numbers, so the only way
        // `to_string` fails is a `Serialize` impl that cannot exist here.
        serde_json::to_string(self).expect("a cursor is plain data and always serializes")
    }

    /// The lower bound of the next `updated >=` clause: two minutes before
    /// the watermark, an hour more on the one run after the server's offset
    /// changed ([`knobas_source::watermark::since`]).
    ///
    /// # What the caller owes, in full
    ///
    /// Re-delivery here is harmless only if the run satisfies **both** of the
    /// following. Either one alone leaves an idle source oscillating, and the
    /// two fail in different places, so neither substitutes for the other:
    ///
    /// 1. **[`Self::advanced`] is handed the previous watermark** in its
    ///    bounds, so a re-delivered *older* item cannot drag the watermark
    ///    backwards.
    /// 2. **`advanced` is handed every pair in the window, skipped ones
    ///    included** -- see its own contract. A run that passes only what it
    ///    emitted drops the pairs it recognised out of `seen`, and they come
    ///    back unrecognised next run. The watermark holds perfectly while this
    ///    happens; it is `seen` that oscillates, and the source emits an item
    ///    on every poll forever.
    ///
    /// Condition 2 hides from the obvious test: full-sync-then-idle is stable
    /// under it, because a full sync skips nothing. It takes **one new issue
    /// and then an idle poll** to show.
    pub(crate) fn since(&self, current_offset_secs: i32) -> Option<DateTime<Utc>> {
        watermark::since(self.updated_to, self.tz_offset_secs, current_offset_secs)
    }

    /// Was this exact version of this issue already handed to the sink?
    ///
    /// **The fingerprint is the identity and the timestamp takes no part in the
    /// answer** (issue #345). Matching on `updated` as well was not a harmless
    /// belt-and-braces: it *was* the identity until cursor version 2, and it
    /// answered "yes" to a record that had changed since, whenever the change
    /// landed in the same second as the one before it. `/search` reports whole
    /// seconds, an issue changes in milliseconds, and a sync between the two
    /// changes is the ordinary case rather than a rare one -- every landed
    /// write triggers one (`knobas_app::sources::write_queue::after_landing`).
    /// The dropped change was then permanent: `updated` does not move again on
    /// its own, so no later incremental run reached it either.
    ///
    /// Two records with the same fingerprint are the same record, whatever their
    /// clocks said, so re-adding `u` to this comparison could only ever make it
    /// wrong again in the same direction.
    pub(crate) fn already_delivered(&self, key: &str, fingerprint: &str) -> bool {
        watermark::delivered(&self.seen, key, fingerprint)
    }

    /// The cursor for a run that emitted something.
    ///
    /// # `seen_in_window` is every record the run *saw*, not every one it sent
    ///
    /// The run must pass every [`Seen`] it observed inside the overlap window --
    /// **the ones it skipped as already-delivered just as much as the ones it
    /// pushed to the sink**. `seen` is a record of what the *window*
    /// contained, not of what crossed the SPI, and the two differ on exactly
    /// the items that make an idle poll idle.
    ///
    /// Pass only the emitted pairs and the cursor forgets, every run, whatever
    /// it recognised that run. Concretely, with the previous watermark
    /// correctly passed: run 3 emits PAY-3 and skips PAY-2, so
    /// PAY-2 leaves `seen`; run 4 no longer recognises PAY-2 and emits it,
    /// which pushes PAY-3 out; run 5 emits PAY-3 again. The watermark never
    /// moves and the source still emits an item on every poll, forever.
    ///
    /// This is why the parameter is not called `delivered`.
    ///
    /// # The watermark is held at the ceiling (#1622)
    ///
    /// `bounds` carries the previous watermark and the run-start ceiling:
    /// an issue edited during the walk is not where the watermark lands, and
    /// the watermark never moves back. The floor, the order and the cap are
    /// [`knobas_source::watermark::advanced`]'s, shared with Confluence.
    pub(crate) fn advanced(
        bounds: Bounds,
        newest: DateTime<Utc>,
        tz_offset_secs: i32,
        seen_in_window: &[Seen],
    ) -> Self {
        let Advanced { watermark, seen } =
            watermark::advanced::<Jql, Seen>(bounds, newest, tz_offset_secs, seen_in_window);
        Self {
            v: CURSOR_VERSION,
            updated_to: Some(watermark),
            tz_offset_secs,
            seen,
            views: Vec::new(),
            board_read: None,
            scope: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        crate::time::parse_jira_time(s).unwrap()
    }

    /// One record the way a run reports it: key, `updated`, fingerprint.
    ///
    /// The fingerprint stands in for [`knobas_source::fingerprint::of`] over that issue's raw
    /// record. Derived from the key alone, so two *different* issues differ and
    /// two reports of the same **unchanged** issue agree -- which is the case
    /// every test here but [`a_record_that_changed_within_one_second_is_not_the_one_already_delivered`]
    /// is about. [`edited`] is the same issue with a different record.
    fn saw(key: &str, updated: DateTime<Utc>) -> Seen {
        Seen {
            k: key.to_owned(),
            u: updated,
            h: h(key),
        }
    }

    fn edited(key: &str, updated: DateTime<Utc>) -> Seen {
        Seen {
            k: key.to_owned(),
            u: updated,
            h: format!("h-{key}-edited"),
        }
    }

    /// The fingerprint [`saw`] would report for `key`.
    fn h(key: &str) -> String {
        format!("h-{key}")
    }

    /// #1700: the board-read stamp is an optional field, so the version stays
    /// 4 and an envelope written before it still decodes, as never read.
    #[test]
    fn a_v4_envelope_without_the_board_read_stamp_decodes() {
        assert_eq!(CURSOR_VERSION, 4);
        let old = r#"{"v":4,"updated_to":null,"tz_offset_secs":0,"seen":[],"views":[{"k":"board-1","h":"x"}]}"#;
        let cursor = JiraCursor::parse(old).expect("a v4 envelope without the field reads");
        assert_eq!(cursor.board_read, None);
        assert_eq!(cursor.views.len(), 1);
        assert_eq!(cursor.encode(), old, "and re-encodes without inventing it");
    }

    /// #2181: a cursor keeps its issue half only under the scope it was
    /// taken under. One written before the field records none, so it matches
    /// no scope -- the "everything" scope included -- and the views survive.
    #[test]
    fn a_cursor_under_another_scope_drops_its_watermark_and_keeps_its_views() {
        let old = r#"{"v":4,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,"seen":[{"k":"PAY-1","u":"2026-08-22T11:48:00Z","h":"x"}],"views":[{"k":"board-1","h":"x"}]}"#;
        let legacy = JiraCursor::parse(old).expect("a v4 envelope without the field reads");
        assert_eq!(legacy.scope, None);
        let walked = legacy.clone().under("");
        assert_eq!((walked.updated_to, walked.seen.len()), (None, 0));
        assert_eq!(walked.views, legacy.views);
        assert_eq!(walked.scope.as_deref(), Some(""));

        let same = walked.clone().under("");
        assert_eq!(same, walked);
        let mut kept = legacy;
        kept.scope = Some("project in (\"PAY\")".to_owned());
        assert_eq!(kept.clone().under("project in (\"PAY\")"), kept);
        assert_eq!(kept.under("project in (\"PAY\", \"OPS\")").updated_to, None);
    }

    #[test]
    fn round_trips_through_its_json_envelope() {
        let c = JiraCursor::advanced(
            Bounds::default(),
            t("2026-08-22T11:48:00.000+0000"),
            7_200,
            &[saw("PAY-231", t("2026-08-22T11:48:00.000+0000"))],
        );
        let back = JiraCursor::parse(&c.encode()).unwrap();
        assert_eq!(back, c);
        // The stored bytes, literally: the shared watermark (#1622) moved the
        // arithmetic and none of the envelope, so no stored cursor re-syncs.
        assert_eq!(
            c.encode(),
            r#"{"v":4,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":7200,"seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z","h":"h-PAY-231"}]}"#
        );
    }

    /// Interfaces §4.1: "an unrecognised version means full sync". A corrupt or
    /// future cursor must not wedge a source -- it must re-sync.
    #[test]
    fn an_unreadable_cursor_means_full_sync() {
        assert!(JiraCursor::parse(r#"{"v":5,"updated_to":null}"#).is_none());
        // A *complete* envelope of a version this build does not know: the
        // version check has to be what rejects it, not a missing field.
        assert!(
            JiraCursor::parse(
                r#"{"v":5,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,"seen":[]}"#
            )
            .is_none()
        );
        // **A version-3 cursor, which is what every source stored before
        // #811.** Refusing it is the full sync that gives every stored ticket
        // its `sprints`.
        assert!(
            JiraCursor::parse(
                r#"{"v":3,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,
                    "seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z","h":"h-PAY-231"}]}"#
            )
            .is_none()
        );
        // **A version-2 cursor, which is what every source stored before
        // #598.** Well-formed in every field, fingerprints included, so only
        // the version refuses it -- and refusing it is what makes the first run
        // after the upgrade the full sync that stores every classic epic as
        // `epic` (the deputy's ruling of 2026-10-12 on #598).
        assert!(
            JiraCursor::parse(
                r#"{"v":2,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,
                    "seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z","h":"h-PAY-231"}]}"#
            )
            .is_none()
        );
        // **A version-1 cursor, which is what every source stored before issue
        // #345.** This is the whole upgrade path and it is load-bearing: the
        // envelope is complete and its `seen` entries are well-formed apart
        // from the missing `h`, so a build that read it leniently would carry
        // the old identity forward and keep the bug. Refusing it is what makes
        // the first run after the upgrade a full sync.
        assert!(
            JiraCursor::parse(
                r#"{"v":1,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,
                    "seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z"}]}"#
            )
            .is_none()
        );
        assert!(JiraCursor::parse("tidewater-v1").is_none());
        assert!(JiraCursor::parse("").is_none());
    }

    /// The two-minute overlap, and why it is not optional.
    #[test]
    fn the_query_starts_two_minutes_before_the_watermark() {
        let c = JiraCursor::advanced(
            Bounds::default(),
            t("2026-08-22T11:48:00.000+0000"),
            7_200,
            &[],
        );
        assert_eq!(c.since(7_200), Some(t("2026-08-22T11:46:00.000+0000")));
    }

    /// A daylight-saving change moves the zone JQL literals are read in. The
    /// recorded offset detects it, and that one run widens the overlap past the
    /// shift instead of skipping an hour of issues.
    #[test]
    fn an_offset_change_widens_the_overlap_once() {
        let c = JiraCursor::advanced(
            Bounds::default(),
            t("2026-10-25T01:30:00.000+0000"),
            7_200,
            &[],
        );
        assert_eq!(c.since(3_600), Some(t("2026-10-25T00:25:00.000+0000")));
    }

    #[test]
    fn a_cursor_with_no_watermark_asks_for_everything() {
        assert_eq!(JiraCursor::empty(0).since(0), None);
    }

    /// The overlap re-fetches; the seen set is what keeps the *sink* from
    /// seeing it twice, which is what battery clause 2 asserts.
    #[test]
    fn issues_already_delivered_at_the_watermark_are_recognised() {
        let updated = t("2026-08-22T11:48:00.000+0000");
        let c = JiraCursor::advanced(Bounds::default(), updated, 0, &[saw("PAY-231", updated)]);
        assert!(c.already_delivered("PAY-231", &h("PAY-231")));
        // Edited since: a different record, so a real change.
        assert!(!c.already_delivered("PAY-231", &edited("PAY-231", updated).h));
        // Another issue, even one whose record happened to fingerprint the same.
        assert!(!c.already_delivered("PAY-240", &h("PAY-231")));
    }

    /// **A change that landed in the same second as the one already delivered
    /// is still a change** (issue #345).
    ///
    /// This is the whole of what cursor version 2 buys, and the old identity
    /// could not express it: `/search` reports `updated` to the second while an
    /// issue changes in milliseconds, so `(key, updated)` said *already
    /// delivered* to a record that had changed since. The run dropped it before
    /// the sink and no later run reached it either -- `updated` does not move
    /// again on its own -- so the change was lost until a backfill.
    ///
    /// The timestamp is deliberately **identical** in both halves, because a
    /// version that still consulted it would pass a test that let the stamp
    /// move.
    #[test]
    fn a_record_that_changed_within_one_second_is_not_the_one_already_delivered() {
        let one_second = t("2026-09-03T22:54:59.000+0000");
        let c = JiraCursor::advanced(
            Bounds::default(),
            one_second,
            0,
            &[saw("PAY-240", one_second)],
        );
        assert!(
            c.already_delivered("PAY-240", &h("PAY-240")),
            "the unchanged record is still recognised -- battery clause 2"
        );
        assert!(
            !c.already_delivered("PAY-240", &edited("PAY-240", one_second).h),
            "the reassignment landed 88 ms after the comment and shares its second; \
             recognising it here is how it was lost for ever"
        );
    }

    /// Only the overlap window needs remembering -- everything older can never
    /// come back from the next query.
    #[test]
    fn the_seen_set_covers_the_overlap_and_nothing_else() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        let c = JiraCursor::advanced(
            Bounds::default(),
            watermark,
            0,
            &[
                saw("PAY-231", watermark),
                saw("PAY-240", t("2026-08-22T11:47:10.000+0000")),
                saw("PAY-219", t("2026-08-22T09:00:00.000+0000")),
            ],
        );
        let keys: Vec<&str> = c.seen.iter().map(|s| s.k.as_str()).collect();
        assert_eq!(keys, vec!["PAY-231", "PAY-240"]);
    }

    /// Two runs that delivered the same set must encode the same bytes, in
    /// whatever order the pages happened to arrive -- the engine compares
    /// cursors as strings, so an order that tracked arrival order would make
    /// every idle poll look like progress.
    ///
    /// The timestamps here are deliberately **distinct and counter to key
    /// order**: an earlier version of this test gave all three entries the
    /// same timestamp, which the sort's tie-break already resolved by key, so
    /// it could not tell newest-first ordering from no ordering at all.
    #[test]
    fn the_seen_set_is_ordered_newest_first_however_it_arrived() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        // PAY-0 is the oldest and PAY-2 the newest, so newest-first is the
        // reverse of key order and the two cannot be confused.
        let pairs = |order: [i64; 3]| {
            order.map(|i| {
                saw(
                    &format!("PAY-{i}"),
                    watermark - chrono::Duration::seconds(2 - i),
                )
            })
        };
        let encode = |order: [i64; 3]| {
            JiraCursor::advanced(Bounds::default(), watermark, 0, &pairs(order)).encode()
        };
        assert_eq!(encode([2, 0, 1]), encode([0, 1, 2]));
        assert_eq!(encode([1, 2, 0]), encode([0, 1, 2]));

        let c = JiraCursor::advanced(Bounds::default(), watermark, 0, &pairs([2, 0, 1]));
        let keys: Vec<&str> = c.seen.iter().map(|s| s.k.as_str()).collect();
        assert_eq!(keys, vec!["PAY-2", "PAY-1", "PAY-0"]);
    }

    /// A bulk edit stamps many issues with the same `updated`, so the ordering
    /// cannot rest on the timestamp alone: without the key tie-break, `sort_by`
    /// is stable and equal-timestamp entries would keep the order the pages
    /// arrived in -- different bytes for the same delivered set.
    #[test]
    fn entries_sharing_a_timestamp_are_still_ordered_deterministically() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        let encode = |order: [&str; 3]| {
            JiraCursor::advanced(
                Bounds::default(),
                watermark,
                0,
                &order.map(|k| saw(k, watermark)),
            )
            .encode()
        };
        assert_eq!(
            encode(["PAY-240", "PAY-219", "PAY-231"]),
            encode(["PAY-219", "PAY-231", "PAY-240"])
        );
        assert_eq!(
            encode(["PAY-231", "PAY-240", "PAY-219"]),
            encode(["PAY-219", "PAY-231", "PAY-240"])
        );
    }

    /// The instant Jira will read this cursor's next `updated >=` literal as.
    ///
    /// Deliberately goes *through* the JQL rendering the query uses and parses
    /// the literal back by hand, rather than calling `Jql::floor` or
    /// recomputing a constant: the property under test is that the `seen`
    /// filter and the rendered query agree, so a test that recomputed the
    /// floor its own way could agree with neither and still pass.
    fn query_floor(c: &JiraCursor, offset: i32) -> DateTime<Utc> {
        let literal = <Jql as knobas_source::watermark::MinuteLiteral>::render(
            c.since(offset).expect("this cursor has a watermark"),
            offset,
        );
        let zone = chrono::FixedOffset::east_opt(offset).expect("a real offset");
        chrono::NaiveDateTime::parse_from_str(&literal, "%Y-%m-%d %H:%M")
            .expect("a literal this crate rendered")
            .and_local_timezone(zone)
            .single()
            .expect("a fixed offset has no ambiguous local times")
            .with_timezone(&Utc)
    }

    /// **Everything the next query returns and this run already delivered must
    /// be recognised.** Otherwise the overlap re-emits an item the sink has
    /// seen, and -- because the run takes its watermark from what it emitted --
    /// the watermark moves *backwards* onto that older item. The next run
    /// pushes it forward again, and an idle source alternates between two
    /// cursors on every poll, forever.
    ///
    /// The gap this pins was real: `advanced` filtered on
    /// `watermark - 2min` at full precision while the query truncates to the
    /// minute, so up to 59 s of delivered pairs sat inside the window and
    /// outside `seen`. A watermark carrying seconds is what exposes it, which
    /// is why this test does not use a round minute.
    #[test]
    fn nothing_the_next_query_returns_is_forgotten() {
        let watermark = t("2026-08-22T11:48:30.000+0000");
        let offset = 7_200;
        let delivered = [
            // Inside the truncated minute the literal will name, outside the
            // un-truncated arithmetic. This is the one that used to be lost.
            saw("PAY-1", t("2026-08-22T11:46:10.000+0000")),
            saw("PAY-2", t("2026-08-22T11:48:30.000+0000")),
            // Genuinely older than the window; the query will not return it.
            saw("PAY-0", t("2026-08-22T11:40:00.000+0000")),
        ];
        let c = JiraCursor::advanced(Bounds::default(), watermark, offset, &delivered);
        let floor = query_floor(&c, offset);

        for record in &delivered {
            let (key, updated) = (&record.k, record.u);
            if updated >= floor {
                assert!(
                    c.already_delivered(key, &record.h),
                    "{key} @ {updated} is inside the next query's window (>= {floor}) \
                     but is not in seen -- it will be re-emitted and drag the watermark back"
                );
            }
        }
        // The reviewer's demonstration, named outright.
        assert!(c.already_delivered("PAY-1", &h("PAY-1")));
        // And the set is still bounded: what the query cannot return is dropped.
        assert!(!c.already_delivered("PAY-0", &h("PAY-0")));
    }

    /// The same invariant swept across zones and second-offsets, because the
    /// truncation interacts with both.
    #[test]
    fn nothing_the_next_query_returns_is_forgotten_in_any_zone() {
        for hours in [-5, 0, 2, 5, 14] {
            let offset = hours * 3_600;
            for second in [0, 1, 17, 30, 59] {
                let watermark = t(&format!("2026-08-22T11:48:{second:02}.000+0000"));
                let delivered: Vec<Seen> = (0..150)
                    .map(|i| {
                        saw(
                            &format!("PAY-{i}"),
                            watermark - chrono::Duration::seconds(i),
                        )
                    })
                    .collect();
                let c = JiraCursor::advanced(Bounds::default(), watermark, offset, &delivered);
                let floor = query_floor(&c, offset);
                for record in &delivered {
                    let (key, updated) = (&record.k, record.u);
                    if updated >= floor {
                        assert!(
                            c.already_delivered(key, &record.h),
                            "offset {hours}h second {second}: {key} @ {updated} \
                             is in the window (>= {floor}) but not in seen"
                        );
                    }
                }
            }
        }
    }

    /// What a run holding `cursor` would emit, given what its query returned.
    ///
    /// The sync loop is task 5's; this is only the three lines of it that
    /// [`JiraCursor::already_delivered`] already decides, which is enough to
    /// show what [`JiraCursor::advanced`]'s input contract buys.
    fn would_emit<'a>(cursor: &JiraCursor, returned: &'a [Seen]) -> Vec<&'a str> {
        returned
            .iter()
            .filter(|s| !cursor.already_delivered(&s.k, &s.h))
            .map(|s| s.k.as_str())
            .collect()
    }

    /// **`advanced` must be given the pairs the run *skipped*, not just the
    /// ones it emitted.** `seen` records what the window contained; a run that
    /// passes only what it pushed to the sink drops everything it recognised,
    /// and those items come back unrecognised on the very next poll.
    ///
    /// The `max()` watermark does not save it -- the watermark here never moves
    /// at all. It is `seen` that oscillates, so the failure is invisible to
    /// anything watching the cursor's timestamp.
    ///
    /// Note what it takes to see this: a full sync skips nothing, so
    /// full-sync-then-idle is stable and a battery that certifies only that
    /// sequence passes. It needs **one new issue and then an idle poll**.
    #[test]
    fn seen_must_record_the_skipped_pairs_or_an_idle_source_never_settles() {
        let older = t("2026-08-22T11:47:00.000+0000");
        let newer = t("2026-08-22T11:48:00.000+0000");
        // What the next query returns: both are inside the overlap window.
        let window = [saw("PAY-2", older), saw("PAY-3", newer)];

        // Run 3 emitted PAY-3 and skipped PAY-2, having recognised it from run
        // 2. Contract honoured: `advanced` is handed both.
        let honoured = JiraCursor::advanced(Bounds::default(), newer, 0, &window);
        assert!(
            would_emit(&honoured, &window).is_empty(),
            "an idle poll must emit nothing"
        );
        // And it stays settled: the same window yields the same cursor, so the
        // engine reads "same cursor, no items" and writes no activity line.
        assert_eq!(
            JiraCursor::advanced(Bounds::default(), newer, 0, &window).encode(),
            honoured.encode()
        );

        // The same run passing only what it *emitted* -- the reading the old
        // parameter name invited.
        let run3 = JiraCursor::advanced(Bounds::default(), newer, 0, &window[1..]);
        assert_eq!(
            would_emit(&run3, &window),
            vec!["PAY-2"],
            "the skipped pair fell out of seen and comes back unrecognised"
        );

        // It does not converge. Run 4 emits PAY-2; its watermark is still
        // max(newer, older) == newer, so nothing looks wrong -- but its seen
        // now holds only PAY-2, so run 5 emits PAY-3, and so on forever.
        let run4 = JiraCursor::advanced(Bounds::default(), newer, 0, &[saw("PAY-2", older)]);
        assert_eq!(run4.updated_to, Some(newer), "the watermark never moved");
        assert_eq!(would_emit(&run4, &window), vec!["PAY-3"]);
    }

    /// A cursor is stored in a text column and read on every run; an unbounded
    /// seen list would grow with a mass edit of ten thousand issues.
    #[test]
    fn the_seen_set_is_capped_at_the_newest_entries() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        let delivered: Vec<Seen> = (0..knobas_source::watermark::SEEN_CAP + 50)
            .map(|i| {
                saw(
                    &format!("PAY-{i}"),
                    watermark - chrono::Duration::milliseconds(i as i64),
                )
            })
            .collect();
        let c = JiraCursor::advanced(Bounds::default(), watermark, 0, &delivered);
        assert_eq!(c.seen.len(), knobas_source::watermark::SEEN_CAP);
        // The newest survive the cap: they are the ones the next overlap query
        // will hand back again.
        assert!(c.already_delivered("PAY-0", &h("PAY-0")));
        let oldest = &delivered[knobas_source::watermark::SEEN_CAP + 49];
        assert!(!c.already_delivered(&oldest.k, &oldest.h));
    }

    /// Battery clause 2: a run that emitted nothing hands back the cursor it
    /// was given, byte-identically. The engine reads "same cursor, no items" as
    /// "nothing happened" and writes no activity line -- an adapter that
    /// re-encodes an idle poll turns a five-minute schedule into 288 log lines
    /// a day. Re-encoding must therefore be a no-op at the byte level.
    #[test]
    fn re_encoding_an_unchanged_cursor_is_byte_identical() {
        let encoded = JiraCursor::advanced(
            Bounds::default(),
            t("2026-08-22T11:48:00.000+0000"),
            7_200,
            &[
                saw("PAY-231", t("2026-08-22T11:48:00.000+0000")),
                saw("PAY-240", t("2026-08-22T11:47:10.000+0000")),
            ],
        )
        .encode();
        assert_eq!(JiraCursor::parse(&encoded).unwrap().encode(), encoded);
    }
}
