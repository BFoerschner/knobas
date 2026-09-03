//! The incremental position: `{"v":1,…}`, opaque to everything but this crate.
//!
//! ```text
//! {"v":1,
//!  "modified_to":"2026-08-22T10:40:00Z",  // the newest lastmodified delivered
//!  "tz_offset_secs":7200,                 // the zone that watermark was queried in
//!  "seen":[{"i":"98307","n":3,"u":"2026-08-22T10:40:00Z"}]}
//! ```
//!
//! `seen` is the price of correctness. **CQL resolves to the minute**, so the
//! next query must start *before* the watermark -- and everything in that
//! window comes back. Dropping the records already delivered turns
//! re-delivery from a contract violation (battery clause 2: an idle
//! incremental emits nothing) into an invisible detail.
//!
//! # `(id, version)`, not `(id, timestamp)`
//!
//! The Jira adapter recognises a re-delivered issue by its `(key, updated)`
//! pair and documents the hole that leaves: "an issue edited twice within the
//! same second is missed here". Confluence hands out something better --
//! `version.number` increments on every edit -- so the identity of *this
//! version of this page* is exact, and two edits inside one second are two
//! versions. The timestamp is still stored, because it is what the window
//! filter needs, but it is not what identity is decided on.

use chrono::{DateTime, Utc};

/// Bump when the envelope's shape changes: an unrecognised version reads as
/// "no cursor", which is a full sync.
pub(crate) const CURSOR_VERSION: u8 = 1;

/// How far back of the watermark the next query starts.
///
/// CQL's `lastmodified` takes a `"yyyy-MM-dd HH:mm"` literal and therefore
/// resolves to the minute, so an exact-boundary watermark drops every edit
/// that shares its minute. Re-delivery is free -- upserts are idempotent, and
/// `seen` recognises what came back.
const OVERLAP_MINUTES: i64 = 2;

/// Used for the one run after the instance's UTC offset changed (a daylight
/// saving shift is at most an hour), so no page falls in the seam.
const DST_OVERLAP_MINUTES: i64 = 65;

/// How many `(id, version, when)` records the envelope remembers. Beyond this
/// a mass edit inside one overlap window may be delivered twice -- upserts are
/// idempotent, so the cost is bandwidth, not correctness.
pub(crate) const SEEN_CAP: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ConfluenceCursor {
    pub v: u8,
    /// The newest `version.when` this source has delivered, or `None` for a
    /// source that has never delivered anything.
    pub modified_to: Option<DateTime<Utc>>,
    /// The instance's UTC offset when that watermark was taken.
    pub tz_offset_secs: i32,
    /// What was delivered inside the overlap window, newest first.
    pub seen: Vec<Seen>,
}

/// One version of one page, as the window last saw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Seen {
    /// The content id.
    pub i: String,
    /// `version.number` -- what makes this *this* version of that page.
    pub n: Option<u64>,
    /// `version.when`, which is what the window filter is applied to.
    pub u: DateTime<Utc>,
}

