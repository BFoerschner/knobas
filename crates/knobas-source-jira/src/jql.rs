//! The one function that writes JQL.
//!
//! Data Center only (roadmap §4 gotcha 4): `/rest/api/2/search` takes a `jql`
//! parameter and pages with `startAt`. Cloud's `/rest/api/3/search/jql` takes a
//! bounded JQL and pages with `nextPageToken`; when that flavor is implemented
//! it gets its own renderer beside this one, because the two differ in what the
//! query may contain, not just in how it is sent.

use chrono::{DateTime, Utc};

use knobas_source::watermark::{Jql, MinuteLiteral};

use crate::JiraConfig;

/// The walk's order: two keys, both of which an edit leaves where they were.
const WALK_ORDER: &str = "ORDER BY created ASC, id ASC";

/// `<scope> AND updated >= "<since>" ORDER BY created ASC, id ASC`, with
/// either half omitted when it does not apply.
///
/// The filter is on `updated` and the order on `created` (#1623, ruling A1):
/// the walk pages with `startAt`, and offset paging is only as sound as its
/// sort key is still. `id` breaks ties on `created` (#1720): see the tests for
/// the arithmetic.
///
/// Nothing here escapes anything, and nothing needs to:
/// [`JiraConfig::from_json`] has already refused a project key that is not
/// `[A-Z][A-Z0-9_]{0,31}` and a filter carrying its own `ORDER BY`, and the
/// timestamp is rendered by [`knobas_source::watermark::Jql`]. A key that
/// reached this function cannot contain a quote or a parenthesis.
pub(crate) fn build_jql(
    cfg: &JiraConfig,
    since: Option<DateTime<Utc>>,
    server_offset_secs: i32,
) -> String {
    let mut clauses: Vec<String> = scope(cfg).into_iter().collect();
    if let Some(since) = since {
        clauses.push(format!(
            "updated >= \"{}\"",
            Jql::render(since, server_offset_secs)
        ));
    }
    // `created`, which an edit cannot move, then `id`, which nothing can, is
    // what makes startAt paging safe over a live index; see the tests.
    if clauses.is_empty() {
        WALK_ORDER.to_owned()
    } else {
        format!("{} {WALK_ORDER}", clauses.join(" AND "))
    }
}

/// The run-start ceiling probe (#1622): `<scope> ORDER BY updated DESC`,
/// asked for one row. **The same scope** as the walk, because a ceiling taken
/// from a project this source does not sync is a clamp that never binds.
pub(crate) fn build_ceiling_jql(cfg: &JiraConfig) -> String {
    match scope(cfg) {
        Some(scope) => format!("{scope} ORDER BY updated DESC"),
        None => "ORDER BY updated DESC".to_owned(),
    }
}

/// The source's scope clause: its filter, else its projects, else nothing.
fn scope(cfg: &JiraConfig) -> Option<String> {
    if let Some(filter) = &cfg.jql_filter {
        // Parenthesised: a filter of `a OR b` would otherwise bind looser than
        // the watermark clause and return the whole of `b` on every run.
        Some(format!("({})", filter.trim()))
    } else if !cfg.projects.is_empty() {
        let list = cfg
            .projects
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("project in ({list})"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(value: serde_json::Value) -> crate::JiraConfig {
        crate::JiraConfig::from_json(&value).unwrap()
    }

    fn watermark() -> chrono::DateTime<chrono::Utc> {
        crate::time::parse_jira_time("2026-08-22T11:46:00.000+0000").unwrap()
    }

    /// The ceiling probe reads the walk's scope, newest first. One row, so
    /// the descending order the walk must never use is harmless here.
    #[test]
    fn the_ceiling_probe_is_the_walks_scope_newest_first() {
        assert_eq!(
            build_ceiling_jql(&cfg(json!({ "projects": ["PAY", "OPS"] }))),
            r#"project in ("PAY", "OPS") ORDER BY updated DESC"#
        );
        assert_eq!(
            build_ceiling_jql(&cfg(
                json!({ "jql_filter": "assignee = currentUser() OR x = 1" })
            )),
            "(assignee = currentUser() OR x = 1) ORDER BY updated DESC"
        );
        assert_eq!(build_ceiling_jql(&cfg(json!({}))), "ORDER BY updated DESC");
    }

    /// ORDER BY **created** is load-bearing, not cosmetic (#1623). The walk
    /// pages with `startAt`, advanced by the rows it has read, over a live
    /// index. Over `updated`, an issue on a page already read that is edited
    /// mid-walk moves to the end, every row behind it shifts up by one, and
    /// the first unread issue lands inside the window already read: no later
    /// request returns it, `total` does not move, and the run ends `Ok` with
    /// a hole the sweep then tombstones. `created` never changes, so an edit
    /// moves nothing; an issue filed mid-walk joins the end, where the walk
    /// still reads it. What is left is a deletion, which shifts rows up; the
    /// walk asks each page from the last issue read so it hides nothing
    /// (`crate::walk`, `sync::tests::an_issue_deleted_behind_the_walk_hides_no_unread_one`).
    ///
    /// **`id ASC` after it is load-bearing too** (#1720). Issues that share
    /// one `created` stamp -- an import at minute or day resolution files a
    /// whole batch that way -- come back in Jira's index order, and an edit
    /// reindexes the issue to the end of its group, which is the same shift
    /// inside the group. `id` is unique and nothing changes it; `key` would
    /// change on a move to another project. Measured on the seeded Jira:
    /// `live_jira_seeded`'s
    /// `an_edit_reorders_issues_tied_on_created_and_id_holds_them_still`.
    #[test]
    fn a_full_sync_is_scoped_and_ordered() {
        assert_eq!(
            build_jql(&cfg(json!({ "projects": ["PAY", "OPS"] })), None, 0),
            r#"project in ("PAY", "OPS") ORDER BY created ASC, id ASC"#
        );
    }

    #[test]
    fn an_unscoped_full_sync_is_still_valid_jql() {
        assert_eq!(
            build_jql(&cfg(json!({})), None, 0),
            "ORDER BY created ASC, id ASC"
        );
    }

    /// The literal is rendered in the server's zone, because JQL has none.
    #[test]
    fn an_incremental_query_carries_the_watermark_in_the_servers_zone() {
        assert_eq!(
            build_jql(
                &cfg(json!({ "projects": ["PAY"] })),
                Some(watermark()),
                7_200
            ),
            r#"project in ("PAY") AND updated >= "2026-08-22 13:46" ORDER BY created ASC, id ASC"#
        );
        assert_eq!(
            build_jql(&cfg(json!({})), Some(watermark()), 0),
            r#"updated >= "2026-08-22 11:46" ORDER BY created ASC, id ASC"#
        );
    }

    /// A user's own JQL is parenthesised so `a OR b` cannot swallow the
    /// watermark clause.
    #[test]
    fn a_configured_filter_is_parenthesised() {
        assert_eq!(
            build_jql(
                &cfg(json!({ "jql_filter": "labels = sepa OR priority = High" })),
                Some(watermark()),
                0
            ),
            r#"(labels = sepa OR priority = High) AND updated >= "2026-08-22 11:46" ORDER BY created ASC, id ASC"#
        );
    }

    /// Project keys are validated at configuration time, so quoting them here
    /// cannot produce a broken -- or a hostile -- query.
    #[test]
    fn project_keys_are_quoted() {
        assert!(build_jql(&cfg(json!({ "projects": ["TW_2"] })), None, 0).contains(r#""TW_2""#));
    }
}
