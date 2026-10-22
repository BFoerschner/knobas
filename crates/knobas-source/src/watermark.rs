//! The minute-resolution watermark Jira and Confluence both sync on
//! (`CONTEXT.md` **Watermark**; spec #1618, ruling A4).
//!
//! Both products bound an incremental query with a date literal that
//! resolves to the minute and is read in the server's zone: JQL's
//! `updated >= "yyyy-MM-dd HH:mm"`, CQL's `lastmodified >= "yyyy-MM-dd HH:mm"`.
//! Everything that follows from that is here, once:
//!
//! * [`since`]: the next query starts [`OVERLAP_MINUTES`] before the
//!   watermark, because an exact-boundary bound drops every edit that shares
//!   its minute; on the one run after the server's UTC offset changed it
//!   reaches back [`DST_OVERLAP_MINUTES`] instead, so nothing falls in the
//!   daylight-saving seam.
//! * [`advanced`]: the new watermark, held at the run-start **ceiling** and
//!   never below the previous one, and the `seen` set the next run recognises
//!   re-deliveries by: filtered at the query's real floor, newest first,
//!   capped at [`SEEN_CAP`].
//!
//! Each adapter keeps its own envelope (its field names, its version, its
//! extra fields) and its own [`Entry`] type, so the bytes it stores are its
//! own; what it no longer keeps is a copy of this arithmetic. The two copies
//! this replaced had already drifted apart: Confluence clamped to its ceiling
//! and Jira did not.
//!
//! # What the caller owes, in full
//!
//! Re-delivery inside the overlap is harmless only if the run satisfies
//! **both** of the following. Either one alone leaves an idle source
//! oscillating, and the two fail in different places:
//!
//! 1. **The watermark only moves forward.** [`advanced`] takes the previous
//!    watermark in its [`Bounds`] and never returns less, so a re-delivered
//!    older record cannot drag it back.
//! 2. **`advanced` is handed every record the run saw in the window, skipped
//!    ones included.** `seen` is a record of what the *window* contained, not
//!    of what crossed the SPI. A run that passes only what it emitted forgets,
//!    every run, whatever it recognised that run, and an idle poll never
//!    settles. It takes one new record and then an idle poll to show; a full
//!    sync skips nothing, so full-sync-then-idle passes either way.

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, TimeZone, Utc};

/// How far back of the watermark the next query starts. The literal resolves
/// to the minute, so a query starting *at* the watermark drops every edit
/// sharing its minute. Re-delivery is free: upserts are idempotent, and
/// `seen` recognises what came back.
pub const OVERLAP_MINUTES: i64 = 2;

/// The overlap for the one run after the server's UTC offset changed. A
/// daylight-saving shift is at most an hour.
pub const DST_OVERLAP_MINUTES: i64 = 65;

/// How many records the envelope remembers. Beyond this a mass edit inside one
/// overlap window may be delivered twice: upserts are idempotent, so the cost
/// is bandwidth, not correctness.
pub const SEEN_CAP: usize = 500;

/// The lowest UTC offset any real time zone uses (UTC-12, Baker Island).
///
/// The fallback wherever the server's own zone is unknown, and deliberately
/// **not zero**. The literal is read back in the *server's* zone, so the
/// query's real lower bound is `intended + (assumed_offset - true_offset)`:
/// assuming an offset *higher* than the truth moves that bound forward and
/// silently skips every edit in between, five hours of them for a UTC-05
/// server, on every run and never recovered. Assuming an offset no higher than
/// any real zone can only move it backwards, which re-reads work already done.
/// Over-fetching is recoverable; under-fetching is not.
pub const MIN_UTC_OFFSET_SECS: i32 = -12 * 3600;

/// A minute-resolution date literal read in a fixed UTC offset: JQL's or
/// CQL's. An adapter names its query language and gets the rendering and the
/// floor from it, so the `seen` filter and the rendered query cannot disagree.
pub trait MinuteLiteral {
    /// The `chrono` format the literal is written in.
    const FORMAT: &'static str;

    /// `t` as the literal, in the server's zone.
    fn render(t: DateTime<Utc>, offset_secs: i32) -> String {
        t.with_timezone(&zone_of(offset_secs))
            .format(Self::FORMAT)
            .to_string()
    }

