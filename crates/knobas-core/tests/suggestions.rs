//! The suggestion engine, against a real PostgreSQL.
//!
//! Every test in this binary shares one database (see `test_util`), so each one
//! seeds entities under a namespace of its own and asserts only over its own
//! rows: absolute counts would depend on which test committed first.
//!
//! What is asserted here is **which suggestions are proposed and what happens
//! when the user acts** -- never how a detector scanned text. Each rule gets a
//! positive and a negative case in one test, because a rule that fires for
//! everything is as broken as one that never fires and only the negative case
//! catches the first.

use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::suggest::{self, RuleClass};
use knobas_core::{CoreError, link};
use sqlx::PgPool;
use uuid::Uuid;

/// A migrated pool and a namespace nothing else in this binary writes.
///
/// The namespace doubles as the source id, so `proposals`' room scoping can be
/// asserted per test without any test seeing another's rows.
async fn scratch() -> (PgPool, String) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let source = format!("s{}", Uuid::new_v4().simple());
    (pool, source)
}

/// Put one live mirror item in the corpus, and return its address.
///
/// Both halves, because every rule reads `sync.live_item`, which is the join of
/// the two: an entity row alone is linkable but invisible to detection, which
/// is exactly the state a note or a context is in.
async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, title: &str, body: &str) -> String {
    mirror(pool, source, kind, key, title, body, serde_json::json!({})).await
}

/// As [`item`], with a payload -- what the source-recorded rule reads.
async fn mirror(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Run one named rule. Panics on a name no rule carries, which is a typo in a
/// test rather than a failure worth a message.
async fn run_rule(pool: &PgPool, name: &str) -> u64 {
    let rule = suggest::rule(name).expect("a rule by that name");
    suggest::detect_rule(pool, rule).await.unwrap()
}

/// Every proposal in this test's own room, newest first.
async fn tray(pool: &PgPool, source: &str) -> Vec<suggest::SuggestionEntry> {
    suggest::proposals(pool, &[source.to_owned()], 100)
        .await
        .unwrap()
}

/// The `(from, to)` pairs the tray holds, as a set.
fn pairs(entries: &[suggest::SuggestionEntry]) -> std::collections::BTreeSet<(String, String)> {
    entries
        .iter()
        .map(|e| (e.link.from_id.clone(), e.link.to_id.clone()))
        .collect()
}

/// The one proposal connecting `a` and `b`, in either direction.
fn between<'e>(
    entries: &'e [suggest::SuggestionEntry],
    a: &str,
    b: &str,
) -> Option<&'e suggest::SuggestionEntry> {
    entries.iter().find(|e| {
        (e.link.from_id == a && e.link.to_id == b) || (e.link.from_id == b && e.link.to_id == a)
    })
}

// -- one test per rule, each with its negative control ----------------------

#[tokio::test]
async fn a_branch_name_proposes_the_ticket_it_names_and_nothing_else() {
    let (pool, s) = scratch().await;
    let ticket = item(&pool, &s, "ticket", "PAY-231", "Payout retry storm", "").await;
    let named = item(&pool, &s, "branch", "b1", "feature/PAY-231-retry", "").await;
    // The negative control, and it is a *near* miss: a branch that carries no
    // key at all would be caught by a rule that fired on the wrong kind, but
    // only a longer key catches one whose match has no word boundary.
    let unnamed = item(&pool, &s, "branch", "b2", "feature/retry-storm", "").await;
    let longer = item(&pool, &s, "branch", "b3", "feature/PAY-2311-other", "").await;

    run_rule(&pool, "branch_name_key").await;

    let entries = tray(&pool, &s).await;
    assert_eq!(
        pairs(&entries),
        [(named.clone(), ticket.clone())].into_iter().collect(),
        "only the branch that names PAY-231 proposes it"
    );
    assert!(between(&entries, &unnamed, &ticket).is_none());
    assert!(between(&entries, &longer, &ticket).is_none());

    let proposal = &entries[0];
    assert_eq!(
        proposal.link.reason.as_deref(),
        Some("the branch name contains PAY-231"),
        "the reason names the key, not the rule"
    );
    assert_eq!(proposal.link.rule.as_deref(), Some("branch_name_key"));
    assert_eq!(proposal.link.rule_class, Some(RuleClass::ExactKey));
    assert_eq!(proposal.link.origin, Origin::Suggested);
    assert_eq!(proposal.link.confirmed_at, None);
    assert_eq!(proposal.from.entity_id, named);
    assert_eq!(proposal.to.title, "Payout retry storm");
}
