//! One rule, two implementations, against a real PostgreSQL (#277).
//!
//! A declared payload path is resolved twice in this codebase: in Rust, by
//! `knobas_core::payload`'s resolvers, which is what the contract battery
//! certifies an adapter's declarations with; and in SQL, by
//! `declared_string!`, `declared_flag!` and `declared_list!`, which is what
//! every reader actually runs -- the project census is a pass over the whole
//! live corpus and the mini board narrows inside its own statement, so pulling
//! payloads into Rust to resolve them is not on offer.
//!
//! Two implementations of one rule drift. This is the pin: every case below
//! goes through **both**, and the assertion is that they agree *and* that they
//! agree on the stated answer. A case whose expected value were read out of
//! one of the two would be a tautology; each is written out.
//!
//! The cases are the failure directions ADR-0007 requires pinned, at the level
//! the resolution itself decides them: a candidate that lands on an object, a
//! blank string, a key an object does not have, a source that declares
//! nothing, a kind that declares nothing, a field that declares nothing.

use knobas_core::payload::{
    Declarations, KindPaths, ListPath, PayloadPath, resolve_flag, resolve_list, resolve_string,
};
use sqlx::{PgPool, Row};

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("payload_paths")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One live mirror item, carrying the payload it is given.
async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, payload: &serde_json::Value) {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// The statement a reader takes: one row per mirrored item, its declared
/// status, priority and merged flag resolved the way the mini board and the
/// merge pass resolve them.
const RESOLVED: &str = concat!(
    "select i.entity_id, ",
    knobas_core::declared_string!("$1", "status_name"),
    " as status, ",
    knobas_core::declared_string!("$1", "priority"),
    " as priority, ",
    knobas_core::declared_flag!("$1", "merged"),
    " as merged, (select array_agg(value order by value) from ",
    knobas_core::declared_list!("$1", "reviewers"),
    " lst) as reviewers
       from sync.live_item i
      where i.entity_id = $2"
);

/// What one item's declared fields resolve to, through the database.
struct Resolved {
    status: Option<String>,
    priority: Option<String>,
    merged: Option<bool>,
    reviewers: Vec<String>,
}

async fn in_sql(pool: &PgPool, declarations: &Declarations, entity_id: &str) -> Resolved {
    let row = sqlx::query(RESOLVED)
        .bind(sqlx::types::Json(declarations.as_json()))
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{entity_id} must resolve: {e}"));
    Resolved {
        status: row.get("status"),
        priority: row.get("priority"),
        merged: row.get("merged"),
        reviewers: row
            .get::<Option<Vec<String>>, _>("reviewers")
            .unwrap_or_default(),
    }
}

/// The same four questions asked of the payload in Rust.
fn in_rust(declarations: &Declarations, source: &str, kind: &str, payload: &serde_json::Value) -> Resolved {
    let empty = KindPaths::default();
    let paths = declarations.get(source, kind).unwrap_or(&empty);
    let mut reviewers = resolve_list(payload, &paths.reviewers);
    reviewers.sort();
    Resolved {
        status: resolve_string(payload, &paths.status_name),
        priority: resolve_string(payload, &paths.priority),
        merged: resolve_flag(payload, &paths.merged),
        reviewers,
    }
}

/// Run one case through both implementations and assert they agree with each
/// other **and** with the stated answer.
async fn both(
    pool: &PgPool,
    declarations: &Declarations,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
    expected: (Option<&str>, Option<&str>, Option<bool>, &[&str]),
) {
    item(pool, source, kind, key, &payload).await;
    let sql = in_sql(pool, declarations, &format!("{source}:{key}")).await;
    let rust = in_rust(declarations, source, kind, &payload);
    let (status, priority, merged, reviewers) = expected;
    let reviewers: Vec<String> = reviewers.iter().map(|r| (*r).to_owned()).collect();

    assert_eq!(sql.status.as_deref(), status, "SQL status for {key}");
    assert_eq!(rust.status.as_deref(), status, "Rust status for {key}");
    assert_eq!(sql.priority.as_deref(), priority, "SQL priority for {key}");
    assert_eq!(rust.priority.as_deref(), priority, "Rust priority for {key}");
    assert_eq!(sql.merged, merged, "SQL merged for {key}");
    assert_eq!(rust.merged, merged, "Rust merged for {key}");
    assert_eq!(sql.reviewers, reviewers, "SQL reviewers for {key}");
    assert_eq!(rust.reviewers, reviewers, "Rust reviewers for {key}");
}

/// Jira Data Center's spellings, and a source that spells a status flat.
fn declarations() -> Declarations {
    Declarations::empty()
        .with(
            "jira",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                status_name: vec![
                    PayloadPath::of(["fields", "status", "name"]),
                    PayloadPath::of(["status"]),
                ],
                priority: vec![PayloadPath::of(["fields", "priority", "name"])],
                ..KindPaths::default()
            }],
        )
        .with(
            "gitea",
            vec![KindPaths {
                kind: "pr".to_owned(),
                merged: vec![PayloadPath::of(["merged"])],
                reviewers: vec![ListPath {
                    at: PayloadPath::of(["requested_reviewers"]),
                    entry: PayloadPath::of(["login"]),
                }],
                ..KindPaths::default()
            }],
        )
}