    /// The instant the server reads [`render`](Self::render)'s literal as:
    /// the start of `t`'s minute in that zone. Rendered and read back, so it
    /// is the query's meaning by construction rather than a second
    /// definition of it. A literal that will not read back (it cannot, for
    /// the formats here) floors a whole minute early, which only re-reads.
    ///
    /// Do not "simplify" this into a truncation in UTC. The two agree only
    /// because every real offset is a whole number of minutes, which is a fact
    /// about the adapters' offset parsers and not about this `i32`; at
    /// `+00:00:30` they diverge. Reading back costs nothing and stays right.
    fn floor(t: DateTime<Utc>, offset_secs: i32) -> DateTime<Utc> {
        let zone = zone_of(offset_secs);
        NaiveDateTime::parse_from_str(&Self::render(t, offset_secs), Self::FORMAT)
            .ok()
            .and_then(|naive| zone.from_local_datetime(&naive).single())
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|| t - chrono::Duration::minutes(1))
    }
}

/// Jira's JQL date literal, `"yyyy-MM-dd HH:mm"` in the server's zone.
pub enum Jql {}

impl MinuteLiteral for Jql {
    const FORMAT: &'static str = "%Y-%m-%d %H:%M";
}

/// Confluence's CQL date literal, `"yyyy-MM-dd HH:mm"` in the instance's zone.
pub enum Cql {}

impl MinuteLiteral for Cql {
    const FORMAT: &'static str = "%Y-%m-%d %H:%M";
}

/// The zone `offset_secs` names, or [`MIN_UTC_OFFSET_SECS`] for one that does
/// not exist.
fn zone_of(offset_secs: i32) -> FixedOffset {
    FixedOffset::east_opt(offset_secs)
        .or_else(|| FixedOffset::east_opt(MIN_UTC_OFFSET_SECS))
        .unwrap_or_else(|| Utc.fix())
}

/// One record of the overlap window as an adapter's envelope stores it.
///
/// The identity is the adapter's: Jira's is the issue key and the fingerprint
/// of the raw record (a stamp to the second cannot tell two edits in one
/// second apart, #345), Confluence's the content id and `version.number`.
pub trait Entry: Clone {
    /// What tells two versions of one record apart.
    type Version: ?Sized;
    /// The record's key. A key appears at most once per window, which is what
    /// makes the newest-first order total.
    fn key(&self) -> &str;
    /// The record's stamp, which bounds the set to the window. Not the
    /// identity.
    fn stamp(&self) -> DateTime<Utc>;
    /// Is this entry `version` of its record?
    fn is_version(&self, version: &Self::Version) -> bool;
}

/// Was this version of the record `key` already handed to the sink?
pub fn delivered<E: Entry>(seen: &[E], key: &str, version: &E::Version) -> bool {
    seen.iter().any(|s| s.key() == key && s.is_version(version))
}

/// The lower bound of the next query, or `None` (a full sync) for a source
/// that never delivered anything.
///
/// On the one run after the server's offset changed this reaches further back
/// than the window [`advanced`] recorded `seen` for, so that run re-delivers
/// what falls in between. That is deliberate: widening is how nothing falls
/// in the seam, and `advanced` cannot know an offset not yet observed.
pub fn since(
    watermark: Option<DateTime<Utc>>,
    recorded_offset_secs: i32,
    current_offset_secs: i32,
) -> Option<DateTime<Utc>> {
    let minutes = if current_offset_secs == recorded_offset_secs {
        OVERLAP_MINUTES
    } else {
        DST_OVERLAP_MINUTES
    };
    watermark.map(|w| w - chrono::Duration::minutes(minutes))
}

/// What holds a run's new watermark in place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bounds {
    /// The watermark the run started from. The new one is never below it.
    pub previous: Option<DateTime<Utc>>,
    /// The newest stamp in the source's scope when the run started, the
    /// **ceiling**: witnessed by a probe, never clocked. `None` for a corpus
    /// with nothing in it, which clamps nothing.
    pub ceiling: Option<DateTime<Utc>>,
}

/// The position a run that emitted something ends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advanced<E> {
    pub watermark: DateTime<Utc>,
    /// The window's records, newest first and capped.
    pub seen: Vec<E>,
}

