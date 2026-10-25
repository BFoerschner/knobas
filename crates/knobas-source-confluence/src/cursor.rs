//! The incremental position: `{"v":3,…}`, opaque to everything but this crate.
//!
//! ```text
//! {"v":3,
//!  "modified_to":"2026-08-22T10:40:00Z",  // the newest lastmodified delivered
//!  "tz_offset_secs":7200,                 // the zone that watermark was queried in
//!  "seen":[{"i":"98307","n":3,"u":"2026-08-22T10:40:00Z"}],
//!  "positions":{"98307":0,"98311":1}}      // each placed page's place (#1138)
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
//! A `(id, timestamp)` pair leaves a hole: a page edited twice within the
//! same second is missed (Jira's cursor had it until #345 keyed `seen` on a
//! fingerprint instead). Confluence hands out something better --
//! `version.number` increments on every edit -- so the identity of *this
//! version of this page* is exact, and two edits inside one second are two
//! versions. The timestamp is still stored, because it is what the window
//! filter needs, but it is not what identity is decided on.

use chrono::{DateTime, Utc};
use knobas_source::watermark::{self, Advanced, Bounds, Cql, Entry, MinuteLiteral};

/// Bump when the envelope's shape changes: an unrecognised version reads as
/// "no cursor", which is a full sync. Bumped to 2 by #860, whose `body_text`
/// carries the Docs search's marks: one full sync re-indexes every page that
/// was mirrored before them, where an incremental one would only reach the
/// pages edited since. Bumped to 3 by #953 for the same reason: one full sync
/// gives every mirrored page its `position` and its `mentioned_users`.
pub(crate) const CURSOR_VERSION: u8 = 3;

// The overlap, the daylight-saving widening and the seen cap are
// `knobas_source::watermark`'s (#1622), shared with Jira: CQL's
// `lastmodified` takes a `"yyyy-MM-dd HH:mm"` literal and resolves to the
// minute, exactly as JQL's `updated` does.

/// How many of the comment walk's records the envelope remembers, apart from
/// the page walk's [`SEEN_CAP`](knobas_source::watermark::SEEN_CAP) (#2073).
///
/// A comment is newer than the watermark it was found under -- the comment
/// walk never moves the watermark -- so it stays in the window until a page
/// edit carries the watermark past it, and a busy discussion piles up entries
/// for as long as nobody edits a page. In one shared cap those would win (the
/// cap keeps the newest), push the page walk's entries out and re-deliver
/// their pages on every poll. Past this cap the oldest comments' pages are
/// re-delivered instead, which costs a request and an upsert each, never an
/// edit. About 60 bytes an entry, so a full cap adds ~120 kB to the cursor.
pub(crate) const COMMENT_SEEN_CAP: usize = 2_000;

/// `seen`'s total order: newest first, the id breaking a tie, so one window
/// always encodes to the same bytes (`knobas_source::watermark::advanced`).
fn newest_first(a: &Seen, b: &Seen) -> std::cmp::Ordering {
    b.u.cmp(&a.u).then_with(|| a.i.cmp(&b.i))
}

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
    /// Where each page with a place among its siblings sat when the mirror
    /// last received it: content id to `extensions.position` (#1138). Pages
    /// in title order (`"none"`) are left out, so a space nobody reordered
    /// costs nothing here.
    ///
    /// `None` is a cursor written before #1138, which cannot tell a reorder
    /// from none: its next run re-reads every placed page once. That, rather
    /// than a version bump, is how the upgrade avoids a full sync. A map, so
    /// the same places always encode to the same bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positions: Option<Positions>,
}

/// Content id to its place among its siblings (#1138).
pub(crate) type Positions = std::collections::BTreeMap<String, i64>;

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

/// The identity is the id and `version.number`; `u` only bounds the window.
/// An entry stored with no number matches no version.
impl Entry for Seen {
    type Version = u64;
    fn key(&self) -> &str {
        &self.i
    }
    fn stamp(&self) -> DateTime<Utc> {
        self.u
    }
    fn is_version(&self, number: &u64) -> bool {
        self.n == Some(*number)
    }
}

impl ConfluenceCursor {
    /// A cursor for a source that has never delivered anything.
    pub(crate) fn empty(tz_offset_secs: i32) -> Self {
        Self {
            v: CURSOR_VERSION,
            modified_to: None,
            tz_offset_secs,
            seen: Vec::new(),
            positions: Some(Positions::new()),
        }
    }

