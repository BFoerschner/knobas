//! The incremental position: `{"v":1,…}`, opaque to everything but this crate.
//!
//! ```text
//! {"v":2,
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
//! * **`h`** -- [`crate::digest`] of the raw `/search` record -- is what
//!   [`JiraCursor::already_delivered`] matches on. It replaced `u` in that role
//!   in cursor version 2. `/search` reports `updated` to the *second* while an
//!   issue changes to the millisecond, so two changes inside one second shared
//!   a `(key, updated)` pair: with a run in between them, the second change was
//!   dropped before the sink and lost for ever, because `updated` never moves
//!   again on its own. Measured on Jira DC 10.3.24; `crate::digest`'s own docs
//!   carry the numbers and the everyday sequence that reaches it.
//! * **`u`** is what [`JiraCursor::advanced`] filters the set on, so it holds
//!   the overlap window and nothing more. A digest carries no date, so without
//!   `u` there would be nothing to bound the set by and it would grow until the
//!   cap dropped entries the next query still returns.

use chrono::{DateTime, Utc};

/// Bump when the envelope's shape changes: an unrecognised version reads as
/// "no cursor", which is a full sync.
///
/// **2** since issue #345 gave `seen` a digest. That path is the whole upgrade
/// plan and it needs no migration: every stored version-1 cursor stops parsing
/// the moment this build runs, each affected source does one full sync, and it
/// comes back with a version-2 cursor. A full sync is the correct recovery
/// here for its own reason as well as for convenience -- a mirror that has been
/// running on version 1 may be holding rows whose last change was dropped by
/// the bug this version fixes, and nothing cheaper than re-reading the source
/// finds them.
pub(crate) const CURSOR_VERSION: u8 = 2;

/// Interfaces §4.2: "**JQL time resolution is one minute**, so an
/// exact-boundary watermark drops items. Re-delivery is free -- upserts are
/// idempotent."
const OVERLAP_MINUTES: i64 = 2;

/// Used for the one run after the server's UTC offset changed (a daylight
/// saving shift is at most an hour), so no item falls in the seam.
const DST_OVERLAP_MINUTES: i64 = 65;

/// How many `(key, updated)` pairs the envelope remembers. Beyond this a mass
/// edit inside one overlap window may be delivered twice -- upserts are
/// idempotent, so the cost is bandwidth, not correctness.
pub(crate) const SEEN_CAP: usize = 500;

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
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Seen {
    /// The issue key.
    pub k: String,
    /// Its `fields.updated`, which bounds the set to the overlap window.
    pub u: DateTime<Utc>,
    /// [`crate::digest`] of the raw record -- the identity.
    pub h: String,
}

impl JiraCursor {
    /// A cursor for a source that has never delivered anything.
    pub(crate) fn empty(tz_offset_secs: i32) -> Self {
        Self {
            v: CURSOR_VERSION,
            updated_to: None,
            tz_offset_secs,
            seen: Vec::new(),
        }
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

    /// The lower bound of the next `updated >=` clause.
    ///
    /// On the one run after the server's offset changed this reaches further
    /// back than the window [`Self::advanced`] recorded `seen` for, so that run
    /// re-delivers what falls in between. That is deliberate -- widening is how
    /// no item falls in the daylight-saving seam, and `advanced` cannot know an
    /// offset that has not been observed yet.
    ///
    /// # What the caller owes, in full
    ///
    /// Re-delivery here is harmless only if the run satisfies **both** of the
    /// following. Either one alone leaves an idle source oscillating, and the
    /// two fail in different places, so neither substitutes for the other:
    ///
    /// 1. **The new watermark is `max(previous, newest emitted)`.** Otherwise a
    ///    re-delivered *older* item drags the watermark backwards -- the same
    ///    oscillation the minute-flooring in [`Self::advanced`] exists to
    ///    prevent, arriving by a different door.
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
        let watermark = self.updated_to?;
        let minutes = if current_offset_secs == self.tz_offset_secs {
            OVERLAP_MINUTES
        } else {
            DST_OVERLAP_MINUTES
        };
        Some(watermark - chrono::Duration::minutes(minutes))
    }