#[tokio::test]
async fn a_declared_path_resolves_the_same_in_sql_and_in_rust() {
    let pool = scratch().await;
    let declarations = declarations();

    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-1",
        serde_json::json!({ "fields": {
            "status": { "name": "In Progress" },
            "priority": { "name": "High" }
        }}),
        (Some("In Progress"), Some("High"), None, &[]),
    )
    .await;

    // The second candidate, which is what a `coalesce` arm used to be: the
    // first lands on nothing, the flat spelling answers.
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-2",
        serde_json::json!({ "status": "To Do" }),
        (Some("To Do"), None, None, &[]),
    )
    .await;

    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#7",
        serde_json::json!({
            "merged": true,
            "requested_reviewers": [{ "login": "tom.reyes" }, { "login": "mara.lindqvist" }]
        }),
        (
            None,
            None,
            Some(true),
            &["mara.lindqvist", "tom.reyes"],
        ),
    )
    .await;
}

/// The miss directions, both implementations, one case each. A path that lands
/// on an object, on a blank string or on nothing at all contributes **no**
/// value -- `->>` would have stringified the object into `{"id":3}`, which is
/// a guess dressed as an observation.
#[tokio::test]
async fn a_path_that_lands_on_no_usable_value_misses_in_both() {
    let pool = scratch().await;
    let declarations = declarations();

    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-10",
        serde_json::json!({ "fields": { "status": { "name": { "id": 3 } } } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-11",
        serde_json::json!({ "fields": { "status": { "name": "   " } } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-12",
        serde_json::json!({ "fields": { "status": serde_json::Value::Null } }),
        (None, None, None, &[]),
    )
    .await;
    // A flag is a boolean or it is nothing, and a list that is not an array
    // yields nothing rather than raising -- `jsonb_array_elements` raises on a
    // scalar, which would fail the read for every source at once.
    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#8",
        serde_json::json!({ "merged": "true", "requested_reviewers": { "login": "mara" } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#9",
        serde_json::json!({ "requested_reviewers": serde_json::Value::Null }),
        (None, None, None, &[]),
    )
    .await;
}

/// Three ways a declaration can be absent, and all three miss rather than
/// falling back on a shape knobas happens to recognise. This is the clause
/// that makes "a kind that declares no path is a miss, never a guess" a fact
/// about the code rather than a promise on a doc comment: the payload below is
/// Jira-shaped in every case, and no reader may read it.
#[tokio::test]
async fn an_undeclared_source_kind_or_field_misses_in_both() {
    let pool = scratch().await;
    let declarations = declarations();
    let jira_shaped = serde_json::json!({ "fields": {
        "status": { "name": "In Progress" }, "priority": { "name": "High" }
    }});

    // A source nothing declares for.
    both(
        &pool,
        &declarations,
        "jira-eu",
        "ticket",
        "PAY-20",
        jira_shaped.clone(),
        (None, None, None, &[]),
    )
    .await;
    // A kind this source does not declare: Jira declares `ticket` only.
    both(
        &pool,
        &declarations,
        "jira",
        "page",
        "PAY-21",
        jira_shaped.clone(),
        (None, None, None, &[]),
    )
    .await;
    // A field this kind does not declare: Jira's `ticket` declares no merged
    // flag and no reviewers, and this pull-request-shaped payload carries
    // both.
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-22",
        serde_json::json!({
            "merged": true,
            "requested_reviewers": [{ "login": "mara" }],
            "fields": { "status": { "name": "In Progress" } }
        }),
        (Some("In Progress"), None, None, &[]),
    )
    .await;
    // Nobody declares anything at all.
    both(
        &pool,
        &Declarations::empty(),
        "jira",
        "ticket",
        "PAY-23",
        jira_shaped,
        (None, None, None, &[]),
    )
    .await;
}
