//! `recorded_parent_key!` held to the Jira descriptor's `ticket` parent
//! candidates (#649), against a real PostgreSQL.
//!
//! Contexts read a ticket's parent through `knobas_core::recorded_parent_key!`,
//! a `coalesce` written by hand: `fields.parent.key`, then the classic
//! `epic.key` (#641). `under:` reads the same relation through the descriptor's
//! `ParentPath` candidates, in the order the deputy's ruling of 2026-10-12 on
//! #598 set. Until this file only the macro's doc comment kept the two in
//! step ("the order is what the two are held to"). This is the pin, in the
//! shape of `knobas-core`'s `tests/it/payload_paths.rs`: the macro's arms are
//! compared with the declaration, and the SQL is run.
//!
//! It lives in this crate because it reads this crate's descriptor;
//! `knobas-core` cannot depend on an adapter without a cycle.
//!
//! The two rules are not the same rule, by design, and the last test states
//! where they part: `declared_parents!` takes the first candidate that is an
//! object, `coalesce` the first that yields a key, so a keyless
//! `fields.parent: {}` stops `under:` and falls through to `epic` here.

use knobas_core::payload::{Declarations, ParentPath, resolve_parents};
use knobas_source_jira::{KIND_TICKET, descriptor_template};
use sqlx::PgPool;

/// The Jira descriptor's `ticket` parent candidates, in declared order.
fn ticket_candidates() -> Vec<ParentPath> {
    descriptor_template()
        .payload_paths
        .into_iter()
        .find(|paths| paths.kind == KIND_TICKET)
        .expect("a ticket declares its paths")
        .parent
}

/// One candidate as the path to the key `coalesce` would have to read: its
/// `at`, then its first name -- the name a context matches on.
fn key_path(candidate: &ParentPath) -> Vec<String> {
    let first_name = candidate
        .names
        .first()
        .unwrap_or_else(|| panic!("{candidate:?} declares no name"));
    candidate
        .at
        .segments()
        .iter()
        .chain(first_name.segments())
        .cloned()
        .collect()
}

