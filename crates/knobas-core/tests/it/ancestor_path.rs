//! A Confluence page's path line, joined by SQL, against a real PostgreSQL
//! (#284, read through the declaration since #646).
//!
//! #284 drew a page's ancestors with `ancestor_path_read!`, a payload read
//! outside an adapter under ADR-0007's interim discipline, and this file
//! pinned its failure direction. #599 had Confluence declare its page parent
//! on that same `ancestors` array, and #646 retired the read: a page's line is
//! now [`path_line_read!`](knobas_core::path_line_read) over the declaration,
//! like every other row's. What this file still pins is what a person reads
//! under a page, through the one expression every row expands, bound to a
//! declaration shaped like Confluence's (`ancestors`, each answering to its
//! `title`). `payload_paths.rs` holds the SQL to its Rust twin; this file
//! holds the line to the shapes a page's `ancestors` can arrive in.
//!
//! **The direction is still absence.** Every unusable shape below yields SQL
//! `null`, not an empty string and not a partial guess, so a row shows no path
//! rather than a wrong one. `string_agg` over no rows is `null` by
//! construction, which is what makes that true without a guard anybody has to
//! remember to keep.

use knobas_core::payload::{Declarations, KindPaths, ParentPath, PayloadPath};
use sqlx::{PgPool, Row};

/// The statement under test: the line a row reads, over a page payload bound
/// as `$1` and the declarations bound as `$2`.
///
/// The macro takes its declaration expression as a literal, so this is the
/// same `&'static str` the launcher and the room statements expand -- not a
/// paraphrase of it.
const READ: &str = concat!(
    "select ",
    knobas_core::path_line_read!("$2"),
    " as path from (select $1::jsonb as payload, 'confluence'::text as source_id, \
      'page'::text as kind) i"
);

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("ancestor_path")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database")
}

/// A page declared as Confluence declares it: its parent is every ancestor,
/// outermost first, each answering to its `title`.
fn confluence() -> Declarations {
    Declarations::empty().with(
        "confluence",
        vec![KindPaths {
            kind: "page".to_owned(),
            parent: vec![ParentPath {
                kind: "page".to_owned(),
                at: PayloadPath::of(["ancestors"]),
                names: vec![PayloadPath::of(["title"])],
            }],
            ..KindPaths::default()
        }],
    )
}

/// What the line answers for one page payload.
async fn path_of(pool: &PgPool, payload: serde_json::Value) -> Option<String> {
    sqlx::query(READ)
        .bind(payload)
        .bind(confluence().as_param())
        .fetch_one(pool)
        .await
        .expect("the path line is valid SQL")
        .get::<Option<String>, _>("path")
}

/// The shape a Confluence page actually arrives in: `ancestors`,
/// **outermost first**, each an object with a `title`.
///
/// Order is the whole meaning of a path, so it is asserted rather than
/// assumed.
#[tokio::test]
async fn a_pages_ancestors_join_outermost_first() {
    let pool = scratch().await;
    let path = path_of(
        &pool,
        serde_json::json!({
            "id": "98307",
            "ancestors": [
                { "id": "65537", "title": "Engineering" },
                { "id": "65540", "title": "Payments" },
                { "id": "65544", "title": "SEPA" }
            ]
        }),
    )
    .await;
    assert_eq!(path.as_deref(), Some("Engineering › Payments › SEPA"));

    // One ancestor is a path of one: a page directly under the space home.
    let path = path_of(
        &pool,
        serde_json::json!({ "ancestors": [{ "title": "Engineering" }] }),
    )
    .await;
    assert_eq!(path.as_deref(), Some("Engineering"));
}

