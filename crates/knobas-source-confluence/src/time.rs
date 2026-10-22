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
//! Rendering the literal, its floor and the zone fallback are
//! [`knobas_source::watermark::Cql`]'s, shared with Jira's JQL (#1622): a
//! second adapter guessing the safe direction differently would be a second
//! answer to a question that has one.

use chrono::{DateTime, FixedOffset, Utc};

/// The fallback wherever the instance's zone is unknown: UTC-12, never zero
/// (see the constant's own reasoning). Shared with Jira (#1622).
pub use knobas_source::watermark::MIN_UTC_OFFSET_SECS;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