/// The macro's `coalesce` arms, each as the path it reads.
///
/// A strict reading of the one form the macro is written in --
/// `coalesce(i.payload->'a'->…->>'z', …)` -- that panics on anything else, so
/// an arm rewritten in another spelling fails here loudly rather than parsing
/// to a wrong path.
fn macro_arms() -> Vec<Vec<String>> {
    let sql = knobas_core::recorded_parent_key!();
    let inner = sql
        .strip_prefix("coalesce(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("not a coalesce: {sql}"));
    inner
        .split(", ")
        .map(|arm| {
            let mut rest = arm
                .strip_prefix("i.payload")
                .unwrap_or_else(|| panic!("an arm not over i.payload: {arm}"));
            let mut path = Vec::new();
            loop {
                let (last, tail) = if let Some(tail) = rest.strip_prefix("->>'") {
                    (true, tail)
                } else if let Some(tail) = rest.strip_prefix("->'") {
                    (false, tail)
                } else {
                    panic!("an arm not of the form ->'seg'…->>'key': {arm}");
                };
                let (segment, after) = tail
                    .split_once('\'')
                    .unwrap_or_else(|| panic!("an unterminated segment: {arm}"));
                path.push(segment.to_owned());
                rest = after;
                if last {
                    assert!(rest.is_empty(), "text after ->> in {arm}");
                    return path;
                }
            }
        })
        .collect()
}

/// `payload` with `value` set at `path`, creating objects on the way.
fn set_at(payload: &mut serde_json::Value, path: &[String], value: &str) {
    let (last, parents) = path.split_last().expect("a non-empty path");
    let mut here = payload;
    for segment in parents {
        here = here
            .as_object_mut()
            .expect("an object on the way")
            .entry(segment.clone())
            .or_insert_with(|| serde_json::json!({}));
    }
    here.as_object_mut()
        .expect("an object at the end")
        .insert(last.clone(), serde_json::Value::String(value.to_owned()));
}

/// The source instance the pinned records live in: an instance id, as
/// `Declarations` keys them, and not the adapter kind.
const SOURCE: &str = "jira-pin";

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("parent_key_pin")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One live mirrored Jira ticket carrying `payload`, as `sync.live_item` --
/// the view every reader of these macros runs over -- serves it. Returns its
/// entity id.
async fn ticket(pool: &PgPool, key: &str, payload: &serde_json::Value) -> String {
    let id = format!("{SOURCE}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(KIND_TICKET)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(SOURCE)
    .bind(KIND_TICKET)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// What `recorded_parent_key!` reads off one item, through the database.
async fn recorded_in_sql(pool: &PgPool, entity_id: &str) -> Option<String> {
    const RECORDED: &str = concat!(
        "select ",
        knobas_core::recorded_parent_key!(),
        " from sync.live_item i where i.entity_id = $1"
    );
    sqlx::query_scalar(RECORDED)
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("recorded_parent_key! must run on {entity_id}: {e}"))
}

/// One column of a parent table expression over one item, under the Jira
/// descriptor's declarations.
async fn under_in_sql(pool: &PgPool, statement: &'static str, entity_id: &str) -> Vec<String> {
    let declarations = Declarations::empty().with(SOURCE, descriptor_template().payload_paths);
    sqlx::query_scalar(statement)
        .bind(declarations.as_param())
        .bind(entity_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("{statement} must run on {entity_id}: {e}"))
}

/// The arms are the descriptor's candidates, each one's `at` plus its first
/// name, in the same order. A candidate gained, lost or moved on either side
/// fails here.
#[test]
fn the_macros_arms_are_the_ticket_parent_candidates_in_declared_order() {
    let declared: Vec<Vec<String>> = ticket_candidates().iter().map(key_path).collect();
    assert_eq!(
        macro_arms(),
        declared,
        "recorded_parent_key! must read the Jira ticket's parent candidates, in order"
    );
}

/// The same order, run: for each candidate, a record carrying a key at it and
/// at every later one resolves to that candidate's key, in the macro's SQL and
/// in `under:`'s Rust resolver alike. So where several carry a key, the
/// earliest declared wins on both sides -- a sub-task's direct parent over its
/// epic.
#[tokio::test]
async fn the_first_candidate_carrying_a_key_wins_in_sql_and_for_under() {
    let pool = scratch().await;
    let candidates = ticket_candidates();
    assert!(
        candidates.len() >= 2,
        "the pin is about an order: {candidates:?}"
    );
    for first in 0..candidates.len() {
        let mut payload = serde_json::json!({ "key": "PAY-999" });
        for (later, candidate) in candidates.iter().enumerate().skip(first) {
            set_at(&mut payload, &key_path(candidate), &format!("PAY-{later}"));
        }
        let expected = format!("PAY-{first}");
        let id = ticket(&pool, &format!("PIN-{first}"), &payload).await;

        assert_eq!(
            recorded_in_sql(&pool, &id).await.as_deref(),
            Some(expected.as_str()),
            "recorded_parent_key! on {payload}"
        );
        let under = resolve_parents(&payload, &candidates);
        assert_eq!(under.len(), 1, "{under:?}");
        assert_eq!(under[0].names, vec![expected], "under: on {payload}");
    }
}

/// The documented divergence, stated by a test. A keyless `fields.parent: {}`
/// is an object, so `under:` stops at it -- one parent that answers to no
/// name, in Rust and in SQL -- and never reads `epic`. `coalesce` asks for a
/// key, finds none there, and takes `epic`'s. Jira sends no such record; if it
/// ever did, a context would seed the child under its epic while `under:`
/// would not find it there.
#[tokio::test]
async fn a_keyless_fields_parent_stops_under_and_falls_through_to_epic_for_contexts() {
    let pool = scratch().await;
    let payload = serde_json::json!({
        "key": "PAY-250",
        "fields": { "parent": {} },
        "epic": { "key": "PAY-200" }
    });
    let id = ticket(&pool, "PAY-250", &payload).await;

    // Contexts: falls through to the epic.
    assert_eq!(
        recorded_in_sql(&pool, &id).await.as_deref(),
        Some("PAY-200")
    );

    // `under:`, in Rust: the first candidate is an object, so it is the parent,
    // and it answers to nothing.
    let under = resolve_parents(&payload, &ticket_candidates());
    assert_eq!(under.len(), 1, "{under:?}");
    assert_eq!(under[0].kind, KIND_TICKET);
    assert!(under[0].names.is_empty(), "{under:?}");

    // `under:`, in SQL: the same one parent, and no name for it to match.
    const PARENTS: &str = concat!(
        "select p.kind from sync.live_item i cross join lateral ",
        knobas_core::declared_parents!("$1"),
        " p where i.entity_id = $2"
    );
    const NAMES: &str = concat!(
        "select n.name from sync.live_item i cross join lateral ",
        knobas_core::declared_parent_names!("$1"),
        " n where i.entity_id = $2"
    );
    assert_eq!(
        under_in_sql(&pool, PARENTS, &id).await,
        vec![KIND_TICKET.to_owned()]
    );
    let names = under_in_sql(&pool, NAMES, &id).await;
    assert!(names.is_empty(), "under: matched {names:?}");
}
