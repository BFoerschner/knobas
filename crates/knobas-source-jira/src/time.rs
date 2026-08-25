//! Jira's timestamps in, JQL's date literals out.
//!
//! Two formats, and neither is RFC 3339:
//!
//! * **Responses** stamp `2026-08-22T13:48:00.000+0200` -- an offset without a
//!   colon.
//! * **JQL** takes `"yyyy-MM-dd HH:mm"` with **no zone at all**, read in the
//!   user's own time zone, at one-minute resolution. Both facts shape the
//!   cursor: the zone is taken from `serverInfo.serverTime`, and the minute
//!   resolution is why the watermark is queried with a two-minute overlap
//!   (interfaces doc §4.2).

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, TimeZone, Utc};

/// `strftime` for a JQL date literal: minute resolution, no zone.
const JQL_FMT: &str = "%Y-%m-%d %H:%M";

/// The lowest UTC offset any real time zone uses (UTC-12, Baker Island).
///
/// The fallback wherever the server's own zone is unknown -- and deliberately
/// **not zero**. The literal is read back in the *server's* zone, so the query's
/// real lower bound is `intended + (assumed_offset - true_offset)`: assuming an
/// offset *higher* than the truth moves that bound forward and silently skips
/// every edit in between, five hours of them for a UTC-05 server, on every run
/// and never recovered. Assuming an offset no higher than any real zone can only
/// move it backwards, which re-reads work already done -- and upserts are
/// idempotent. Over-fetching is recoverable; under-fetching is not.
pub(crate) const MIN_UTC_OFFSET_SECS: i32 = -12 * 3600;

/// The response format, with a tolerated fractional part and a colon-less
/// offset. Tried before RFC 3339 because it is what Jira DC actually sends.
const JIRA_FMT: &str = "%Y-%m-%dT%H:%M:%S%.f%z";

/// Parse a Jira timestamp into UTC. Tolerates `+0200`, `+02:00` and `Z`, with
/// or without a fractional part.
pub(crate) fn parse_jira_time(s: &str) -> Option<DateTime<Utc>> {
    parse_fixed(s).map(|t| t.with_timezone(&Utc))
}

/// The UTC offset a Jira timestamp was written in, in seconds.
///
/// This is how the adapter learns which zone JQL literals will be read in --
/// `serverInfo.serverTime` carries the server's clock *with its offset*, and
/// no timezone database is needed to use it.
pub(crate) fn parse_offset_secs(s: &str) -> Option<i32> {
    parse_fixed(s).map(|t| t.offset().local_minus_utc())
}

fn parse_fixed(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_str(s, JIRA_FMT)
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .ok()
}

/// The zone JQL literals are rendered in, falling back safely.
///
/// An offset no `FixedOffset` can hold falls back to [`MIN_UTC_OFFSET_SECS`],
/// not to UTC, for the reason spelled out on that constant: guessing high
/// skips edits permanently, guessing low only re-reads them. Shared by
/// [`format_jql_time`] and [`jql_floor`] so the two cannot disagree about
/// which zone a literal meant.
fn zone_of(offset_secs: i32) -> FixedOffset {
    FixedOffset::east_opt(offset_secs)
        .or_else(|| FixedOffset::east_opt(MIN_UTC_OFFSET_SECS))
        .unwrap_or_else(|| Utc.fix())
}

/// Render `t` as the zone-less JQL literal Jira will read back in the server's
/// own time zone.
///
/// **Lossy on purpose, and the loss is load-bearing.** JQL has minute
/// resolution, so the seconds are dropped and Jira reads the literal as the
/// start of that minute -- up to 59 s *earlier* than `t`. Anything deciding
/// what that query will return must ask [`jql_floor`], never `t`.
pub(crate) fn format_jql_time(t: DateTime<Utc>, offset_secs: i32) -> String {
    t.with_timezone(&zone_of(offset_secs))
        .format(JQL_FMT)
        .to_string()
}

