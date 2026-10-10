//! Where a record sits inside its source, joined by SQL, against a real
//! PostgreSQL (#284).
//!
//! `ancestor_path_read!` is a **payload read outside an adapter** and therefore
//! bound by ADR-0007's three interim requirements. Two of them are properties
//! of the source: it is one named statement, and it misses rather than
//! guessing. The third -- *its failure direction is stated and pinned by a
//! test* -- is this file, and it is the requirement that turns "the author
//! thought about it" into an obligation.
//!
//! **The direction is absence.** Every unusable shape below yields SQL `null`,
//! not an empty string and not a partial guess, so a row shows no path rather
//! than a wrong one. That matters twice over: the launcher draws a row per
//! result with no idea what kind it holds, and `string_agg` over no rows is
//! `null` by construction, which is what makes the property true without a
//! guard anybody has to remember to keep.
//!
//! Run against the database rather than against a Rust reimplementation on
//! purpose. The rule has exactly one implementation -- this SQL -- and a Rust
//! twin written to check it would be a second rule to keep in step, which is
//! the drift #277 spent `payload_paths.rs` pinning against.

use sqlx::{PgPool, Row};

/// The statement under test, over a payload bound as a parameter.
///
/// The macro takes its payload expression as a literal, so this is the same
/// `&'static str` the launcher and the room statements expand -- not a
/// paraphrase of it.
const READ: &str = concat!(
    "select ",
    knobas_core::ancestor_path_read!("i.payload"),
    " as path from (select $1::jsonb as payload) i"
);

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("ancestor_path")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database")
}

/// What the read answers for one payload.
async fn path_of(pool: &PgPool, payload: serde_json::Value) -> Option<String> {
    sqlx::query(READ)
        .bind(payload)
        .fetch_one(pool)
        .await
        .expect("the ancestor path read is valid SQL")
        .get::<Option<String>, _>("path")
}

/// The shape a Confluence page actually arrives in: `ancestors`,
/// **outermost first**, each an object with a `title`.
///
/// Order is the whole meaning of a path, so it is asserted rather than
/// assumed: `jsonb_array_elements ... with ordinality` is what preserves it,
/// and a `string_agg` without the `order by` would join in whatever order the
/// executor happened to produce.
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
/// Absence, six ways, and never an empty string: an empty string would draw an
/// empty path line under every ticket in the launcher, which is a wrong answer
/// wearing a right one's clothes.
///
/// The second case is the one that would take the whole query down rather than
/// miss: `jsonb_array_elements` **raises** on a non-array, so the
/// `jsonb_typeof(...) = 'array'` guard is what stands between one oddly-shaped
/// payload and a launcher that returns an error for every search.
#[tokio::test]
async fn every_unusable_shape_misses_rather_than_guessing() {
    let pool = scratch().await;
    for payload in [
        // A Jira ticket: no `ancestors` at all, which is most of the mirror.
        serde_json::json!({ "fields": { "summary": "Retry failed SEPA payouts" } }),
        // `ancestors` that is not an array -- the shape that raises.
        serde_json::json!({ "ancestors": "Engineering" }),
        serde_json::json!({ "ancestors": { "title": "Engineering" } }),
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
async fn the_ancestor_path_joins_on_the_one_separator() {
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