    /// This cursor, recording `positions` as the places the mirror now holds.
    pub(crate) fn with_positions(mut self, positions: Positions) -> Self {
        self.positions = Some(positions);
        self
    }

    /// `None` for anything this version does not recognise -- which the caller
    /// reads as "full sync".
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let cursor: Self = serde_json::from_str(raw).ok()?;
        (cursor.v == CURSOR_VERSION).then_some(cursor)
    }

    pub(crate) fn encode(&self) -> String {
        // The one sanctioned `expect` in this crate: a cursor is plain fields
        // whose one map is keyed by strings, with no non-finite numbers, so the only way
        // `to_string` fails is a `Serialize` impl that cannot exist here.
        serde_json::to_string(self).expect("a cursor is plain data and always serializes")
    }

    /// The lower bound of the next `lastmodified >=` clause: two minutes
    /// before the watermark, an hour more on the one run after the instance's
    /// offset changed ([`knobas_source::watermark::since`]).
    ///
    /// # What the caller owes, in full
    ///
    /// Re-delivery here is harmless only if the run satisfies **both** of the
    /// following. Either one alone leaves an idle source oscillating, and the
    /// two fail in different places, so neither substitutes for the other:
    ///
    /// 1. **[`Self::advanced`] is handed the previous watermark and the
    ///    ceiling** in its bounds. Otherwise a re-delivered *older* page drags
    ///    the watermark backwards, or a page edited during the walk carries it
    ///    past edits the walk never saw.
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
        watermark::since(self.modified_to, self.tz_offset_secs, current_offset_secs)
    }

    /// Was this exact version of this page already handed to the sink?
    ///
    /// Identity is `(id, version.number)`. A record with **no** version number
    /// is never recognised -- a page whose version Confluence would not report
    /// is re-delivered on every run rather than skipped on a guess, which is
    /// the direction that costs bandwidth instead of losing an edit.
    pub(crate) fn already_delivered(&self, id: &str, number: Option<u64>) -> bool {
        number.is_some_and(|number| watermark::delivered(&self.seen, id, &number))
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
    ///
    /// `comments_seen` is the comment walk's share of the window, kept under
    /// [`COMMENT_SEEN_CAP`] rather than the page walk's cap: the two are
    /// filtered and ordered alike and stored as one list, so the envelope's
    /// shape is what it was.
    ///
    /// `bounds` carries the previous watermark and the run-start ceiling. The
    /// clamp, and the page walk's floor, order and cap, are
    /// [`knobas_source::watermark::advanced`]'s, shared with Jira; the
    /// comments' share repeats its floor and order here, because a second
    /// call would cut it at the shared cap.
    pub(crate) fn advanced(
        bounds: Bounds,
        newest: DateTime<Utc>,
        tz_offset_secs: i32,
        seen_in_window: &[Seen],
        comments_seen: &[Seen],
    ) -> Self {
        let Advanced {
            watermark,
            mut seen,
        } = watermark::advanced::<Cql, Seen>(bounds, newest, tz_offset_secs, seen_in_window);
        // The comments' share: the same band and order, under its own cap.
        // Not a second `watermark::advanced` call, which would cut it at
        // `SEEN_CAP`; that cap is shared with Jira and stays as it is.
        let floor = Cql::floor(
            watermark - chrono::Duration::minutes(watermark::OVERLAP_MINUTES),
            tz_offset_secs,
        );
        let mut comments: Vec<Seen> = comments_seen
            .iter()
            .filter(|s| s.u >= floor)
            .cloned()
            .collect();
        comments.sort_by(newest_first);
        comments.truncate(COMMENT_SEEN_CAP);
        seen.extend(comments);
        seen.sort_by(newest_first);
        Self {
            v: CURSOR_VERSION,
            modified_to: Some(watermark),
            tz_offset_secs,
            seen,
            positions: None,
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
            Bounds::default(),
            utc("2026-08-22T10:40:00Z"),
            7200,
            &[seen("98307", 3, "2026-08-22T10:40:00Z")],
            &[],
        );
        let encoded = good.encode();
        // The stored bytes, literally: the shared watermark (#1622) moved the
        // arithmetic and none of the envelope, so no stored cursor re-syncs.
        assert_eq!(
            encoded,
            r#"{"v":3,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"2026-08-22T10:40:00Z"}]}"#
        );
        assert_eq!(
            good.clone()
                .with_positions(Positions::from([("98307".to_owned(), 0)]))
                .encode(),
            r#"{"v":3,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"2026-08-22T10:40:00Z"}],"positions":{"98307":0}}"#
        );
        assert_eq!(ConfluenceCursor::parse(&encoded), Some(good));
        assert_eq!(ConfluenceCursor::parse("not json"), None);
        assert_eq!(ConfluenceCursor::parse(r#"{"v":99}"#), None);
        // A cursor from before #860's marks is a full sync, which re-indexes.
        let mut old: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        old["v"] = serde_json::json!(1);
        assert_eq!(ConfluenceCursor::parse(&old.to_string()), None);
        // So is one from before #953's positions and looked-up users.
        old["v"] = serde_json::json!(2);
        assert_eq!(ConfluenceCursor::parse(&old.to_string()), None);
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
        let cursor = ConfluenceCursor::advanced(
            Bounds::default(),
            utc("2026-08-22T10:40:00Z"),
            7200,
            &[],
            &[],
        );
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
        let cursor = ConfluenceCursor::advanced(
            Bounds::default(),
            utc("2026-10-25T10:40:00Z"),
            7200,
            &[],
            &[],
        );
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
            Bounds::default(),
            utc("2026-08-22T10:40:00Z"),
            7200,
            &[seen("98307", 3, "2026-08-22T10:40:00Z")],
            &[],
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
            Bounds::default(),
            watermark,
            0,
            &[
                seen("in-band", 1, "2026-08-22T10:38:30Z"),
                seen("out-of-band", 1, "2026-08-22T10:37:00Z"),
            ],
            &[],
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
        let one = ConfluenceCursor::advanced(
            Bounds::default(),
            watermark,
            0,
            &[a.clone(), b.clone(), c.clone()],
            &[],
        );
        let two = ConfluenceCursor::advanced(Bounds::default(), watermark, 0, &[c, a, b], &[]);
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
        let records: Vec<Seen> = (0..knobas_source::watermark::SEEN_CAP + 10)
            .map(|i| Seen {
                i: format!("{i:06}"),
                n: Some(1),
                u: base - chrono::Duration::milliseconds(i as i64),
            })
            .collect();
        let cursor = ConfluenceCursor::advanced(Bounds::default(), base, 0, &records, &[]);
        assert_eq!(cursor.seen.len(), knobas_source::watermark::SEEN_CAP);
        assert_eq!(cursor.seen[0].i, "000000", "the newest survives");
        assert!(cursor.already_delivered("000000", Some(1)));
    }

    /// The comment walk's entries are capped apart (#2073): a window holding
    /// more comments than both caps keeps the page walk's entries whole and
    /// the newest [`COMMENT_SEEN_CAP`] comments, all of them newer than every
    /// page, in one newest-first list.
    #[test]
    fn comments_past_their_cap_push_out_no_page_entry() {
        let base = utc("2026-08-22T10:40:00Z");
        let pages: Vec<Seen> = (0..3)
            .map(|i| Seen {
                i: format!("page-{i}"),
                n: Some(1),
                u: base - chrono::Duration::milliseconds(i),
            })
            .collect();
        let comments: Vec<Seen> = (0..COMMENT_SEEN_CAP + 10)
            .map(|i| Seen {
                i: format!("{i:06}"),
                n: Some(1),
                u: base + chrono::Duration::milliseconds(1 + i as i64),
            })
            .collect();
        let cursor = ConfluenceCursor::advanced(Bounds::default(), base, 0, &pages, &comments);
        assert_eq!(cursor.seen.len(), COMMENT_SEEN_CAP + 3);
        for page in &pages {
            assert!(cursor.already_delivered(&page.i, Some(1)), "{}", page.i);
        }
        let newest = format!("{:06}", COMMENT_SEEN_CAP + 9);
        assert_eq!(cursor.seen[0].i, newest, "newest first across both walks");
        assert!(
            !cursor.already_delivered("000000", Some(1)),
            "the oldest comments go"
        );
        assert_eq!(cursor.seen[COMMENT_SEEN_CAP].i, "page-0");
    }
}
