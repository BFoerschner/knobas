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
use knobas_search::{
    KindCatalog, ResultGroup, SearchFilters, SearchQuery, SearchResponse, Searcher,
};

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

/// One route on an asset, the way the asset store writes it.
async fn seed_route(pool: &sqlx::PgPool, id: &str, asset_id: &str, name: &str, url: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'route',$2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.route (id, asset_id, name, url) values ($1,$2,$3,$4)")
        .bind(id)
        .bind(asset_id)
        .bind(name)
        .bind(url)
        .execute(pool)
        .await
        .unwrap();
}

/// One kind's group, or a failure that prints the whole response.
fn group<'r>(results: &'r SearchResponse, kind: &str) -> &'r ResultGroup {
    results
        .groups
        .iter()
        .find(|group| group.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} group in {results:?}"))
}

/// The ids of one group's rows, in the order the launcher draws them.
fn group_ids(results: &SearchResponse, kind: &str) -> Vec<String> {
    group(results, kind)
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.clone())
        .collect()
}

fn days_ago(n: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(n)
}

/// Story 25, in two mirrored kinds and three kinds knobas owns.
///
/// In each kind the title match is the older row, and the body match mentions
/// the word exactly once -- the comparable item the criterion asks for. A
/// ticket and a commit are both mirror rows; the note (`knobas.note`), the
/// asset (`knobas.asset`) and the route (`knobas.route`) are corpora of their
/// own, so "every kind" is asserted across the weighting of every relation
/// `corpus::ALL` searches. Every mirrored kind shares `sync.item`'s one `fts`,
/// so the two mirror kinds stand for all of them.
///
/// **A route's body is its URL.** Migration `0018` weighted a route's name and
/// its URL both at A, so a route named for the word and a route that only
/// carried it in its URL tied and recency decided. The deputy's ruling on #592
/// (`docs/archive/2026-10-unattended-rulings.md`) made the URL the body, and
/// `0026` moved it to B; the route pair below is that migration's witness.
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
    // The name is hyphenated on purpose. `to_tsvector` gives `{t}-db` a lexeme
    // for the whole *and* one for each part, and before #611 the launcher's
    // prefix query counted the token twice in the name -- enough to outrank
    // the property at equal weights, which would leave this pair blind to the
    // weighting it is here to witness. A finished word now ranks by its whole
    // matches (`sql.rs`), so the name counts the token once.
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

    // A route named for the token, and one whose URL alone carries it -- as
    // its host, which `to_tsvector` gives a lexeme of its own when the URL
    // ends at `/` (`corpus::ROUTE`'s table). Both hang off the title asset;
    // the URL match is created second, so it is the newer.
    seed_route(
        &pool,
        &format!("route:{t}-title"),
        &format!("asset:{t}-title"),
        &format!("{t} dashboard"),
        "http://127.0.0.1:3000/",
    )
    .await;
    seed_route(
        &pool,
        &format!("route:{t}-body"),
        &format!("asset:{t}-title"),
        "Admin console",
        &format!("http://{t}:8080/"),
    )
    .await;

    let r = searcher(&pool).search(q(&t)).await.unwrap();

    assert_eq!(
        group_ids(&r, "route"),
        [format!("route:{t}-title"), format!("route:{t}-body")],
        "{r:?}"
    );
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

/// The per-kind cap, the global limit and the true total, under the weighting.
///
/// Twelve body matches, all newer than the one title match, in one kind. The
/// group shows its cap of rows, the title match is the first of them though it
/// is the oldest row in the kind, and the group still reports all thirteen --
/// the cut is a page, not a count. The same holds when the global limit cuts
/// deeper than the cap.
#[tokio::test]
async fn the_cap_and_the_limit_keep_the_title_match_and_the_true_total() {
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
    let ticket = group(&r, "ticket");

    // Ten is the crate's per-kind cap (`PER_GROUP` in `src/lib.rs`, private).
    assert_eq!(ticket.hits.len(), 10, "{r:?}");
    assert_eq!(
        ticket.hits[0].row.entity_id,
        format!("jira:{t}-title"),
        "{r:?}"
    );
    assert_eq!(ticket.total, 13, "{r:?}");
    assert_eq!(r.total, 13, "{r:?}");

    // And the global limit, cutting below the cap: still the title match
    // first, still all thirteen counted.
    let mut narrow = q(&t);
    narrow.limit = 3;
    let r = searcher(&pool).search(narrow).await.unwrap();
    assert_eq!(
        group_ids(&r, "ticket")[0],
        format!("jira:{t}-title"),
        "{r:?}"
    );
    assert_eq!(group(&r, "ticket").hits.len(), 3, "{r:?}");
    assert_eq!(group(&r, "ticket").total, 13, "{r:?}");
    assert_eq!(r.total, 13, "{r:?}");
}

