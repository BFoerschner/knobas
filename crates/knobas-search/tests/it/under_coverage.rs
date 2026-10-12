//! `under:` coverage is measured on the corpus, against a real PostgreSQL
//! (#642, Björn's ruling of 2026-10-12: option 1).
//!
//! A source answers `under:` only if some live item it synced names a parent
//! -- the **child arm** of `sync.item_parent`, a row written by
//! `declared_parent_names!` -- over all of the source's kinds, whatever the
//! query's kind scope. A row from the names-as-parent arm does not count:
//! every Jira ticket answers to its own key, so "the source has an
//! `item_parent` row" would vouch for a classic Jira that stores no `epic`.
//!
//! The adapters' real descriptors, and items shaped the way those adapters
//! store them: a classic Data Center child with `epic: {key}` beside the
//! record (#598, `epic_link_field` configured), a classic ticket with neither
//! `epic` nor `fields.parent` (the field never configured), a Gitea repo and a
//! commit carrying its `repository`, and a Kuma monitor, whose adapter
//! declares nothing. Its own database (one per test module, #572); every
//! source id and key carries the test's token.

use knobas_search::{FilterAnswer, FilterDimension, SearchFilters, SearchQuery, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

/// A token of letters only, so it can sit in a Jira key (`[A-Z][A-Z0-9_]*`).
fn token() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect()
}

/// A migrated database and the searcher the app builds: Jira's and Gitea's
/// declared payload paths. Kuma's template is not passed -- an adapter that
/// declares nothing.
async fn searcher() -> (sqlx::PgPool, Searcher) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let searcher = Searcher::declaring(
        pool.clone(),
        [
            knobas_source_jira::descriptor_template(),
            knobas_source_gitea::descriptor_template(),
        ],
    )
    .await
    .unwrap();
    (pool, searcher)
}

async fn configure(pool: &sqlx::PgPool, id: &str, adapter_kind: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $1, 'http://x', 'Pat')",
    )
    .bind(id)
    .bind(adapter_kind)
    .execute(pool)
    .await
    .unwrap();
}

async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$4,$4,$5, now())",
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

/// A classic Data Center ticket: no `fields.parent`; with `epic` beside the
/// record when the source has its Epic Link field configured.
fn classic(key: &str, epic: Option<&str>) -> serde_json::Value {
    let mut record = serde_json::json!({
        "id": "1", "key": key,
        "fields": { "summary": key, "issuetype": { "name": "Story" },
                    "status": { "name": "Open" } },
    });
    if let Some(epic) = epic {
        record["fields"]["customfield_10108"] = serde_json::json!(epic);
        record["epic"] = serde_json::json!({ "key": epic });
    }
    record
}

async fn under_answers(searcher: &Searcher, raw: &str) -> Vec<(String, FilterAnswer)> {
    let response = searcher.search(q(raw)).await.unwrap();
    let mut answers: Vec<(String, FilterAnswer)> = response
        .coverage
        .iter()
        .find(|c| c.dimension == FilterDimension::Under)
        .map(|c| {
            c.sources
                .iter()
                .map(|s| (s.source_id.clone(), s.answer))
                .collect()
        })
        .unwrap_or_default();
    answers.sort_by(|a, b| a.0.cmp(&b.0));
    answers
}

/// The unconfigured classic Jira is the case #642 is about: Jira declares a
/// parent for `ticket`, so the declaration said `answered`, while nothing the
/// source synced names an epic or a parent issue -- every row it has in
/// `sync.item_parent` is a ticket answering to its own key.
#[tokio::test]
async fn a_source_answers_under_only_if_something_it_synced_names_a_parent() {
    let (pool, searcher) = searcher().await;
    let t = token();
    let (configured, unconfigured, kuma) = (
        format!("jira-epics-{t}"),
        format!("jira-bare-{t}"),
        format!("kuma-{t}"),
    );
    configure(&pool, &configured, "jira").await;
    configure(&pool, &unconfigured, "jira").await;
    configure(&pool, &kuma, "kuma").await;

    let epic = format!("E{t}-100");
    item(&pool, &configured, "ticket", &epic, classic(&epic, None)).await;
    let child = format!("E{t}-101");
    item(
        &pool,
        &configured,
        "ticket",
        &child,
        classic(&child, Some(&epic)),
    )
    .await;
    for n in 200..203 {
        let key = format!("B{t}-{n}");
        item(&pool, &unconfigured, "ticket", &key, classic(&key, None)).await;
    }
    item(
        &pool,
        &kuma,
        "monitor",
        &format!("m-{t}"),
        serde_json::json!({ "name": format!("payments-{t}") }),
    )
    .await;

    let answers = under_answers(
        &searcher,
        &format!("under:{epic} source:{configured} source:{unconfigured} source:{kuma}"),
    )
    .await;
    assert_eq!(
        answers,
        sorted(vec![
            (configured.clone(), FilterAnswer::Answered),
            (unconfigured.clone(), FilterAnswer::NoValues),
            (kuma.clone(), FilterAnswer::NoValues),
        ]),
        "an unconfigured classic Jira has synced nothing that names a parent"
    );

    // Its one ticket in an epic is what made the configured source answer: a
    // Jira project where no ticket is in an epic has nothing to look under.
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(format!("{configured}:{child}"))
        .execute(&pool)
        .await
        .unwrap();
    let answers = under_answers(&searcher, &format!("under:{epic} source:{configured}")).await;
    assert_eq!(answers, vec![(configured, FilterAnswer::NoValues)]);
}

/// The verdict reads every kind of the source, not the query's kind scope:
/// `under:` also returns the parent itself, so `kind:repo under:<repo>` shows
/// the repo -- and Gitea, whose repos name no parent but whose commits do,
/// answers it -- under the adapter it is configured with.
#[tokio::test]
async fn a_kind_scope_does_not_narrow_the_verdict() {
    let (pool, searcher) = searcher().await;
    let t = token().to_lowercase();
    let gitea = format!("gitea-{t}");
    configure(&pool, &gitea, "gitea").await;
    let full = format!("o{t}/payout-{t}");
    let name = format!("payout-{t}");
    item(
        &pool,
        &gitea,
        "repo",
        &full,
        serde_json::json!({ "full_name": full, "name": name }),
    )
    .await;
    item(
        &pool,
        &gitea,
        "commit",
        &format!("c-{t}"),
        serde_json::json!({ "repository": { "full_name": full, "name": name } }),
    )
    .await;

    let answers = under_answers(&searcher, &format!("kind:repo under:{full} source:{gitea}")).await;
    assert_eq!(answers, vec![(gitea.clone(), FilterAnswer::Answered)]);

    // A stored row says what one adapter's declarations make of the item, and
    // counts only while the source is configured with that adapter (ADR-0019):
    // given an adapter that declares nothing, the commit names no parent.
    sqlx::query("update knobas.source_config set kind = 'kuma' where id = $1")
        .bind(&gitea)
        .execute(&pool)
        .await
        .unwrap();
    let answers = under_answers(&searcher, &format!("under:{full} source:{gitea}")).await;
    assert_eq!(answers, vec![(gitea, FilterAnswer::NoValues)]);
}

fn sorted(mut answers: Vec<(String, FilterAnswer)>) -> Vec<(String, FilterAnswer)> {
    answers.sort_by(|a, b| a.0.cmp(&b.0));
    answers
}