/// The instant Jira will actually treat `updated >= "<format_jql_time(t)>"` as.
///
/// Derived by rendering the literal and reading it back, rather than by
/// truncating `t` to the minute independently: the two would then be free to
/// drift, and drift is precisely the bug this exists to close. The cursor's
/// `seen` set is filtered on this, so it covers exactly the band the next
/// query returns -- one second of mismatch and an already-delivered issue comes
/// back unrecognised, dragging the watermark backwards on every poll.
///
/// Truncation is toward the past, so the result is never later than `t`.
pub(crate) fn jql_floor(t: DateTime<Utc>, offset_secs: i32) -> DateTime<Utc> {
    let zone = zone_of(offset_secs);
    NaiveDateTime::parse_from_str(&format_jql_time(t, offset_secs), JQL_FMT)
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

    /// Jira DC stamps `2026-08-22T13:48:00.000+0200` -- an offset with no colon,
    /// which is *not* RFC 3339. `DateTime::parse_from_rfc3339` rejects it, so an
    /// adapter that reaches for the obvious function silently gets `None` for
    /// every timestamp and never advances its watermark.
    #[test]
    fn parses_jiras_own_offset_form() {
        let t = parse_jira_time("2026-08-22T13:48:00.000+0200").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-08-22T11:48:00+00:00");
        assert_eq!(parse_jira_time("2026-08-22T11:48:00.000+0000").unwrap(), t);
        // Tolerated variants: no fraction, and a real RFC 3339 stamp.
        assert!(parse_jira_time("2026-08-22T11:48:00+0000").is_some());
        assert!(parse_jira_time("2026-08-22T11:48:00Z").is_some());
        assert!(parse_jira_time("2026-08-22T13:48:00.000+02:00").is_some());
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for bad in ["", "yesterday", "2026-08-22", "2026-08-22 11:48"] {
            assert!(parse_jira_time(bad).is_none(), "{bad:?}");
        }
    }

    /// `serverInfo.serverTime` is how the adapter learns the server's UTC
    /// offset without a timezone database.
    #[test]
    fn reads_the_offset_off_a_server_timestamp() {
        assert_eq!(
            parse_offset_secs("2026-08-22T13:48:00.000+0200"),
            Some(7_200)
        );
        assert_eq!(parse_offset_secs("2026-08-22T11:48:00.000+0000"), Some(0));
        assert_eq!(
            parse_offset_secs("2026-08-22T06:48:00.000-0500"),
            Some(-18_000)
        );
        assert_eq!(parse_offset_secs("nonsense"), None);
    }

    /// JQL date literals carry no zone: Jira reads them in the *user's* time
    /// zone. Formatting a UTC watermark as if it were local is how an adapter
    /// against a UTC+2 server silently queries two hours into the future and
    /// loses every issue in between.
    #[test]
    fn renders_a_jql_literal_in_the_servers_zone() {
        let t = parse_jira_time("2026-08-22T11:46:00.000+0000").unwrap();
        assert_eq!(format_jql_time(t, 7_200), "2026-08-22 13:46");
        assert_eq!(format_jql_time(t, 0), "2026-08-22 11:46");
        assert_eq!(format_jql_time(t, -18_000), "2026-08-22 06:46");
    }

    /// An offset outside the range a fixed offset can hold is not a reason to
    /// render nothing: a query one zone off still returns issues, a query with
    /// no watermark clause returns the corpus. It falls back to the *lowest*
    /// real offset, not to UTC -- see `never_shifts_the_query_forward` below.
    #[test]
    fn an_impossible_offset_falls_back_to_the_safe_direction() {
        let t = parse_jira_time("2026-08-22T11:46:00.000+0000").unwrap();
        assert_eq!(
            format_jql_time(t, i32::MAX),
            format_jql_time(t, MIN_UTC_OFFSET_SECS)
        );
        assert_eq!(format_jql_time(t, i32::MAX), "2026-08-21 23:46");
    }

    /// Read a JQL literal the way a server in `offset_secs` would.
    ///
    /// Independent of [`jql_floor`] on purpose: using that here would test the
    /// implementation against itself.
    fn read_as(literal: &str, offset_secs: i32) -> DateTime<Utc> {
        let zone = FixedOffset::east_opt(offset_secs).expect("a real offset");
        chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M")
            .expect("a literal this crate rendered")
            .and_local_timezone(zone)
            .single()
            .expect("a fixed offset has no ambiguous local times")
            .with_timezone(&Utc)
    }

    /// **The invariant the whole watermark rests on.** A literal must be read
    /// back as an instant no *later* than the one it was rendered from --
    /// later means `updated >=` starts past edits that were never delivered,
    /// and nothing ever goes back for them.
    ///
    /// Checked across every real zone, because the failure is silent and
    /// permanent rather than loud and once.
    #[test]
    fn rendering_a_literal_never_shifts_the_query_forward() {
        let t = parse_jira_time("2026-08-22T11:46:40.000+0000").unwrap();
        for hours in -12..=14 {
            let offset = hours * 3_600;
            // The zone is known: rendered and read in the same one.
            let known = read_as(&format_jql_time(t, offset), offset);
            assert!(known <= t, "offset {hours}: {known} is later than {t}");
            assert!(
                known == jql_floor(t, offset),
                "offset {hours}: floor disagrees"
            );

            // The zone is *unknown*, so the fallback is used and the server is
            // really in `offset`. This is the case a UTC fallback gets wrong.
            let guessed = read_as(&format_jql_time(t, MIN_UTC_OFFSET_SECS), offset);
            assert!(
                guessed <= t,
                "offset {hours}: an unknown server zone shifted the query to {guessed}, past {t}"
            );
        }
    }

    /// The minute the literal truncates to, which is what the query really
    /// asks for -- and is up to 59 s earlier than the timestamp it came from.
    #[test]
    fn the_query_floor_is_the_start_of_the_rendered_minute() {
        let t = parse_jira_time("2026-08-22T11:46:40.000+0000").unwrap();
        assert_eq!(
            jql_floor(t, 7_200),
            parse_jira_time("2026-08-22T11:46:00.000+0000").unwrap()
        );
        // Already on a minute boundary: nothing to truncate.
        let exact = parse_jira_time("2026-08-22T11:46:00.000+0000").unwrap();
        assert_eq!(jql_floor(exact, 7_200), exact);
        // The zone does not move the floor, because every real offset is a
        // whole number of minutes.
        assert_eq!(jql_floor(t, 0), jql_floor(t, 7_200));
        assert_eq!(jql_floor(t, -18_000), jql_floor(t, 7_200));
        // A 45-minute zone (Nepal) is still whole minutes.
        assert_eq!(jql_floor(t, 20_700), jql_floor(t, 7_200));
    }
}