/// Stories 1 and 25: the item named exactly the typed word comes first, above
/// every sibling whose hyphenated name only starts with it (#611).
///
/// `to_tsvector` lexes `payout-service` as `payout-servic`, `payout` and
/// `servic`, and the launcher's prefix query `payout:*` matched the first two,
/// so a hyphenated name counted the word twice and scored 2 against the plain
/// name's 1. The plain name is the oldest row in each kind, so recency can
/// only produce the wrong answer. An asset (`knobas.asset`) and a page (the
/// mirror's `sync.item`) are two relations with two `fts` columns.
#[tokio::test]
async fn the_item_named_exactly_the_word_ranks_above_its_hyphenated_siblings() {
    let pool = pool().await;
    let t = token("named");

    // Inserted in order: the plain name is the oldest asset.
    for (suffix, name) in [
        ("plain", t.clone()),
        ("service", format!("{t}-service")),
        ("worker", format!("{t}-worker")),
    ] {
        seed_asset(
            &pool,
            &format!("asset:{t}-{suffix}"),
            &name,
            serde_json::json!({}),
        )
        .await;
    }
    for (suffix, title, age) in [
        ("plain", t.clone(), 30),
        ("service", format!("{t}-service"), 2),
        ("worker", format!("{t}-worker"), 1),
    ] {
        seed(
            &pool,
            &format!("confluence:{t}-{suffix}"),
            "page",
            &title,
            "Attendees, action items, nothing else.",
            days_ago(age),
        )
        .await;
    }

    let r = searcher(&pool).search(q(&t)).await.unwrap();

    assert_eq!(
        group_ids(&r, "asset")[0],
        format!("asset:{t}-plain"),
        "{r:?}"
    );
    assert_eq!(
        group_ids(&r, "page")[0],
        format!("confluence:{t}-plain"),
        "{r:?}"
    );
    assert_eq!(group(&r, "asset").total, 3, "{r:?}");
    assert_eq!(group(&r, "page").total, 3, "{r:?}");
}

/// Story 25 past the per-kind cap (#1650): the item named exactly the word is
/// kept when more than a kind's cap of newer rows tie with it on rank.
///
/// The thirteen siblings each hold the word once in the title, as the plain
/// name does, so all fourteen rank alike and only `named` tells them apart.
/// The plain name is the oldest, so a cap that cut by rank and recency before
/// asking which row is named would cut it, and the tiebreak would then reorder
/// ten rows that do not include it.
#[tokio::test]
async fn the_item_named_exactly_the_word_survives_a_cap_of_equally_ranked_siblings() {
    let pool = pool().await;
    let t = token("tie");

    seed(
        &pool,
        &format!("jira:{t}-plain"),
        "ticket",
        &t,
        "Attendees, action items, nothing else.",
        days_ago(30),
    )
    .await;
    for n in 0..13 {
        seed(
            &pool,
            &format!("jira:{t}-sibling-{n:02}"),
            "ticket",
            &format!("{t} rollout {n}"),
            "Attendees, action items, nothing else.",
            Utc::now() - Duration::minutes(n),
        )
        .await;
    }

    for raw in [t.clone(), format!("{t} ")] {
        let r = searcher(&pool).search(q(&raw)).await.unwrap();
        let ticket = group(&r, "ticket");
        // Ten is the crate's per-kind cap (`PER_GROUP` in `src/lib.rs`).
        assert_eq!(ticket.hits.len(), 10, "{raw:?}: {r:?}");
        assert_eq!(
            ticket.hits[0].row.entity_id,
            format!("jira:{t}-plain"),
            "{raw:?}: {r:?}"
        );
        assert_eq!(ticket.total, 14, "{raw:?}: {r:?}");
        // The ranks really are tied, or this pins nothing about the cap.
        let first = ticket.hits[0].rank;
        assert!(
            ticket.hits.iter().all(|h| h.rank == first),
            "{raw:?}: {r:?}"
        );
    }
}

/// A hyphenated name stays findable by its full spelling and by each part
/// (#611): the fix changes how a match ranks, never whether it matches.
///
/// The second part is a token of its own, so a search for it alone answers
/// from this test's rows and not from whatever else the shared database holds.
#[tokio::test]
async fn a_hyphenated_name_is_found_by_its_whole_spelling_and_by_each_part() {
    let pool = pool().await;
    let t = token("whole");
    let u = token("part");
    let name = format!("{t}-{u}");
    let id = format!("confluence:{t}");

    seed(
        &pool,
        &id,
        "page",
        &name,
        "Attendees, action items, nothing else.",
        days_ago(1),
    )
    .await;

    let searcher = searcher(&pool);
    for typed in [name.clone(), t.clone(), u.clone()] {
        let r = searcher.search(q(&typed)).await.unwrap();
        assert_eq!(group_ids(&r, "page"), [id.as_str()], "{typed}: {r:?}");
    }
}