impl ConfluenceCursor {
    /// A cursor for a source that has never delivered anything.
    pub(crate) fn empty(tz_offset_secs: i32) -> Self {
        Self {
            v: CURSOR_VERSION,
            modified_to: None,
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

    /// The lower bound of the next `lastmodified >=` clause.
    ///
    /// On the one run after the instance's offset changed this reaches further
    /// back than the window [`Self::advanced`] recorded `seen` for, so that run
    /// re-delivers what falls in between. That is deliberate -- widening is how
    /// no page falls in the daylight-saving seam, and `advanced` cannot know an
    /// offset that has not been observed yet.
    ///
    /// # What the caller owes, in full
    ///
    /// Re-delivery here is harmless only if the run satisfies **both** of the
    /// following. Either one alone leaves an idle source oscillating, and the
    /// two fail in different places, so neither substitutes for the other:
    ///
    /// 1. **The new watermark is `max(previous, newest emitted)`, clamped to
    ///    the run's ceiling.** Otherwise a re-delivered *older* page drags the
    ///    watermark backwards.
    /// 2. **`advanced` is handed every record in the window, skipped ones
    ///    included** -- see its own contract. A run that passes only what it
    ///    emitted drops the records it recognised out of `seen`, and they come
    ///    back unrecognised next run. The watermark holds perfectly while this
    ///    happens; it is `seen` that oscillates, and the source emits an item
    ///    on every poll forever.
    ///
    /// Condition 2 hides from the obvious test: full-sync-then-idle is stable
    /// under it, because a full sync skips nothing. It takes **one edited page
    /// and then an idle poll** to show.
    pub(crate) fn since(&self, current_offset_secs: i32) -> Option<DateTime<Utc>> {
        let watermark = self.modified_to?;
        let minutes = if current_offset_secs == self.tz_offset_secs {
            OVERLAP_MINUTES
        } else {
            DST_OVERLAP_MINUTES
        };
        Some(watermark - chrono::Duration::minutes(minutes))
    }

    /// Was this exact version of this page already handed to the sink?
    ///
    /// Identity is `(id, version.number)`. A record with **no** version number
    /// is never recognised -- a page whose version Confluence would not report
    /// is re-delivered on every run rather than skipped on a guess, which is
    /// the direction that costs bandwidth instead of losing an edit.
    pub(crate) fn already_delivered(&self, id: &str, number: Option<u64>) -> bool {
        let Some(number) = number else {
            return false;
        };
        self.seen
            .iter()
            .any(|s| s.n == Some(number) && s.i == id)
    }

    /// The cursor for a run that emitted something.
    ///
    /// # `seen_in_window` is every record the run *saw*, not every one it sent
    ///
    /// The run must pass every `(id, version, when)` it observed inside the
    /// overlap window -- **the ones it skipped as already-delivered just as
    /// much as the ones it pushed to the sink**. `seen` is a record of what the
    /// *window* contained, not of what crossed the SPI, and the two differ on
    /// exactly the items that make an idle poll idle.
    ///
    /// This is why the parameter is not called `delivered`.
    pub(crate) fn advanced(
        watermark: DateTime<Utc>,
        tz_offset_secs: i32,
        seen_in_window: &[Seen],
    ) -> Self {
        // The band the next query will really return, not the band the
        // arithmetic suggests. `since` subtracts whole minutes, but the literal
        // it is rendered into has minute resolution and truncates *downwards*,
        // so the query's true lower bound is up to 59 s earlier than
        // `watermark - OVERLAP_MINUTES`. Filtering `seen` on the un-truncated
        // value drops every record delivered in that band while the query keeps
        // returning them: run N+1 re-emits one, run N+2 re-emits another, and
        // an idle poll never settles -- battery clause 2 failing in steady
        // state, not at a daylight-saving edge.
        //
        // Asking `cql_floor` rather than truncating here keeps the two
        // definitions from drifting apart: it renders the literal and reads it
        // back, so the floor is the query's meaning by construction.
        let floor = crate::time::cql_floor(
            watermark - chrono::Duration::minutes(OVERLAP_MINUTES),
            tz_offset_secs,
        );
        let mut seen: Vec<Seen> = seen_in_window
            .iter()
            .filter(|s| s.u >= floor)
            .cloned()
            .collect();
        // Newest first, so the cap drops the entries least likely to come back,
        // with the id breaking ties. A page appears at most once per run, so
        // this is a *total* order on the records -- which is the whole of what
        // makes the same delivered set encode to the same bytes however the
        // pages happened to arrive. The engine compares cursors as strings, so
        // that property is what lets an idle poll be recognised as one.
        seen.sort_by(|a, b| b.u.cmp(&a.u).then_with(|| a.i.cmp(&b.i)));
        seen.truncate(SEEN_CAP);
        Self {
            v: CURSOR_VERSION,
            modified_to: Some(watermark),
            tz_offset_secs,
            seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().expect("a test timestamp")
    }

    fn seen(id: &str, n: u64, when: &str) -> Seen {
        Seen {
            i: id.to_owned(),
            n: Some(n),
            u: utc(when),
        }
    }

    /// Contract §4.1: the cursor is a versioned JSON envelope, so a later
    /// shape change is detectable -- and an unrecognised version means "full
    /// sync" rather than a parse failure that strands the source.
    #[test]
    fn an_unrecognised_envelope_reads_as_no_cursor() {
        let good = ConfluenceCursor::advanced(
            utc("2026-08-22T10:40:00Z"),
            7200,
            &[seen("98307", 3, "2026-08-22T10:40:00Z")],
        );
        let encoded = good.encode();
        assert_eq!(ConfluenceCursor::parse(&encoded), Some(good));
        assert_eq!(ConfluenceCursor::parse("not json"), None);
        assert_eq!(ConfluenceCursor::parse(r#"{"v":99}"#), None);
        // A TeamCity or Jira cursor handed to this adapter is not this
        // adapter's cursor, and reads as a full sync rather than as a position.
        assert_eq!(
            ConfluenceCursor::parse(r#"{"v":1,"updated_to":"2026-08-22T10:40:00Z"}"#),
            None,
            "a cursor with no `tz_offset_secs` is not one this adapter wrote"
        );
    }

    /// The overlap is mandatory, and it is two minutes because the literal has
    /// minute resolution: a query starting *at* the watermark drops every edit
    /// that shares the watermark's minute.
    #[test]
    fn the_next_query_starts_before_the_watermark() {
        let cursor = ConfluenceCursor::advanced(utc("2026-08-22T10:40:00Z"), 7200, &[]);
        assert_eq!(cursor.since(7200), Some(utc("2026-08-22T10:38:00Z")));
        // A source that never delivered anything has no lower bound at all,
        // which is what makes its first run a full sync.
        assert_eq!(ConfluenceCursor::empty(7200).since(7200), None);
    }

    /// After a daylight-saving shift the window widens for exactly one run, so
    /// nothing falls in the seam between the zone the watermark was taken in
    /// and the zone the next query is read in.
    #[test]
    fn a_changed_offset_widens_the_window_for_one_run() {
        let cursor = ConfluenceCursor::advanced(utc("2026-10-25T10:40:00Z"), 7200, &[]);
        assert_eq!(cursor.since(3600), Some(utc("2026-10-25T09:35:00Z")));
        assert!(
            cursor.since(3600).unwrap() < cursor.since(7200).unwrap(),
            "the widened window reaches strictly further back"
        );
    }

    /// Identity is `(id, version)`. Two edits in one second are two versions,
    /// which is the hole a `(id, timestamp)` pair would leave -- and the whole
    /// reason this adapter stores the number.
    #[test]
    fn a_page_is_recognised_by_its_version_not_by_its_timestamp() {
        let cursor = ConfluenceCursor::advanced(
            utc("2026-08-22T10:40:00Z"),
            7200,
            &[seen("98307", 3, "2026-08-22T10:40:00Z")],
        );
        assert!(cursor.already_delivered("98307", Some(3)));
        // The same page, edited again inside the same second: a new version,
        // so it is *not* already delivered.
        assert!(!cursor.already_delivered("98307", Some(4)));
        // Another page that happens to be at version 3.
        assert!(!cursor.already_delivered("98999", Some(3)));
        // A record whose version the server would not report is re-delivered
        // rather than skipped on a guess.
        assert!(!cursor.already_delivered("98307", None));
    }

    /// `seen` covers exactly the band the next query returns. Filtering on the
    /// un-truncated watermark instead would drop the records delivered in the
    /// sub-minute tail while the query keeps returning them, and an idle poll
    /// would never settle.
    #[test]
    fn seen_covers_the_band_the_next_query_really_returns() {
        // The watermark is 59 s past a minute boundary, so the query's true
        // lower bound is 10:38:00 -- not 10:38:59.
        let watermark = utc("2026-08-22T10:40:59Z");
        let cursor = ConfluenceCursor::advanced(
            watermark,
            0,
            &[
                seen("in-band", 1, "2026-08-22T10:38:30Z"),
                seen("out-of-band", 1, "2026-08-22T10:37:00Z"),
            ],
        );
        let ids: Vec<&str> = cursor.seen.iter().map(|s| s.i.as_str()).collect();
        assert_eq!(
            ids,
            vec!["in-band"],
            "a record inside the band the query returns is remembered; one before it is not"
        );
        assert!(cursor.already_delivered("in-band", Some(1)));
    }

    /// The engine compares cursors as strings, so the same delivered set must
    /// encode to the same bytes however the pages happened to arrive -- or an
    /// idle poll is never recognised as one.
    #[test]
    fn the_same_window_encodes_to_the_same_bytes_in_any_arrival_order() {
        let a = seen("98307", 3, "2026-08-22T10:40:00Z");
        let b = seen("98311", 1, "2026-08-22T10:39:00Z");
        let c = seen("98299", 7, "2026-08-22T10:40:00Z");
        let watermark = utc("2026-08-22T10:40:00Z");
        let one = ConfluenceCursor::advanced(watermark, 0, &[a.clone(), b.clone(), c.clone()]);
        let two = ConfluenceCursor::advanced(watermark, 0, &[c, a, b]);
        assert_eq!(one.encode(), two.encode());
        // Newest first, with the id breaking a tie -- a total order, which is
        // what makes the bytes stable rather than merely usually equal.
        let ids: Vec<&str> = one.seen.iter().map(|s| s.i.as_str()).collect();
        assert_eq!(ids, vec!["98299", "98307", "98311"]);
    }

    /// A mass edit inside one window costs bandwidth, not correctness: the cap
    /// drops the oldest records, which are the ones least likely to come back.
    ///
    /// The records are milliseconds apart, so every one of them is inside the
    /// band and the **cap** is what does the dropping -- spread over seconds
    /// the window filter would truncate the list first and this would be
    /// asserting the wrong mechanism.
    #[test]
    fn a_window_larger_than_the_cap_keeps_the_newest() {
        let base = utc("2026-08-22T10:40:00Z");
        let records: Vec<Seen> = (0..SEEN_CAP + 10)
            .map(|i| Seen {
                i: format!("{i:06}"),
                n: Some(1),
                u: base - chrono::Duration::milliseconds(i as i64),
            })
            .collect();
        let cursor = ConfluenceCursor::advanced(base, 0, &records);
        assert_eq!(cursor.seen.len(), SEEN_CAP);
        assert_eq!(cursor.seen[0].i, "000000", "the newest survives");
        assert!(cursor.already_delivered("000000", Some(1)));
    }
}