/// **The failure direction, in every shape that can produce it.**
///
/// Absence, five ways, and never an empty string: an empty string would draw
/// an empty path line under the row, which is a wrong answer wearing a right
/// one's clothes. A non-array `ancestors` is among them: the declared read
/// only walks an array or an object, so a string cannot take the query down.
#[tokio::test]
async fn every_unusable_shape_misses_rather_than_guessing() {
    let pool = scratch().await;
    for payload in [
        // No `ancestors` at all.
        serde_json::json!({ "title": "Retry failed SEPA payouts" }),
        // `ancestors` that is neither an array nor an object.
        serde_json::json!({ "ancestors": "Engineering" }),
        // An empty array: a page at the top of its space.
        serde_json::json!({ "ancestors": [] }),
        // Elements that are not objects, and objects with no usable title.
        serde_json::json!({ "ancestors": ["Engineering", 7, null] }),
        serde_json::json!({ "ancestors": [{ "id": "1" }, { "title": null }, { "title": 7 }] }),
    ] {
        assert_eq!(
            path_of(&pool, payload.clone()).await,
            None,
            "{payload} must contribute no path at all"
        );
    }
}

/// One shape reads differently from #284's: an `ancestors` that is a single
/// object. The ancestor read missed on it; a declared parent's `at` may lead
/// to one object or to an array of them (the deputy's ruling of 2026-10-11 on
/// #594), so it is one parent. Confluence never sends this shape -- its
/// `ancestors` is always an array -- so no page's line changes, and this pins
/// the one rule every declared parent follows rather than a Confluence quirk.
#[tokio::test]
async fn an_ancestors_object_is_one_parent_as_every_declared_parent_is() {
    let pool = scratch().await;
    let path = path_of(
        &pool,
        serde_json::json!({ "ancestors": { "title": "Engineering" } }),
    )
    .await;
    assert_eq!(path.as_deref(), Some("Engineering"));
}

/// A title that is present but blank is not a segment.
///
/// The same triple refusal `declared_string!` makes -- absent, wrong type,
/// blank -- because a path with an empty segment reads as `Engineering ›  ›
/// SEPA`, which is worse than the segment simply not being there. Whitespace
/// counts as blank: a title of three spaces is a title nobody typed.
#[tokio::test]
async fn a_blank_title_is_not_a_segment_and_the_rest_still_form_a_path() {
    let pool = scratch().await;
    let path = path_of(
        &pool,
        serde_json::json!({
            "ancestors": [
                { "title": "Engineering" },
                { "title": "   " },
                { "title": "" },
                { "title": "Payments" }
            ]
        }),
    )
    .await;
    assert_eq!(
        path.as_deref(),
        Some("Engineering › Payments"),
        "half a path is still where the page lives; dropping the whole thing \
         because one ancestor lost its title would hide the space as well"
    );

    // And a title with padding is trimmed rather than joined with its spaces.
    let path = path_of(
        &pool,
        serde_json::json!({ "ancestors": [{ "title": "  Engineering  " }] }),
    )
    .await;
    assert_eq!(path.as_deref(), Some("Engineering"));

    // Every segment blank is no path at all, not a run of separators.
    let path = path_of(
        &pool,
        serde_json::json!({ "ancestors": [{ "title": " " }, { "title": "" }] }),
    )
    .await;
    assert_eq!(path, None);
}

/// The separator is the constant, and the constant is what two surfaces draw.
///
/// The launcher row and the detail panel both render whatever this joined, so
/// a second spelling anywhere would be a path that reads differently depending
/// on where you look at it. The SQL cannot expand a `const`, so the literal is
/// written into the macro -- and this is what holds the two together.
#[tokio::test]
async fn the_path_line_joins_on_the_one_separator() {
    let pool = scratch().await;
    let path = path_of(
        &pool,
        serde_json::json!({ "ancestors": [{ "title": "A" }, { "title": "B" }] }),
    )
    .await
    .expect("two segments join");
    assert_eq!(
        path,
        format!("A{}B", knobas_core::payload::ANCESTOR_SEPARATOR),
        "the statement joins on ANCESTOR_SEPARATOR, which is what the frontend \
         reads back"
    );
    assert!(
        READ.contains(knobas_core::payload::ANCESTOR_SEPARATOR),
        "the separator literal is in the statement itself: {READ}"
    );
}