/// The new watermark and the `seen` set the next run recognises.
///
/// **The watermark** is `newest` held at the ceiling and never below the
/// previous watermark. A record edited *during* the walk carries a stamp past
/// anything the run saw at its start; advancing to it would put the run's own
/// duration below the next query's bound and hide every other edit made
/// while it ran. The ceiling can also be older than the previous watermark
/// (the record that set it was deleted or left the scope), and clamping to it
/// then would drag the position back and re-deliver its recent history on
/// every poll, so the floor wins.
///
/// **`seen`** is every record of `seen_in_window` at or after the next
/// query's real floor: `watermark - OVERLAP_MINUTES` *as the literal reads
/// it*, which truncates down by up to 59 s. Filtering on the un-truncated
/// instant drops the records in that band while the query keeps returning
/// them, and an idle poll oscillates for ever. Newest first with the key
/// breaking ties, a total order, so the same window encodes to the same bytes
/// however its pages arrived; the engine compares cursors as strings. Capped
/// at [`SEEN_CAP`], which drops the oldest: the ones least likely to come
/// back.
pub fn advanced<L: MinuteLiteral, E: Entry>(
    bounds: Bounds,
    newest: DateTime<Utc>,
    tz_offset_secs: i32,
    seen_in_window: &[E],
) -> Advanced<E> {
    let held = bounds.ceiling.map_or(newest, |ceiling| newest.min(ceiling));
    let watermark = bounds.previous.map_or(held, |previous| held.max(previous));
    let floor = L::floor(
        watermark - chrono::Duration::minutes(OVERLAP_MINUTES),
        tz_offset_secs,
    );
    let mut seen: Vec<E> = seen_in_window
        .iter()
        .filter(|s| s.stamp() >= floor)
        .cloned()
        .collect();
    seen.sort_by(|a, b| b.stamp().cmp(&a.stamp()).then_with(|| a.key().cmp(b.key())));
    seen.truncate(SEEN_CAP);
    Advanced { watermark, seen }
}

