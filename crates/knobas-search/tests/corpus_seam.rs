//! The M4 seam, run rather than asserted about.
//!
//! Spec §4 marks *"asset search matches ancestor path names (searching
//! 'pve-02' finds the containers under it)"* as **Decided**, with the round-3
//! gap called out: the corpus indexed only the asset's own fields, and the
//! implementation must index the path. M1 has no assets, so what is
//! deliverable here is that adding them in M4 is a [`Corpus`] entry and a
//! migration -- not a rewrite of the search stack.
//!
//! An assertion to that effect is worth nothing. A **second corpus that
//! actually runs through the same builder** is worth the hour, so
//! `corpus::NOTE` (`knobas.note`, a different relation with a different id
//! space, constant `kind`/`source_id` and a composed `headline_text`) goes
//! through `sql::search_sql`, `sql::query_as_with`, `group::group` and
//! `snippet::segments` untouched.
//!
//! Every test shares one database, so each seeds a token unique to itself.
//!
//! [`Corpus`]: knobas_search::corpus::Corpus

use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{query_as_with, search_sql};
use knobas_search::{KindCatalog, RawHit, corpus, group};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn token(tag: &str) -> String {
    format!("zc{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn seed_item(pool: &sqlx::PgPool, id: &str, kind: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, payload)
         values ($1,'jira',$2,$3,$4, now(), '{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_note(pool: &sqlx::PgPool, id: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.note (id, title, body_md) values ($1,$2,$3)")
        .bind(id)
        .bind(title)
        .bind(body)
        .execute(pool)
        .await
        .unwrap();
}

async fn run(pool: &sqlx::PgPool, text: &str, limit: u32) -> Vec<knobas_search::ResultGroup> {
    let built = search_sql(
        &[&corpus::LIVE_ITEM, &corpus::NOTE],
        Some(text),
        false,
        &EffectiveFilters::default(),
        10,
        limit,
    );
    let rows: Vec<RawHit> = query_as_with(built).fetch_all(pool).await.unwrap();
    group::group(rows, &KindCatalog::default())
}

#[tokio::test]
async fn a_second_corpus_joins_the_same_pipeline_untouched() {
    let pool = pool().await;
    let t = token("join");
    seed_item(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "SEPA retry",
        "nothing to do with the estate",
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-1"),
        &format!("{t} rebuild"),
        "Rebuild the hypervisor from the golden image.",
    )
    .await;

    // Grouping, ordering, totals and snippets are the same code as for
    // live_item: the second corpus needed no branch anywhere above sql.rs.
    let groups = run(&pool, &t, 20).await;
    assert_eq!(
        groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
        ["note"]
    );
    assert_eq!(groups[0].total, 1);
    assert!(groups[0].hits[0].snippet.iter().any(|s| s.hit));
    // The constant columns arrive as columns: a corpus whose `kind` and
    // `source_id` are literals is indistinguishable downstream from one whose
    // are real columns, which is the whole claim.
    assert_eq!(groups[0].hits[0].row.entity_id, format!("note:{t}-1"));
    assert_eq!(groups[0].hits[0].row.source_id, "note");
    assert_eq!(groups[0].hits[0].row.kind, "note");
    assert_eq!(groups[0].plural, "Notes");
    // `headline_text` composes two columns that are not the title, and the
    // excerpt is quoted from the composition.
    let joined: String = groups[0].hits[0]
        .snippet
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert!(joined.contains("hypervisor"), "{joined:?}");
}

/// Two corpora, one query: the union must interleave by rank rather than put
/// one relation's rows first -- otherwise M4's assets would always sort above
/// or below the work items regardless of how well they matched.
///
/// The page is the observable: the final result is ordered by *kind* (the
/// launcher's fixed groups), so which rows survive `limit` is the only place a
/// corpus-first ordering shows. Two strong matches, one per corpus, and two
/// weak ones; a page of two must be the two strong ones.
#[tokio::test]
async fn ranking_is_comparable_across_corpora() {
    let pool = pool().await;
    let t = token("rank");
    let filler = "ledger reconciliation window batch payout backlog";

    seed_item(
        &pool,
        &format!("jira:{t}-strong"),
        "ticket",
        &format!("{t} {t} {t}"),
        filler,
    )
    .await;
    seed_item(
        &pool,
        &format!("jira:{t}-weak"),
        "ticket",
        "Reconcile the ledger",
        &format!("{filler} {filler} {t}"),
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-strong"),
        &format!("{t} {t} {t}"),
        filler,
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-weak"),
        "Runbook",
        &format!("{filler} {filler} {t}"),
    )
    .await;

    // All four match, so the page really is a choice between them.
    let all = run(&pool, &t, 20).await;
    assert_eq!(all.iter().map(|g| g.total).sum::<u32>(), 4);

    let page = run(&pool, &t, 2).await;
    let mut ids: Vec<&str> = page
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [format!("jira:{t}-strong"), format!("note:{t}-strong")],
        "a page of two must be the two best matches, one from each corpus"
    );
    // And both corpora are still *counted* in full: the limit cut the page,
    // not the totals.
    assert_eq!(page.iter().map(|g| g.total).sum::<u32>(), 4);
}

/// The bind list is per *query*, not per branch.
///
/// `Builder::param` exists for exactly this: each branch repeats the text and
/// filter placeholders and must reuse the same `$n`. A builder that pushed a
/// duplicate per branch would still produce runnable SQL for one corpus and
/// would silently read the wrong argument for two.
#[tokio::test]
async fn a_second_corpus_adds_no_binds_and_still_executes() {
    let pool = pool().await;
    let filters = EffectiveFilters {
        sources: vec!["jira".to_owned()],
        kinds: vec!["ticket".to_owned(), "note".to_owned()],
        updated_within_days: Some(30),
        mine: false,
        authors: Vec::new(),
    };
    let one = search_sql(&[&corpus::LIVE_ITEM], Some("sepa"), true, &filters, 10, 20);
    let two = search_sql(
        &[&corpus::LIVE_ITEM, &corpus::NOTE],
        Some("sepa"),
        true,
        &filters,
        10,
        20,
    );
    assert_eq!(one.bind_count(), two.bind_count());
    assert!(two.sql().contains("union all"));
    // Executing it is the half a shape assertion cannot do: a placeholder that
    // drifted from its argument is a type error the *server* raises. The row
    // set is beside the point here -- that the statement parsed, bound and
    // decoded into `RawHit` at all is the assertion.
    let _rows: Vec<RawHit> = query_as_with(two).fetch_all(&pool).await.unwrap();
}
