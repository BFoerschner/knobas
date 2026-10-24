//! The one function that writes CQL.
//!
//! Data Center only: `/rest/api/content/search` takes a `cql` parameter and
//! pages with `_links.next`. Cloud's v2 API takes no CQL at all; when that
//! flavor is implemented it gets its own renderer beside this one.

use chrono::{DateTime, Utc};

use knobas_source::watermark::{Cql, MinuteLiteral};

use crate::ConfluenceConfig;

/// Which direction a query walks, and therefore what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// The sync walk: `order by created asc, id asc`, keys no edit can move
    /// (#1976). It pages by offset, and over `lastmodified` a page edited
    /// mid-walk moved to the end and shifted the row behind it past the next
    /// offset; a full sync then returned `Ok` without that page, and the
    /// sweep tombstoned it. `id` breaks ties on `created`, as it does for
    /// Jira (#1720). A deletion can still shift a row; the walk's anchored
    /// pages are what close that (`crate::walk`).
    Ascending,
    /// The run-start ceiling probe: one row, the newest thing in the corpus.
    Descending,
}

/// `type = page [AND space in (…)] [AND lastmodified >= "…"] order by
/// created asc, id asc` for the walk, `order by lastmodified desc` for the
/// ceiling probe ([`Order`]). The time bound stays on `lastmodified`: it is
/// what an edit moves, so it is what an incremental run is bounded by.
///
/// Nothing here escapes anything, and nothing needs to:
/// [`ConfluenceConfig::from_json`] has already refused a space key that is not
/// `~?[A-Za-z0-9_.-]+`, and the timestamp is rendered by
/// [`knobas_source::watermark::Cql`]. A key that reached this function cannot
/// contain a quote, a parenthesis or a comma.
///
/// The live suite sends this form through the adapter's full syncs
/// (`tests/live_confluence_seeded.rs`, in pages of two among them), which is
/// where Data Center 9.2 accepts `order by created asc, id asc`;
/// `a_space_list_becomes_a_quoted_in_clause` below is where the string is
/// pinned.
pub(crate) fn build_cql(
    cfg: &ConfluenceConfig,
    since: Option<DateTime<Utc>>,
    offset_secs: i32,
    order: Order,
) -> String {
    // `type = page` is the scope, not a filter on top of one: this adapter
    // emits exactly one kind, and a blog post or an attachment arriving as a
    // `page` item would be a kind the descriptor never declared.
    let order_by = match order {
        Order::Ascending => "created asc, id asc",
        Order::Descending => "lastmodified desc",
    };
    render_query("type = page", cfg, since, offset_secs, order_by)
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
    render_query(
        "mention = currentUser() AND type in (page, comment)",
        cfg,
        since,
        offset_secs,
        "lastmodified asc",
    )
}

/// `type = page [AND space in (…)] order by created asc` -- the **positions
/// sweep** (#1138), which reads where every page sits among its siblings.
///
/// A drag in Confluence's page tree changes `extensions.position` and nothing
/// else: no new version, no new `lastmodified` (measured on Data Center 9.2),
/// so no time bound can reach it and this query has none. It is ordered by
/// `created` rather than `lastmodified` because it pages by offset, and
/// `created` is the one order an edit made mid-sweep cannot shift a row in.
pub(crate) fn build_positions_cql(cfg: &ConfluenceConfig) -> String {
    format!(
        "{} order by created asc",
        scoped("type = page", cfg).join(" AND ")
    )
}

/// `type = page AND id in (…)` -- the pages the positions sweep found moved,
/// read again whole. The ids come from the server's own answer; any that is
/// not all digits is dropped rather than written into a query.
pub(crate) fn build_ids_cql(ids: &[&str]) -> String {
    let list = ids
        .iter()
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    format!("type = page AND id in ({list})")
}

/// The scope, then the space list when one is configured.
fn scoped(scope: &str, cfg: &ConfluenceConfig) -> Vec<String> {
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
    clauses
}

/// One CQL string: the scope, then the two clauses every walk shares, then the
/// ordering.
///
/// One renderer so the space list and the watermark literal cannot come out
/// differently for the two walks: the mention walk reads the *same* cursor,
/// so a bound rendered in another zone there would be a bound two hours wrong.
fn render_query(
    scope: &str,
    cfg: &ConfluenceConfig,
    since: Option<DateTime<Utc>>,
    offset_secs: i32,
    order_by: &str,
) -> String {
    let mut clauses = scoped(scope, cfg);
    if let Some(since) = since {
        clauses.push(format!(
            "lastmodified >= \"{}\"",
            Cql::render(since, offset_secs)
        ));
    }
    format!("{} order by {order_by}", clauses.join(" AND "))
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
            "type = page order by created asc, id asc"
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
            "type = page AND space in (\"ENG\", \"OPS\") order by created asc, id asc"
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
             order by created asc, id asc"
        );
        assert_eq!(
            build_cql(&cfg(json!({})), Some(since), 0, Order::Ascending),
            "type = page AND lastmodified >= \"2026-08-22 10:38\" order by created asc, id asc"
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

    /// The positions sweep (#1138): the page walk's scope, no time bound, and
    /// ordered by the field no edit moves.
    #[test]
    fn the_positions_sweep_is_the_scope_unbounded_in_creation_order() {
        assert_eq!(
            build_positions_cql(&cfg(json!({ "spaces": ["ENG", "OPS"] }))),
            "type = page AND space in (\"ENG\", \"OPS\") order by created asc"
        );
        assert_eq!(
            build_positions_cql(&cfg(json!({}))),
            "type = page order by created asc"
        );
    }

    /// The re-read names its pages by id, and only ids that are numbers reach
    /// the query.
    #[test]
    fn the_re_read_names_its_pages_by_numeric_id() {
        assert_eq!(
            build_ids_cql(&["360460", "360461"]),
            "type = page AND id in (360460, 360461)"
        );
        assert_eq!(
            build_ids_cql(&["360460", "1) OR (type = blogpost", ""]),
            "type = page AND id in (360460)"
        );
    }

    /// Ascending: the mention walk pages by offset too, and a descending walk
    /// over the field it orders by would duplicate rows ahead of the cursor
    /// rather than behind it. It stays on `lastmodified`: it delivers no
    /// kind of its own, and the page walk is what a full sync's sweep reads.
    #[test]
    fn the_mention_query_walks_ascending() {
        let q = build_mention_cql(&cfg(json!({})), None, 0);
        assert!(q.ends_with("order by lastmodified asc"), "{q}");
    }
}
