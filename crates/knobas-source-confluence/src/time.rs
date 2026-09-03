//! Confluence's timestamps in, CQL's date literals out.
//!
//! Two formats, and neither is interchangeable with the other:
//!
//! * **Responses** stamp `version.when` as ISO 8601 with an offset. Which
//!   offset is not knowable in advance and is never assumed here: measured on
//!   Confluence 9.2.21, this container renders **UTC with a `Z`**
//!   (`2026-09-03T11:58:04.419Z`), where Jira DC on the same host sends the
//!   same kind of field in the host's own zone (`+02:00`). That is why the
//!   offset is read back off a timestamp the server itself rendered rather
//!   than copied from the sibling adapter -- see the header of
//!   `tests/live_confluence_seeded.rs`, which records the measurement.
//! * **CQL** takes `"yyyy-MM-dd HH:mm"` with **no zone at all**, read in the
//!   instance's own time zone, at **one-minute resolution**. Both facts shape
//!   the cursor: the zone is taken from a timestamp the server itself
//!   rendered, and the minute resolution is why the watermark is queried with
//!   an overlap ([`crate::cursor`]).
//!
//! This is the Jira adapter's `time.rs` problem with one letter changed, and
//! the reasoning below is deliberately the same reasoning -- a second adapter
//! guessing the safe direction differently would be a second answer to a
//! question that has one.

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, TimeZone, Utc};

/// `strftime` for a CQL date literal: minute resolution, no zone.
const CQL_FMT: &str = "%Y-%m-%d %H:%M";

/// The lowest UTC offset any real time zone uses (UTC-12, Baker Island).
///
/// The fallback wherever the instance's own zone is unknown -- and
/// deliberately **not zero**. The literal is read back in the *instance's*
/// zone, so the query's real lower bound is
/// `intended + (assumed_offset - true_offset)`: assuming an offset *higher*
/// than the truth moves that bound forward and silently skips every edit in
/// between, on every run and never recovered. Assuming an offset no higher
/// than any real zone can only move it backwards, which re-reads work already
/// done -- and upserts are idempotent. Over-fetching is recoverable;
/// under-fetching is not.
pub const MIN_UTC_OFFSET_SECS: i32 = -12 * 3600;

/// Parse a Confluence timestamp into UTC.
///
/// Tolerates `+02:00`, `+0200` and `Z`, with or without a fractional part.
/// RFC 3339 first, because that is what this product actually sends.
pub(crate) fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    parse_fixed(s).map(|t| t.with_timezone(&Utc))
}

/// The UTC offset a Confluence timestamp was written in, in seconds.
///
/// This is how the adapter learns which zone CQL literals will be read in,
/// without a timezone database and without an administrator-only settings
/// endpoint: the server rendered `version.when` in that zone itself.
pub(crate) fn parse_offset_secs(s: &str) -> Option<i32> {
    parse_fixed(s).map(|t| t.offset().local_minus_utc())
}

fn parse_fixed(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
}

/// The zone CQL literals are rendered in, falling back safely.
///
/// Shared by [`format_cql_time`] and [`cql_floor`] so the two cannot disagree
/// about which zone a literal meant.
fn zone_of(offset_secs: i32) -> FixedOffset {
    FixedOffset::east_opt(offset_secs)
        .or_else(|| FixedOffset::east_opt(MIN_UTC_OFFSET_SECS))
        .unwrap_or_else(|| Utc.fix())
}

/// Render `t` as the zone-less CQL literal Confluence will read back in the
/// instance's own time zone.
///
/// **Lossy on purpose, and the loss is load-bearing.** CQL has minute
/// resolution, so the seconds are dropped and Confluence reads the literal as
/// the start of that minute -- up to 59 s *earlier* than `t`. Anything
/// deciding what that query will return must ask [`cql_floor`], never `t`.
pub(crate) fn format_cql_time(t: DateTime<Utc>, offset_secs: i32) -> String {
    t.with_timezone(&zone_of(offset_secs))
        .format(CQL_FMT)
        .to_string()
}