    /// Was this exact version of this issue already handed to the sink?
    ///
    /// **The digest is the identity and the timestamp takes no part in the
    /// answer** (issue #345). Matching on `updated` as well was not a harmless
    /// belt-and-braces: it *was* the identity until cursor version 2, and it
    /// answered "yes" to a record that had changed since, whenever the change
    /// landed in the same second as the one before it. `/search` reports whole
    /// seconds, an issue changes in milliseconds, and a sync between the two
    /// changes is the ordinary case rather than a rare one -- every landed
    /// write triggers one (`knobas_app::sources::write_queue`'s `refresh`).
    /// The dropped change was then permanent: `updated` does not move again on
    /// its own, so no later incremental run reached it either.
    ///
    /// Two records with the same digest are the same record, whatever their
    /// clocks said, so re-adding `u` to this comparison could only ever make it
    /// wrong again in the same direction.
    pub(crate) fn already_delivered(&self, key: &str, digest: &str) -> bool {
        self.seen.iter().any(|s| s.k == key && s.h == digest)
    }

    /// The cursor for a run that emitted something.
    ///
    /// # `seen_in_window` is every record the run *saw*, not every one it sent
    ///
    /// The run must pass every `(key, updated, digest)` it observed inside the
    /// overlap window -- **the ones it skipped as already-delivered just as much as the
    /// ones it pushed to the sink**. `seen` is a record of what the *window*
    /// contained, not of what crossed the SPI, and the two differ on exactly
    /// the items that make an idle poll idle.
    ///
    /// Pass only the emitted pairs and the cursor forgets, every run, whatever
    /// it recognised that run. Concretely, with the `max()` watermark from
    /// [`Self::since`] correctly applied: run 3 emits PAY-3 and skips PAY-2, so
    /// PAY-2 leaves `seen`; run 4 no longer recognises PAY-2 and emits it,
    /// which pushes PAY-3 out; run 5 emits PAY-3 again. The watermark never
    /// moves and the source still emits an item on every poll, forever.
    ///
    /// This is why the parameter is not called `delivered`.
    pub(crate) fn advanced(
        watermark: DateTime<Utc>,
        tz_offset_secs: i32,
        seen_in_window: &[(String, DateTime<Utc>, String)],
    ) -> Self {
        // The band the next query will really return, not the band the
        // arithmetic suggests. `since` subtracts whole minutes, but the literal
        // it is rendered into has minute resolution and truncates *downwards*,
        // so the query's true lower bound is up to 59 s earlier than
        // `watermark - OVERLAP_MINUTES`. Filtering `seen` on the un-truncated
        // value drops every pair delivered in that band while the query keeps
        // returning them: run N+1 re-emits one, its watermark moves *backwards*
        // to that item, run N+2 moves it forward again, and an idle poll
        // oscillates between two cursors forever -- battery clause 2 failing in
        // steady state, not at a DST edge.
        //
        // Asking `jql_floor` rather than truncating here keeps the two
        // definitions from drifting apart again: it renders the literal and
        // reads it back, so the floor is the query's meaning by construction.
        let floor = crate::time::jql_floor(
            watermark - chrono::Duration::minutes(OVERLAP_MINUTES),
            tz_offset_secs,
        );
        let mut seen: Vec<Seen> = seen_in_window
            .iter()
            .filter(|(_, u, _)| *u >= floor)
            .map(|(k, u, h)| Seen {
                k: k.clone(),
                u: *u,
                h: h.clone(),
            })
            .collect();
        // Newest first, so the cap drops the entries least likely to come back,
        // with the key breaking ties. A key appears at most once, so this is a
        // *total* order on the entries -- which is the whole of what makes the
        // same delivered set encode to the same bytes however the pages
        // happened to arrive. The engine compares cursors as strings, so that
        // property is what lets an idle poll be recognised as one.
        //
        // A second pass sorting by key alone used to follow this one, claiming
        // to be what stabilised the bytes. It was not: it was redundant, and a
        // mutation check found no test that could tell whether it was there.
        seen.sort_by(|a, b| b.u.cmp(&a.u).then_with(|| a.k.cmp(&b.k)));
        seen.truncate(SEEN_CAP);
        Self {
            v: CURSOR_VERSION,
            updated_to: Some(watermark),
            tz_offset_secs,
            seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        crate::time::parse_jira_time(s).unwrap()
    }

    /// One record the way a run reports it: `(key, updated, digest)`.
    ///
    /// The digest stands in for [`crate::digest::of`] over that issue's raw
    /// record. Derived from the key alone, so two *different* issues differ and
    /// two reports of the same **unchanged** issue agree -- which is the case
    /// every test here but [`a_record_that_changed_within_one_second_is_not_the_one_already_delivered`]
    /// is about. [`edited`] is the same issue with a different record.
    fn saw(key: &str, updated: DateTime<Utc>) -> (String, DateTime<Utc>, String) {
        (key.to_owned(), updated, format!("h-{key}"))
    }

    fn edited(key: &str, updated: DateTime<Utc>) -> (String, DateTime<Utc>, String) {
        (key.to_owned(), updated, format!("h-{key}-edited"))
    }

    /// The digest [`saw`] would report for `key`.
    fn h(key: &str) -> String {
        format!("h-{key}")
    }

    #[test]
    fn round_trips_through_its_json_envelope() {
        let c = JiraCursor::advanced(
            t("2026-08-22T11:48:00.000+0000"),
            7_200,
            &[saw("PAY-231", t("2026-08-22T11:48:00.000+0000"))],
        );
        let back = JiraCursor::parse(&c.encode()).unwrap();
        assert_eq!(back, c);
        assert!(c.encode().starts_with(r#"{"v":2,"#), "{}", c.encode());
    }

    /// Interfaces §4.1: "an unrecognised version means full sync". A corrupt or
    /// future cursor must not wedge a source -- it must re-sync.
    #[test]
    fn an_unreadable_cursor_means_full_sync() {
        assert!(JiraCursor::parse(r#"{"v":3,"updated_to":null}"#).is_none());
        // A *complete* envelope of a version this build does not know: the
        // version check has to be what rejects it, not a missing field.
        assert!(
            JiraCursor::parse(
                r#"{"v":3,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,"seen":[]}"#
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
        let c = JiraCursor::advanced(t("2026-08-22T11:48:00.000+0000"), 7_200, &[]);
        assert_eq!(c.since(7_200), Some(t("2026-08-22T11:46:00.000+0000")));
    }

    /// A daylight-saving change moves the zone JQL literals are read in. The
    /// recorded offset detects it, and that one run widens the overlap past the
    /// shift instead of skipping an hour of issues.
    #[test]
    fn an_offset_change_widens_the_overlap_once() {
        let c = JiraCursor::advanced(t("2026-10-25T01:30:00.000+0000"), 7_200, &[]);
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
        let c = JiraCursor::advanced(updated, 0, &[saw("PAY-231", updated)]);
        assert!(c.already_delivered("PAY-231", &h("PAY-231")));
        // Edited since: a different record, so a real change.
        assert!(!c.already_delivered("PAY-231", &edited("PAY-231", updated).2));
        // Another issue, even one whose record happened to digest the same.
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
        let c = JiraCursor::advanced(one_second, 0, &[saw("PAY-240", one_second)]);
        assert!(
            c.already_delivered("PAY-240", &h("PAY-240")),
            "the unchanged record is still recognised -- battery clause 2"
        );
        assert!(
            !c.already_delivered("PAY-240", &edited("PAY-240", one_second).2),
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
        let encode = |order: [i64; 3]| JiraCursor::advanced(watermark, 0, &pairs(order)).encode();
        assert_eq!(encode([2, 0, 1]), encode([0, 1, 2]));
        assert_eq!(encode([1, 2, 0]), encode([0, 1, 2]));

        let c = JiraCursor::advanced(watermark, 0, &pairs([2, 0, 1]));
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
            JiraCursor::advanced(watermark, 0, &order.map(|k| saw(k, watermark))).encode()
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
    /// Deliberately goes *through* [`crate::time::format_jql_time`] and parses
    /// the literal back by hand, rather than calling `jql_floor` or
    /// recomputing a constant: the property under test is that the `seen`
    /// filter and the rendered query agree, so a test that recomputed the
    /// floor its own way could agree with neither and still pass.
    fn query_floor(c: &JiraCursor, offset: i32) -> DateTime<Utc> {
        let literal = crate::time::format_jql_time(
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
        let c = JiraCursor::advanced(watermark, offset, &delivered);
        let floor = query_floor(&c, offset);

        for (key, updated, digest) in &delivered {
            if *updated >= floor {
                assert!(
                    c.already_delivered(key, digest),
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
                let delivered: Vec<(String, DateTime<Utc>, String)> = (0..150)
                    .map(|i| {
                        saw(
                            &format!("PAY-{i}"),
                            watermark - chrono::Duration::seconds(i),
                        )
                    })
                    .collect();
                let c = JiraCursor::advanced(watermark, offset, &delivered);
                let floor = query_floor(&c, offset);
                for (key, updated, digest) in &delivered {
                    if *updated >= floor {
                        assert!(
                            c.already_delivered(key, digest),
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
    fn would_emit<'a>(
        cursor: &JiraCursor,
        returned: &'a [(String, DateTime<Utc>, String)],
    ) -> Vec<&'a str> {
        returned
            .iter()
            .filter(|(key, _, digest)| !cursor.already_delivered(key, digest))
            .map(|(key, _, _)| key.as_str())
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
        let honoured = JiraCursor::advanced(newer, 0, &window);
        assert!(
            would_emit(&honoured, &window).is_empty(),
            "an idle poll must emit nothing"
        );
        // And it stays settled: the same window yields the same cursor, so the
        // engine reads "same cursor, no items" and writes no activity line.
        assert_eq!(
            JiraCursor::advanced(newer, 0, &window).encode(),
            honoured.encode()
        );

        // The same run passing only what it *emitted* -- the reading the old
        // parameter name invited.
        let run3 = JiraCursor::advanced(newer, 0, &window[1..]);
        assert_eq!(
            would_emit(&run3, &window),
            vec!["PAY-2"],
            "the skipped pair fell out of seen and comes back unrecognised"
        );

        // It does not converge. Run 4 emits PAY-2; its watermark is still
        // max(newer, older) == newer, so nothing looks wrong -- but its seen
        // now holds only PAY-2, so run 5 emits PAY-3, and so on forever.
        let run4 = JiraCursor::advanced(newer, 0, &[saw("PAY-2", older)]);
        assert_eq!(run4.updated_to, Some(newer), "the watermark never moved");
        assert_eq!(would_emit(&run4, &window), vec!["PAY-3"]);
    }

    /// A cursor is stored in a text column and read on every run; an unbounded
    /// seen list would grow with a mass edit of ten thousand issues.
    #[test]
    fn the_seen_set_is_capped_at_the_newest_entries() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        let delivered: Vec<(String, DateTime<Utc>, String)> = (0..SEEN_CAP + 50)
            .map(|i| {
                saw(
                    &format!("PAY-{i}"),
                    watermark - chrono::Duration::milliseconds(i as i64),
                )
            })
            .collect();
        let c = JiraCursor::advanced(watermark, 0, &delivered);
        assert_eq!(c.seen.len(), SEEN_CAP);
        // The newest survive the cap: they are the ones the next overlap query
        // will hand back again.
        assert!(c.already_delivered("PAY-0", &h("PAY-0")));
        let oldest = &delivered[SEEN_CAP + 49];
        assert!(!c.already_delivered(&oldest.0, &oldest.2));
    }

    /// Battery clause 2: a run that emitted nothing hands back the cursor it
    /// was given, byte-identically. The engine reads "same cursor, no items" as
    /// "nothing happened" and writes no activity line -- an adapter that
    /// re-encodes an idle poll turns a five-minute schedule into 288 log lines
    /// a day. Re-encoding must therefore be a no-op at the byte level.
    #[test]
    fn re_encoding_an_unchanged_cursor_is_byte_identical() {
        let encoded = JiraCursor::advanced(
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
