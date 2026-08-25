//! The incremental position: `{"v":1,…}`, opaque to everything but this crate.
//!
//! ```text
//! {"v":1,
//!  "updated_to":"2026-08-22T11:48:00Z",   // the newest `updated` delivered
//!  "tz_offset_secs":7200,                 // the zone that watermark was queried in
//!  "seen":[{"k":"PAY-231","u":"2026-08-22T11:48:00Z"}]}
//! ```
//!
//! `seen` is the price of correctness. JQL resolves to the minute, so the next
//! query must start *before* the watermark (interfaces §4.2: the two-minute
//! overlap is mandatory) -- and everything in that window comes back. Dropping
//! the pairs already delivered turns re-delivery from a contract violation
//! (battery clause 2: an idle incremental emits nothing) into an invisible
//! detail.

use chrono::{DateTime, Utc};

/// Bump when the envelope's shape changes: an unrecognised version reads as
/// "no cursor", which is a full sync.
pub(crate) const CURSOR_VERSION: u8 = 1;

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
    pub k: String,
    pub u: DateTime<Utc>,
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
    /// offset that has not been observed yet. It is safe only because the run
    /// takes its new watermark as `max(previous, newest delivered)`: without
    /// that, a re-delivered older item would drag the watermark backwards, the
    /// same oscillation the minute-flooring above exists to prevent.
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
    /// An issue edited twice within the same second is missed here; the next
    /// distinct edit brings it back, and the item was already upserted once.
    pub(crate) fn already_delivered(&self, key: &str, updated: Option<DateTime<Utc>>) -> bool {
        let Some(updated) = updated else {
            return false;
        };
        self.seen.iter().any(|s| s.u == updated && s.k == key)
    }

    /// The cursor for a run that delivered something.
    pub(crate) fn advanced(
        watermark: DateTime<Utc>,
        tz_offset_secs: i32,
        delivered: &[(String, DateTime<Utc>)],
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
        let mut seen: Vec<Seen> = delivered
            .iter()
            .filter(|(_, u)| *u >= floor)
            .map(|(k, u)| Seen {
                k: k.clone(),
                u: *u,
            })
            .collect();
        // Newest first, so the cap drops the entries least likely to come back,
        // with the key breaking ties. A key appears at most once, so this is a
        // *total* order on the pairs -- which is the whole of what makes the
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

    #[test]
    fn round_trips_through_its_json_envelope() {
        let c = JiraCursor::advanced(
            t("2026-08-22T11:48:00.000+0000"),
            7_200,
            &[("PAY-231".to_owned(), t("2026-08-22T11:48:00.000+0000"))],
        );
        let back = JiraCursor::parse(&c.encode()).unwrap();
        assert_eq!(back, c);
        assert!(c.encode().starts_with(r#"{"v":1,"#), "{}", c.encode());
    }

    /// Interfaces §4.1: "an unrecognised version means full sync". A corrupt or
    /// future cursor must not wedge a source -- it must re-sync.
    #[test]
    fn an_unreadable_cursor_means_full_sync() {
        assert!(JiraCursor::parse(r#"{"v":2,"updated_to":null}"#).is_none());
        // A *complete* envelope of a version this build does not know: the
        // version check has to be what rejects it, not a missing field.
        assert!(
            JiraCursor::parse(
                r#"{"v":2,"updated_to":"2026-08-22T11:48:00Z","tz_offset_secs":0,"seen":[]}"#
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
        let c = JiraCursor::advanced(updated, 0, &[("PAY-231".to_owned(), updated)]);
        assert!(c.already_delivered("PAY-231", Some(updated)));
        // Updated again since: a different timestamp, so it is a real change.
        assert!(!c.already_delivered("PAY-231", Some(t("2026-08-22T11:49:00.000+0000"))));
        assert!(!c.already_delivered("PAY-240", Some(updated)));
        assert!(!c.already_delivered("PAY-231", None));
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
                ("PAY-231".to_owned(), watermark),
                ("PAY-240".to_owned(), t("2026-08-22T11:47:10.000+0000")),
                ("PAY-219".to_owned(), t("2026-08-22T09:00:00.000+0000")),
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
                (
                    format!("PAY-{i}"),
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
            JiraCursor::advanced(watermark, 0, &order.map(|k| (k.to_owned(), watermark))).encode()
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
            ("PAY-1".to_owned(), t("2026-08-22T11:46:10.000+0000")),
            ("PAY-2".to_owned(), t("2026-08-22T11:48:30.000+0000")),
            // Genuinely older than the window; the query will not return it.
            ("PAY-0".to_owned(), t("2026-08-22T11:40:00.000+0000")),
        ];
        let c = JiraCursor::advanced(watermark, offset, &delivered);
        let floor = query_floor(&c, offset);

        for (key, updated) in &delivered {
            if *updated >= floor {
                assert!(
                    c.already_delivered(key, Some(*updated)),
                    "{key} @ {updated} is inside the next query's window (>= {floor}) \
                     but is not in seen -- it will be re-emitted and drag the watermark back"
                );
            }
        }
        // The reviewer's demonstration, named outright.
        assert!(c.already_delivered("PAY-1", Some(t("2026-08-22T11:46:10.000+0000"))));
        // And the set is still bounded: what the query cannot return is dropped.
        assert!(!c.already_delivered("PAY-0", Some(t("2026-08-22T11:40:00.000+0000"))));
    }

    /// The same invariant swept across zones and second-offsets, because the
    /// truncation interacts with both.
    #[test]
    fn nothing_the_next_query_returns_is_forgotten_in_any_zone() {
        for hours in [-5, 0, 2, 5, 14] {
            let offset = hours * 3_600;
            for second in [0, 1, 17, 30, 59] {
                let watermark = t(&format!("2026-08-22T11:48:{second:02}.000+0000"));
                let delivered: Vec<(String, DateTime<Utc>)> = (0..150)
                    .map(|i| (format!("PAY-{i}"), watermark - chrono::Duration::seconds(i)))
                    .collect();
                let c = JiraCursor::advanced(watermark, offset, &delivered);
                let floor = query_floor(&c, offset);
                for (key, updated) in &delivered {
                    if *updated >= floor {
                        assert!(
                            c.already_delivered(key, Some(*updated)),
                            "offset {hours}h second {second}: {key} @ {updated} \
                             is in the window (>= {floor}) but not in seen"
                        );
                    }
                }
            }
        }
    }

    /// A cursor is stored in a text column and read on every run; an unbounded
    /// seen list would grow with a mass edit of ten thousand issues.
    #[test]
    fn the_seen_set_is_capped_at_the_newest_entries() {
        let watermark = t("2026-08-22T11:48:00.000+0000");
        let delivered: Vec<(String, DateTime<Utc>)> = (0..SEEN_CAP + 50)
            .map(|i| {
                (
                    format!("PAY-{i}"),
                    watermark - chrono::Duration::milliseconds(i as i64),
                )
            })
            .collect();
        let c = JiraCursor::advanced(watermark, 0, &delivered);
        assert_eq!(c.seen.len(), SEEN_CAP);
        // The newest survive the cap: they are the ones the next overlap query
        // will hand back again.
        assert!(c.already_delivered("PAY-0", Some(watermark)));
        let oldest = &delivered[SEEN_CAP + 49];
        assert!(!c.already_delivered(&oldest.0, Some(oldest.1)));
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
                ("PAY-231".to_owned(), t("2026-08-22T11:48:00.000+0000")),
                ("PAY-240".to_owned(), t("2026-08-22T11:47:10.000+0000")),
            ],
        )
        .encode();
        assert_eq!(JiraCursor::parse(&encoded).unwrap().encode(), encoded);
    }
}