#[cfg(test)]
mod tests {
    //! Every test runs once per literal: the arithmetic is shared, so a
    //! literal whose floor or rendering differed would show here first.

    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().expect("a test timestamp")
    }

    /// A window record: key, stamp, and a version standing in for Jira's
    /// fingerprint or Confluence's `version.number`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Rec {
        k: String,
        u: DateTime<Utc>,
        v: u32,
    }

    impl Entry for Rec {
        type Version = u32;
        fn key(&self) -> &str {
            &self.k
        }
        fn stamp(&self) -> DateTime<Utc> {
            self.u
        }
        fn is_version(&self, version: &u32) -> bool {
            self.v == *version
        }
    }

    fn rec(k: &str, u: &str) -> Rec {
        Rec {
            k: k.to_owned(),
            u: utc(u),
            v: 1,
        }
    }

    fn keys(seen: &[Rec]) -> Vec<&str> {
        seen.iter().map(|s| s.k.as_str()).collect()
    }

    /// Runs `case` for JQL and for CQL.
    fn both(case: fn(&'static str, fn(DateTime<Utc>, i32) -> String, AdvanceFn)) {
        case("jql", Jql::render, advanced::<Jql, Rec>);
        case("cql", Cql::render, advanced::<Cql, Rec>);
    }

    type AdvanceFn = fn(Bounds, DateTime<Utc>, i32, &[Rec]) -> Advanced<Rec>;

    /// The overlap is mandatory: the literal has minute resolution, so a
    /// query starting at the watermark drops every edit sharing its minute.
    /// A source that never delivered anything has no bound: a full sync.
    #[test]
    fn the_next_query_starts_two_minutes_before_the_watermark() {
        let w = utc("2026-08-22T10:40:00Z");
        assert_eq!(since(Some(w), 7200, 7200), Some(utc("2026-08-22T10:38:00Z")));
        assert_eq!(since(None, 7200, 7200), None);
    }

    /// After the server's offset changed, the one run widens past the shift
    /// so nothing falls in the seam between the two zones.
    #[test]
    fn a_changed_offset_widens_the_window_by_more_than_an_hour() {
        let w = utc("2026-10-25T01:30:00Z");
        assert_eq!(since(Some(w), 7200, 3600), Some(utc("2026-10-25T00:25:00Z")));
        assert_eq!(since(Some(w), 3600, 7200), Some(utc("2026-10-25T00:25:00Z")));
    }

    /// The literal is the minute in the server's zone, and an offset no zone
    /// has falls back to UTC-12, never to UTC: guessing low only re-reads.
    #[test]
    fn a_literal_is_the_minute_in_the_servers_zone() {
        both(|name, render, _| {
            let t = utc("2026-08-22T10:40:59Z");
            assert_eq!(render(t, 7200), "2026-08-22 12:40", "{name}");
            assert_eq!(render(t, 0), "2026-08-22 10:40", "{name}");
            assert_eq!(render(t, -5 * 3600), "2026-08-22 05:40", "{name}");
            assert_eq!(render(t, i32::MAX), "2026-08-21 22:40", "{name}");
        });
        let t = utc("2026-08-22T10:40:59Z");
        for offset in [-12 * 3600, -5 * 3600, 0, 7200, 20_700, 14 * 3600] {
            assert_eq!(Jql::floor(t, offset), utc("2026-08-22T10:40:00Z"));
            assert_eq!(Cql::floor(t, offset), utc("2026-08-22T10:40:00Z"));
        }
        let exact = utc("2026-08-22T10:40:00Z");
        assert_eq!(Jql::floor(exact, 7200), exact);
        assert_eq!(Cql::floor(exact, 7200), exact);
    }

    /// Read a literal the way a server in `offset_secs` would. Independent of
    /// [`MinuteLiteral::floor`] on purpose: using that here would test the
    /// implementation against itself.
    fn read_as(literal: &str, offset_secs: i32) -> DateTime<Utc> {
        let zone = FixedOffset::east_opt(offset_secs).expect("a real offset");
        NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M")
            .expect("a literal this module rendered")
            .and_local_timezone(zone)
            .single()
            .expect("a fixed offset has no ambiguous local times")
            .with_timezone(&Utc)
    }

    /// **The invariant the whole watermark rests on.** A literal is read back
    /// as an instant no *later* than the one it was rendered from: later
    /// means the query starts past edits never delivered, and nothing goes
    /// back for them. Across every real zone, and with the zone unknown
    /// (rendered at the fallback, read in the real one), which is the case a
    /// UTC fallback gets wrong.
    #[test]
    fn rendering_a_literal_never_shifts_the_query_forward() {
        fn case<L: MinuteLiteral>(name: &str) {
            let t = utc("2026-08-22T11:46:40Z");
            for hours in -12..=14 {
                let offset = hours * 3_600;
                let known = read_as(&L::render(t, offset), offset);
                assert!(known <= t, "{name} {hours}: {known} is later than {t}");
                assert_eq!(known, L::floor(t, offset), "{name} {hours}: floor disagrees");
                let guessed = read_as(&L::render(t, MIN_UTC_OFFSET_SECS), offset);
                assert!(guessed <= t, "{name} {hours}: a guessed zone queried from {guessed}");
            }
        }
        case::<Jql>("jql");
        case::<Cql>("cql");
    }

    /// **The floor.** `seen` covers the band the next query really returns:
    /// the literal truncates `watermark - 2 min` down to its minute, so a
    /// watermark 59 s past a minute boundary queries from 10:38:00, not
    /// 10:38:59. A record in that band must be remembered or it comes back
    /// unrecognised on every idle poll.
    #[test]
    fn seen_covers_the_band_the_next_query_really_returns() {
        both(|name, _, advance| {
            for offset in [-5 * 3600, 0, 7200] {
                let next = advance(
                    Bounds::default(),
                    utc("2026-08-22T10:40:59Z"),
                    offset,
                    &[
                        rec("in-band", "2026-08-22T10:38:00Z"),
                        rec("out-of-band", "2026-08-22T10:37:59Z"),
                    ],
                );
                assert_eq!(keys(&next.seen), vec!["in-band"], "{name} at {offset}");
            }
        });
    }

    /// The same window encodes the same way however its pages arrived:
    /// newest first, the key breaking a tie.
    #[test]
    fn the_window_is_ordered_newest_first_whatever_the_arrival_order() {
        both(|name, _, advance| {
            let w = utc("2026-08-22T10:40:00Z");
            let a = rec("PAY-2", "2026-08-22T10:40:00Z");
            let b = rec("PAY-0", "2026-08-22T10:39:00Z");
            let c = rec("PAY-1", "2026-08-22T10:40:00Z");
            let one = advance(Bounds::default(), w, 0, &[a.clone(), b.clone(), c.clone()]);
            let two = advance(Bounds::default(), w, 0, &[c, b, a]);
            assert_eq!(keys(&one.seen), vec!["PAY-1", "PAY-2", "PAY-0"], "{name}");
            assert_eq!(one, two, "{name}");
        });
    }

    /// **The seen cap.** A mass edit inside one window keeps the newest
    /// [`SEEN_CAP`] records. Milliseconds apart, so every one is inside the
    /// band and the cap, not the window filter, does the dropping.
    #[test]
    fn a_window_larger_than_the_cap_keeps_the_newest() {
        both(|name, _, advance| {
            let w = utc("2026-08-22T10:40:00Z");
            let records: Vec<Rec> = (0..SEEN_CAP + 10)
                .map(|i| Rec {
                    k: format!("{i:06}"),
                    u: w - chrono::Duration::milliseconds(i as i64),
                    v: 1,
                })
                .collect();
            let next = advance(Bounds::default(), w, 0, &records);
            assert_eq!(next.seen.len(), 500, "{name}");
            assert_eq!(next.seen[0].k, "000000", "{name}: the newest survives");
            assert_eq!(next.seen[499].k, "000499", "{name}");
        });
    }

    /// **The ceiling clamp.** A record edited during the walk carries a
    /// stamp past the ceiling the run witnessed at its start; the watermark
    /// stops at the ceiling, so the next run re-walks the run's own duration.
    /// The record itself stays in `seen`, so that re-walk does not emit it.
    #[test]
    fn the_watermark_never_passes_the_ceiling() {
        both(|name, _, advance| {
            let ceiling = utc("2026-08-22T10:40:00Z");
            let mid_walk = rec("PAY-9", "2026-08-22T10:44:30Z");
            let next = advance(
                Bounds {
                    previous: Some(utc("2026-08-22T10:30:00Z")),
                    ceiling: Some(ceiling),
                },
                mid_walk.u,
                0,
                std::slice::from_ref(&mid_walk),
            );
            assert_eq!(next.watermark, ceiling, "{name}");
            assert!(delivered(&next.seen, "PAY-9", &1), "{name}");
            // Below the ceiling it binds nothing.
            let under = advance(
                Bounds {
                    previous: None,
                    ceiling: Some(ceiling),
                },
                utc("2026-08-22T10:39:00Z"),
                0,
                &[],
            );
            assert_eq!(under.watermark, utc("2026-08-22T10:39:00Z"), "{name}");
        });
    }

    /// **Never backwards.** A ceiling older than the previous watermark (the
    /// record that set it was deleted, or left the scope) does not drag the
    /// position back, and neither does a re-delivered older record.
    #[test]
    fn the_watermark_never_falls_below_the_previous_one() {
        both(|name, _, advance| {
            let previous = utc("2026-08-22T10:40:00Z");
            let next = advance(
                Bounds {
                    previous: Some(previous),
                    ceiling: Some(utc("2026-08-22T10:20:00Z")),
                },
                utc("2026-08-22T10:45:00Z"),
                0,
                &[],
            );
            assert_eq!(next.watermark, previous, "{name}: a low ceiling");
            let next = advance(
                Bounds {
                    previous: Some(previous),
                    ceiling: None,
                },
                utc("2026-08-22T10:35:00Z"),
                0,
                &[],
            );
            assert_eq!(next.watermark, previous, "{name}: an older record");
        });
    }

    /// The identity is the version, not the stamp: two edits inside one
    /// second are two versions.
    #[test]
    fn a_record_is_recognised_by_its_version_not_its_stamp() {
        let seen = [rec("PAY-1", "2026-08-22T10:40:00Z")];
        assert!(delivered(&seen, "PAY-1", &1));
        assert!(!delivered(&seen, "PAY-1", &2));
        assert!(!delivered(&seen, "PAY-2", &1));
    }
}
