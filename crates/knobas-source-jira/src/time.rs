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
//!   (interfaces doc §4.2). Rendering the literal is
//!   [`knobas_source::watermark::Jql`]'s, shared with Confluence's CQL (#1622).

use chrono::{DateTime, FixedOffset, Utc};

/// The fallback wherever the server's zone is unknown: UTC-12, never zero
/// (see the constant's own reasoning).
pub(crate) use knobas_source::watermark::MIN_UTC_OFFSET_SECS;

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
}