/// The instant Confluence will actually treat
/// `lastmodified >= "<format_cql_time(t)>"` as.
///
/// Derived by rendering the literal and reading it back, rather than by
/// truncating `t` to the minute independently: the two would then be free to
/// drift, and drift is precisely the bug this exists to close. The cursor's
/// `seen` set is filtered on this, so it covers exactly the band the next
/// query returns -- one second of mismatch and an already-delivered page comes
/// back unrecognised, dragging the watermark backwards on every poll.
///
/// Truncation is toward the past, so the result is never later than `t`.
pub(crate) fn cql_floor(t: DateTime<Utc>, offset_secs: i32) -> DateTime<Utc> {
    let zone = zone_of(offset_secs);
    NaiveDateTime::parse_from_str(&format_cql_time(t, offset_secs), CQL_FMT)
        .ok()
        .and_then(|naive| zone.from_local_datetime(&naive).single())
        .map(|t| t.with_timezone(&Utc))
        // Unreachable: the string was just produced from this same format. A
        // whole minute back is the safe direction if it ever were reached.
        .unwrap_or_else(|| t - chrono::Duration::minutes(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().expect("a test timestamp")
    }

    /// The shape this container sends, plus the two spellings a differently
    /// configured instance or a proxy may send instead.
    #[test]
    fn the_response_timestamp_parses_in_every_spelling_confluence_uses() {
        for (raw, want) in [
            ("2026-08-22T12:40:00.000+02:00", "2026-08-22T10:40:00+00:00"),
            ("2026-08-22T12:40:00+0200", "2026-08-22T10:40:00+00:00"),
            ("2026-08-22T10:40:00.123Z", "2026-08-22T10:40:00.123+00:00"),
        ] {
            assert_eq!(
                parse_time(raw).map(|t| t.to_rfc3339()),
                Some(want.to_owned()),
                "{raw}"
            );
        }
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(parse_time(""), None);
    }

    /// The offset is the whole point of reading a timestamp the server
    /// rendered: it is the zone the CQL literal will be read back in.
    #[test]
    fn the_offset_is_read_off_the_servers_own_rendering() {
        assert_eq!(
            parse_offset_secs("2026-08-22T12:40:00.000+02:00"),
            Some(7200)
        );
        assert_eq!(
            parse_offset_secs("2026-08-22T05:40:00.000-05:00"),
            Some(-5 * 3600)
        );
        assert_eq!(parse_offset_secs("2026-08-22T10:40:00Z"), Some(0));
        assert_eq!(parse_offset_secs("not a time"), None);
    }

    /// Minute resolution, no zone, in the instance's own zone. A literal that
    /// carried a zone, or seconds, is one Confluence answers 400 to.
    #[test]
    fn a_cql_literal_is_minute_resolution_in_the_instances_zone() {
        let t = utc("2026-08-22T10:40:59Z");
        assert_eq!(format_cql_time(t, 7200), "2026-08-22 12:40");
        assert_eq!(format_cql_time(t, 0), "2026-08-22 10:40");
        assert_eq!(format_cql_time(t, -5 * 3600), "2026-08-22 05:40");
    }

    /// The floor is what the *query* means, which is up to 59 s before the
    /// instant asked for. Getting this wrong by a second is what makes an
    /// already-delivered page come back unrecognised forever.
    #[test]
    fn the_floor_is_the_query_read_back_not_the_instant_asked_for() {
        let t = utc("2026-08-22T10:40:59Z");
        assert_eq!(cql_floor(t, 7200), utc("2026-08-22T10:40:00Z"));
        assert_eq!(cql_floor(t, -5 * 3600), utc("2026-08-22T10:40:00Z"));
        // Already on a minute boundary: the floor is the instant itself.
        let exact = utc("2026-08-22T10:40:00Z");
        assert_eq!(cql_floor(exact, 7200), exact);
        assert!(cql_floor(t, 7200) <= t, "truncation is toward the past");
    }

    /// A zone knobas could not read must not be guessed *high*: that moves the
    /// query's lower bound forward and loses edits permanently, while guessing
    /// low only re-reads work that upserts absorb.
    #[test]
    fn an_unreadable_zone_falls_back_to_the_lowest_real_offset_not_to_utc() {
        let t = utc("2026-08-22T10:40:00Z");
        assert_ne!(MIN_UTC_OFFSET_SECS, 0);
        // An offset no `FixedOffset` can hold takes the same safe road.
        assert_eq!(
            format_cql_time(t, i32::MAX),
            format_cql_time(t, MIN_UTC_OFFSET_SECS)
        );
    }
}
