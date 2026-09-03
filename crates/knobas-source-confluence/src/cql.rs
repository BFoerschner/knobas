//! The one function that writes CQL.
//!
//! Data Center only: `/rest/api/content/search` takes a `cql` parameter and
//! pages with `_links.next`. Cloud's v2 API takes no CQL at all; when that
//! flavor is implemented it gets its own renderer beside this one.

use chrono::{DateTime, Utc};

use crate::ConfluenceConfig;

/// Which direction a query walks, and therefore what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// The sync walk. **Ascending is what makes offset paging safe**: a page
    /// edited mid-walk moves toward the end, so it is re-visited or missed,
    /// never duplicated ahead of the cursor -- and the ceiling clamp
    /// ([`crate::cursor`]) is what brings a missed one back next run.
    Ascending,
    /// The run-start ceiling probe: one row, the newest thing in the corpus.
    Descending,
}

/// `type = page [AND space in (…)] [AND lastmodified >= "…"] ORDER BY
/// lastmodified <asc|desc>`.
///
/// Nothing here escapes anything, and nothing needs to:
/// [`ConfluenceConfig::from_json`] has already refused a space key that is not
/// `~?[A-Za-z0-9_.-]+`, and the timestamp is rendered by
/// [`crate::time::format_cql_time`]. A key that reached this function cannot
/// contain a quote, a parenthesis or a comma.
pub(crate) fn build_cql(
    cfg: &ConfluenceConfig,
    since: Option<DateTime<Utc>>,
    offset_secs: i32,
    order: Order,
) -> String {
    // `type = page` is the scope, not a filter on top of one: this adapter
    // emits exactly one kind, and a blog post or an attachment arriving as a
    // `page` item would be a kind the descriptor never declared.
    let mut clauses = vec!["type = page".to_owned()];
    if !cfg.spaces.is_empty() {
        let list = cfg
            .spaces
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!("space in ({list})"));
    }
    if let Some(since) = since {
        clauses.push(format!(
            "lastmodified >= \"{}\"",
            crate::time::format_cql_time(since, offset_secs)
        ));
    }
    let direction = match order {
        Order::Ascending => "asc",
        Order::Descending => "desc",
    };
    format!(
        "{} order by lastmodified {direction}",
        clauses.join(" AND ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(value: serde_json::Value) -> ConfluenceConfig {
        ConfluenceConfig::from_json(&value).expect("a valid configuration")
    }

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().expect("a test timestamp")
    }

    /// An unconfigured source asks for every page the account can see, with no
    /// time bound -- which is what makes the `page` kind's
    /// `full_sync_exhaustive: true` claim true, and is asserted from the
    /// descriptor's side too.
    #[test]
    fn a_full_sync_of_every_space_is_one_unbounded_clause() {
        assert_eq!(
            build_cql(&cfg(json!({})), None, 7200, Order::Ascending),
            "type = page order by lastmodified asc"
        );
    }

    /// A configured space list narrows the corpus, and the keys are quoted --
    /// a bare `space in (ENG)` is legal CQL only for some keys, and quoting
    /// every one of them is what makes the validated grammar sufficient.
    #[test]
    fn a_space_list_becomes_a_quoted_in_clause() {
        assert_eq!(
            build_cql(
                &cfg(json!({ "spaces": ["ENG", "OPS"] })),
                None,
                7200,
                Order::Ascending
            ),
            "type = page AND space in (\"ENG\", \"OPS\") order by lastmodified asc"
        );
    }

    /// The incremental clause, in the instance's own zone at minute
    /// resolution. The literal is the one thing here that is computed rather
    /// than configured, so it is pinned against the zone it was rendered in.
    #[test]
    fn a_watermark_becomes_a_lastmodified_bound_in_the_instances_zone() {
        let since = utc("2026-08-22T10:38:00Z");
        assert_eq!(
            build_cql(
                &cfg(json!({ "spaces": ["ENG"] })),
                Some(since),
                7200,
                Order::Ascending
            ),
            "type = page AND space in (\"ENG\") AND lastmodified >= \"2026-08-22 12:38\" \
             order by lastmodified asc"
        );
        assert_eq!(
            build_cql(&cfg(json!({})), Some(since), 0, Order::Ascending),
            "type = page AND lastmodified >= \"2026-08-22 10:38\" order by lastmodified asc"
        );
    }

    /// The ceiling probe is the same scope read from the other end: it must
    /// see the newest page in **this source's** corpus, not in the wiki, or a
    /// clamp taken from another space's edit would be no clamp at all.
    #[test]
    fn the_ceiling_probe_is_the_same_scope_ordered_the_other_way() {
        let probe = build_cql(
            &cfg(json!({ "spaces": ["ENG"] })),
            None,
            7200,
            Order::Descending,
        );
        assert_eq!(
            probe,
            "type = page AND space in (\"ENG\") order by lastmodified desc"
        );
        assert!(
            !probe.contains("lastmodified >="),
            "the probe is never time-bounded: it is asking what the newest thing *is*"
        );
    }
}
