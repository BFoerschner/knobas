//! The contexts IPC surface (#47), against a real PostgreSQL.
//!
//! The membership *rule* is proven in `knobas-core`'s own battery
//! (`crates/knobas-core/tests/contexts.rs`); what is asserted here is the
//! boundary: which failures map to which codes, that the room's page and the
//! tray really scope by membership when a stored context asks, and that the
//! activity log hears a mutation exactly once.
//!
//! Every test shares one database (`test_util`), so fixtures carry ids unique
//! per run and assertions are set membership, never absolute totals.

use knobas_app::commands::entity::{
    EntityFilter, EntityOrder, create_context_inner, create_link_inner, list_entities_inner,
    promote_context_inner, room_suggestions_inner,
};
use knobas_app::{IpcError, IpcErrorCode};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A token no other test in this binary writes.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// One live mirror item under this run's own source, so the entity has both
/// halves: an entity row (linkable) and a mirror row (listable).
async fn item(pool: &PgPool, source: &str, kind: &str, key: &str) -> String {
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
         values ($1,$2,$3,$4,'','{}'::jsonb)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn scoped_to(ctx: &str) -> EntityFilter {
    EntityFilter {
        sources: Vec::new(),
        kinds: Vec::new(),
        updated_within_days: None,
        context: Some(ctx.to_owned()),
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
    }
}

fn code(error: &IpcError) -> IpcErrorCode {
    error.code
}

#[tokio::test]
async fn a_blank_label_is_refused_as_invalid() {
    let pool = pool().await;
    let refused = create_context_inner(&pool, "   ").await.unwrap_err();
    assert_eq!(code(&refused), IpcErrorCode::Invalid);
}

#[tokio::test]
async fn promoting_the_unsynced_is_not_found_and_a_bad_id_is_invalid() {
    let pool = pool().await;
    let missing = promote_context_inner(&pool, &format!("jira:GONE-{}", unique()))
        .await
        .unwrap_err();
    assert_eq!(code(&missing), IpcErrorCode::NotFound);

    let malformed = promote_context_inner(&pool, "no-separator")
        .await
        .unwrap_err();
    assert_eq!(code(&malformed), IpcErrorCode::Invalid);
}

/// The room's page scopes by the one-hop rule when the filter names a stored
/// context: the member is on the page, the entity past the rule's edge is
/// not, and `total` counts the scoped set rather than the corpus.
#[tokio::test]
async fn a_stored_context_scopes_the_page_and_its_total() {
    let pool = pool().await;
    let source = format!("ctxsrc-{}", unique());
    let ticket = item(&pool, &source, "ticket", "PAY-1").await;
    let pr = item(&pool, &source, "pr", "payout#1").await;
    let build = item(&pool, &source, "build", "Main/41").await;
    let page = item(&pool, &source, "page", "ENG/design").await;
    create_link_inner(&pool, &ticket, &pr, None, None)
        .await
        .unwrap();
    create_link_inner(&pool, &pr, &build, None, None)
        .await
        .unwrap();
    create_link_inner(&pool, &build, &page, None, None)
        .await
        .unwrap();

    let ctx = promote_context_inner(&pool, &ticket).await.unwrap().context;

    let listed = list_entities_inner(&pool, &scoped_to(&ctx.id), 50, 0)
        .await
        .unwrap();
    let ids: Vec<&str> = listed
        .rows
        .iter()
        .map(|row| row.entity_id.as_str())
        .collect();
    assert!(ids.contains(&ticket.as_str()), "the anchor is a member");
    assert!(ids.contains(&pr.as_str()), "a direct link is a member");
    assert!(ids.contains(&build.as_str()), "one hop out is a member");
    assert!(!ids.contains(&page.as_str()), "two hops out is not");
    assert_eq!(listed.total, 3, "the total describes the scoped set");
}

/// An unknown context is an empty page, not an unscoped one: the difference
/// between "no filter" and "a filter nothing passes" is the whole point of
/// binding membership as its own parameter.
#[tokio::test]
async fn an_unknown_context_is_an_empty_page_not_the_corpus() {
    let pool = pool().await;
    // At least one row exists somewhere in the shared corpus.
    let source = format!("ctxsrc-{}", unique());
    item(&pool, &source, "ticket", "PAY-9").await;

    let listed = list_entities_inner(&pool, &scoped_to("ctx:gone"), 50, 0)
        .await
        .unwrap();
    assert_eq!(listed.total, 0);
    assert!(listed.rows.is_empty());
}

/// The tray's context scope: a proposal touching a member (or the context's
/// own entity) is in the room's tray, everything else is not -- and the count
/// agrees with the rows because it is the same predicate counted.
#[tokio::test]
async fn the_tray_scopes_by_the_contexts_membership() {
    let pool = pool().await;
    let source = format!("ctxsrc-{}", unique());
    let ticket = item(&pool, &source, "ticket", "PAY-1").await;
    let branch = item(&pool, &source, "branch", "feature/pay-1").await;
    let stranger = item(&pool, &source, "ticket", "EU-1").await;
    let stray = item(&pool, &source, "page", "EU/notes").await;

    let ctx = promote_context_inner(&pool, &ticket).await.unwrap().context;

    // One proposal touching the member, one touching nobody in the context.
    for (from, to) in [(&ticket, &branch), (&stranger, &stray)] {
        sqlx::query(
            "insert into knobas.link
                 (from_id, to_id, relation, origin, created_by,
                  confirmed_at, rule, rule_class, reason)
             values ($1, $2, 'related', 'suggested', 'knobas',
                     null, 'test', 'exact_key', 'a guess for the test')",
        )
        .bind(from)
        .bind(to)
        .execute(&pool)
        .await
        .unwrap();
    }

    let tray = room_suggestions_inner(&pool, &[], Some(&ctx.id), 50)
        .await
        .unwrap();
    let pairs: Vec<(&str, &str)> = tray
        .rows
        .iter()
        .map(|entry| (entry.link.from_id.as_str(), entry.link.to_id.as_str()))
        .collect();
    assert_eq!(pairs, vec![(ticket.as_str(), branch.as_str())]);
    assert_eq!(tray.total, 1, "the count is the same predicate counted");
}

/// Promote writes one `promoted` line -- and only the first time, because the
/// second call mutated nothing and a log line for a non-event would be the
/// log lying.
#[tokio::test]
async fn promote_announces_once_however_often_it_is_pressed() {
    let pool = pool().await;
    let source = format!("ctxsrc-{}", unique());
    let ticket = item(&pool, &source, "ticket", "PAY-1").await;

    let first = promote_context_inner(&pool, &ticket).await.unwrap();
    let second = promote_context_inner(&pool, &ticket).await.unwrap();
    assert_eq!(first.context.id, second.context.id);
    assert!(
        first.fresh && !second.fresh,
        "freshness is the store's answer"
    );

    let (lines,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.activity where verb = 'promoted' and entity_id = $1",
    )
    .bind(&first.context.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lines, 1);
}
