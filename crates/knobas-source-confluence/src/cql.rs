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
    render("type = page", cfg, since, offset_secs, order)
}

/// `mention = currentUser() AND type in (page, comment) [AND space in (…)]
/// [AND lastmodified >= "…"] ORDER BY lastmodified asc`.
///
/// **Why a second query and not a filter on the first.** The page walk is
/// bounded by a *page's* `lastmodified`, and a comment is separate content in
/// Confluence: posting one does not touch the page it hangs off. So a comment
/// that names you on a page nobody has edited in a year is invisible to the
/// page walk for ever -- and a comment is where a mention usually lives. This
/// query is bounded by the **comment's own** `lastmodified`, which is what
/// reaches it.
///
/// **Why `mention = currentUser()` and not a text match.** A mention is stored
/// as a user *key*; only the server can resolve one to the credential's
/// account, and `currentUser()` is it asking itself. It also needs no
/// escaping and no username in the query, so a rename at the source cannot
/// silently narrow the scope to nobody.
///
/// `type in (page, comment)` is both places a mention can be: a page body and
/// a comment on one. Blog posts and attachments stay out for the reason
/// [`build_cql`] gives -- this adapter emits one kind, and the entity a
/// mention produces is the page (`crate::sync`).
pub(crate) fn build_mention_cql(
    cfg: &ConfluenceConfig,
    since: Option<DateTime<Utc>>,
    offset_secs: i32,
) -> String {
    render(
        "mention = currentUser() AND type in (page, comment)",
        cfg,
        since,
        offset_secs,
        Order::Ascending,
    )
}

/// The scope, then the two clauses every walk shares, then the ordering.
///
/// One renderer so the space list and the watermark literal cannot come out
/// differently for the two walks: the mention walk reads the *same* cursor,
/// so a bound rendered in another zone there would be a bound two hours wrong.
fn render(
    scope: &str,
    cfg: &ConfluenceConfig,
    since: Option<DateTime<Utc>>,
    offset_secs: i32,
    order: Order,
) -> String {
    let mut clauses = vec![scope.to_owned()];
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

    /// The mention walk's scope, in full: the server resolves the identity,
    /// both kinds a mention can live in are in it, and the space list narrows
    /// it exactly as it narrows the page walk.
    #[test]
    fn the_mention_query_asks_the_server_who_the_credential_is() {
        assert_eq!(
            build_mention_cql(&cfg(json!({ "spaces": ["ENG"] })), None, 7200),
            "mention = currentUser() AND type in (page, comment) AND space in (\"ENG\") \
             order by lastmodified asc"
        );
        assert_eq!(
            build_mention_cql(&cfg(json!({})), None, 0),
            "mention = currentUser() AND type in (page, comment) order by lastmodified asc"
        );
    }

    /// **The cursor bound, and the reason the mention walk exists.** A comment
    /// is separate content, so its `lastmodified` is its own -- the bound has
    /// to be here or every run would re-walk every mention the account has
    /// ever had, and a corpus that grew past the page cap would fail the run.
    /// It is rendered in the instance's zone by the same code path the page
    /// walk uses, so the two walks read one cursor the same way.
    #[test]
    fn the_mention_query_is_bounded_by_the_cursor_in_the_instances_zone() {
        let since = utc("2026-08-22T10:38:00Z");
        let bounded = build_mention_cql(&cfg(json!({ "spaces": ["ENG"] })), Some(since), 7200);
        assert_eq!(
            bounded,
            "mention = currentUser() AND type in (page, comment) AND space in (\"ENG\") \
             AND lastmodified >= \"2026-08-22 12:38\" order by lastmodified asc"
        );
        assert_eq!(
            build_mention_cql(&cfg(json!({})), Some(since), 0),
            "mention = currentUser() AND type in (page, comment) AND lastmodified >= \
             \"2026-08-22 10:38\" order by lastmodified asc"
        );
    }

    /// Ascending, for the reason [`Order::Ascending`] gives: the mention walk
    /// pages by offset too, and a descending walk over the field it orders by
    /// would duplicate rows ahead of the cursor rather than behind it.
    #[test]
    fn the_mention_query_walks_the_same_direction_the_page_walk_does() {
        let q = build_mention_cql(&cfg(json!({})), None, 0);
        assert!(q.ends_with("order by lastmodified asc"), "{q}");
    }
}
