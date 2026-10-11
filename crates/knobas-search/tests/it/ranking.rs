//! A title match outranks a body match, in every kind, without becoming a
//! strict tier (#592, spec #591 story 25 and 26).
//!
//! Through the public [`Searcher`] against a real PostgreSQL, because what the
//! launcher draws is the order of rows inside a group, and that order is the
//! product of the corpus's weighting, `ts_rank_cd`, the per-kind window and
//! the page -- all four, not any one of them.
//!
//! Every test seeds a token unique to itself and searches for that, so the
//! module's shared database (`knobas_db::test_util`, keyed by this file) never
//! hands a test a row another one left behind.
//!
//! **Timestamps run against the expectation throughout.** The row expected
//! first is always the *older* one, so recency -- the ordering's second key --
//! can only produce the wrong answer, and a pass is the rank's doing.

use chrono::{DateTime, Duration, Utc};
use knobas_search::{KindCatalog, SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token(tag: &str) -> String {
    format!("zt{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::with_kinds(
        pool.clone(),
        KindCatalog::from_descriptors([knobas_source_mock::descriptor_template()]),
    )
}

/// One mirrored item, written the way a sync run writes it.
async fn seed(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    title: &str,
    body: &str,
    updated_at: DateTime<Utc>,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    let source_id = id.split(':').next().unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         values ($1,$2,$3,$4,$5,$6, now(), '{}'::jsonb)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
}

/// One asset at the root of the estate, the way the asset store writes it.
async fn seed_asset(pool: &sqlx::PgPool, id: &str, name: &str, properties: serde_json::Value) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, properties, path_text)
         values ($1,'vm',$2,$3,'')",
    )
    .bind(id)
    .bind(name)
    .bind(properties)
    .execute(pool)
    .await
    .unwrap();
}

/// The ids of one group's rows, in the order the launcher draws them.
fn group_ids(results: &SearchResponse, kind: &str) -> Vec<String> {
    results
        .groups
        .iter()
        .find(|group| group.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} group in {results:?}"))
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.clone())
        .collect()
}

fn days_ago(n: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(n)
}

/// Story 25, in two mirrored kinds and two kinds knobas owns.
///
/// In each kind the title match is the older row, and the body match mentions
/// the word exactly once -- the comparable item the criterion asks for. A
/// ticket and a commit are both mirror rows; the note (`knobas.note`) and the
/// asset (`knobas.asset`) are corpora of their own, so "every kind" is asserted
/// across the weighting of three relations and not just one.
#[tokio::test]
async fn a_title_match_ranks_above_a_body_match_in_every_kind() {
    let pool = pool().await;
    let t = token("title");

    seed(
        &pool,
        &format!("jira:{t}-title"),
        "ticket",
        &format!("Retry the {t} payouts"),
        "Unrelated prose about batch windows and ledgers.",
        days_ago(30),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-body"),
        "ticket",
        "Reconcile the ledger",
        &format!("Blocked on the {t} work, mentioned once."),
        days_ago(1),
    )
    .await;
    seed(
        &pool,
        &format!("gitea:{t}-title"),
        "commit",
        &format!("{t}: jitter in backoff"),
        "Cap at five attempts.",
        days_ago(30),
    )
    .await;
    seed(
        &pool,
        &format!("gitea:{t}-body"),
        "commit",
        "Tidy the changelog",
        &format!("Follow-up to the {t} change."),
        days_ago(1),
    )
    .await;

    // Created in order, so the body match is the newer note.
    let title_note = knobas_core::note::create(
        &pool,
        &format!("{t} investigation"),
        "the counter starts at zero, not one",
        &[],
        "user",
    )
    .await
    .unwrap();
    let body_note = knobas_core::note::create(
        &pool,
        "Standup notes",
        &format!("asked about {t} again"),
        &[],
        "user",
    )
    .await
    .unwrap();

    // An asset's "body" is its property values (migration `0019`, weight C):
    // one VM named for the token, one that only carries it as a hostname.
    // Inserted in order, so `updated_at` makes the property match the newer.
    seed_asset(
        &pool,
        &format!("asset:{t}-title"),
        &format!("{t}-db"),
        serde_json::json!({}),
    )
    .await;
    seed_asset(
        &pool,
        &format!("asset:{t}-body"),
        "vm-ledger",
        serde_json::json!({ "hostname": t }),
    )
    .await;

    let r = searcher(&pool).search(q(&t)).await.unwrap();

    assert_eq!(
        group_ids(&r, "asset"),
        [format!("asset:{t}-title"), format!("asset:{t}-body")],
        "{r:?}"
    );
    assert_eq!(
        group_ids(&r, "ticket"),
        [format!("jira:{t}-title"), format!("jira:{t}-body")],
        "{r:?}"
    );
    assert_eq!(
        group_ids(&r, "commit"),
        [format!("gitea:{t}-title"), format!("gitea:{t}-body")],
        "{r:?}"
    );
    assert_eq!(
        group_ids(&r, "note"),
        [title_note.id, body_note.id],
        "{r:?}"
    );
}

/// Story 26: a weighting, not a strict tier.
///
/// A page whose body is about the query -- it says the word four times --
/// against a page that names the word once in its title and never again. The
/// body page is the older one, so recency cannot rescue it either. This fails
/// the day the title becomes a tier of its own (a title hit first, rank
/// second), which is the mistake the criterion names.
#[tokio::test]
async fn a_strong_body_match_still_beats_a_weak_title_match() {
    let pool = pool().await;
    let t = token("strong");

    seed(
        &pool,
        &format!("confluence:{t}-body"),
        "page",
        "Runbook: payouts",
        &format!(
            "What {t} is. Why {t} retries. How to restart {t} safely. \
             When {t} pages someone."
        ),
        days_ago(30),
    )
    .await;
    seed(
        &pool,
        &format!("confluence:{t}-title"),
        "page",
        &format!("Meeting notes mentioning {t}"),
        "Attendees, action items, nothing else.",
        days_ago(1),
    )
    .await;

    let r = searcher(&pool).search(q(&t)).await.unwrap();

    assert_eq!(
        group_ids(&r, "page"),
        [
            format!("confluence:{t}-body"),
            format!("confluence:{t}-title")
        ],
        "{r:?}"
    );
}

/// The per-kind cap and the true total, under the weighting.
///
/// Twelve body matches, all newer than the one title match, in one kind. The
/// group shows its cap of rows, the title match is the first of them though it
/// is the oldest row in the kind, and the group still reports all thirteen --
/// the cut is a page, not a count.
#[tokio::test]
async fn the_cap_keeps_the_title_match_and_the_true_total() {
    let pool = pool().await;
    let t = token("cap");

    seed(
        &pool,
        &format!("jira:{t}-title"),
        "ticket",
        &format!("{t} rollout"),
        "Unrelated prose.",
        days_ago(60),
    )
    .await;
    for n in 0..12 {
        seed(
            &pool,
            &format!("jira:{t}-body-{n:02}"),
            "ticket",
            &format!("Ledger window {n}"),
            &format!("Mentions {t} once, in passing."),
            Utc::now() - Duration::minutes(n),
        )
        .await;
    }

    let r = searcher(&pool).search(q(&t)).await.unwrap();
    let ticket = r
        .groups
        .iter()
        .find(|group| group.kind == "ticket")
        .unwrap();

    assert_eq!(ticket.hits.len(), 10, "{r:?}");
    assert_eq!(ticket.hits[0].row.entity_id, format!("jira:{t}-title"));
    assert_eq!(ticket.total, 13);
    assert_eq!(r.total, 13);
}
