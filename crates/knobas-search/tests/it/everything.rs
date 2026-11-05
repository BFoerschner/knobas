//! *Everything* (#2431, spec #2419): with `titles` off every hit carries the
//! excerpts where the words appear, each with its place -- a page's section,
//! a comment's author, a run log's line -- and a pipeline whose latest log
//! holds the words is a hit. With `titles` on a hit carries none.

use chrono::{DateTime, Utc};
use knobas_core::payload::StateChip;
use knobas_core::tone::Tone;
use knobas_search::{Excerpt, SearchFilters, SearchHit, SearchQuery, SearchResponse, Searcher};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind) in [
        ("teamcity", "teamcity"),
        ("jira", "jira"),
        ("confluence", "confluence"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $2, 'http://x', 'Pat')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [
            knobas_source_teamcity::descriptor_template(),
            knobas_source_jira::descriptor_template(),
            knobas_source_confluence::descriptor_template(),
        ],
    )
    .await
    .unwrap()
}

/// A word no other row holds, in letters only, so it lexes as one word.
fn token() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..12]
        .chars()
        .map(|c| match c.to_digit(16) {
            Some(n) => char::from(b'g' + u8::try_from(n).unwrap()),
            None => c,
        })
        .collect();
    format!("ev{letters}")
}

async fn search(pool: &sqlx::PgPool, raw: &str, titles: bool) -> SearchResponse {
    searcher(pool)
        .await
        .search(SearchQuery {
            titles,
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
}

fn all_hits(response: &SearchResponse) -> Vec<&SearchHit> {
    response.groups.iter().flat_map(|g| &g.hits).collect()
}

fn the_hit<'a>(response: &'a SearchResponse, id: &str) -> &'a SearchHit {
    all_hits(response)
        .into_iter()
        .find(|hit| hit.row.entity_id == id)
        .unwrap_or_else(|| panic!("no hit {id} in {response:#?}"))
}

/// Each excerpt as `(place, the whole text, the marked words)`.
fn read(excerpts: &[Excerpt]) -> Vec<(Option<String>, String, Vec<String>)> {
    excerpts
        .iter()
        .map(|e| {
            (
                e.place.clone(),
                e.segments.iter().map(|s| s.text.as_str()).collect(),
                e.segments
                    .iter()
                    .filter(|s| s.hit)
                    .map(|s| s.text.clone())
                    .collect(),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
    at: DateTime<Utc>,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1,$2,$3,$4,$5,$6,$7,$7)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A TeamCity configuration `<t>_Deploy` named `name`, with one failed run
/// that finished an hour ago. Answers the configuration's and the run's id.
async fn pipeline(pool: &sqlx::PgPool, t: &str, name: &str) -> (String, String) {
    let at = Utc::now();
    let config = format!("{t}_Deploy");
    let id = item(
        pool,
        "teamcity",
        "build_config",
        &config,
        name,
        "",
        serde_json::json!({
            "id": config, "name": name,
            "projectId": "Payouts", "projectName": "Payouts",
            "projectPath": [{ "id": "Root", "name": "Root" }, { "id": "Payouts", "name": "Payouts" }],
        }),
        at,
    )
    .await;
    let run = item(
        pool,
        "teamcity",
        "build",
        &format!("{t}9"),
        &format!("{name} #9"),
        "",
        serde_json::json!({
            "buildTypeId": config, "number": "9", "state": "finished", "status": "FAILURE",
            "finishDate": "20261102T100000+0000",
            "buildType": { "id": config, "name": name, "projectId": "Payouts" },
        }),
        at - chrono::Duration::hours(1),
    )
    .await;
    (id, run)
}

async fn log(pool: &sqlx::PgPool, pipeline: &str, run: &str, first_line: i64, body: &str) {
    sqlx::query(
        "insert into sync.pipeline_log (pipeline_id, run_id, first_line, body)
         values ($1, $2, $3, $4)",
    )
    .bind(pipeline)
    .bind(run)
    .bind(first_line)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_word_only_in_a_run_log_answers_the_pipeline_coloured_by_its_last_run() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let (id, run) = pipeline(&pool, &t, "Deploy payouts").await;
    log(
        &pool,
        &id,
        &run,
        381,
        &format!("Step 3/4: tests\nconnection refused {t} at 10:02\nBuild failed"),
    )
    .await;

    let response = search(&pool, &t, false).await;
    let hits = all_hits(&response);
    assert_eq!(hits.len(), 1, "{response:#?}");
    let hit = hits[0];
    assert_eq!(
        (hit.row.kind.as_str(), hit.row.entity_id.as_str()),
        ("pipeline", id.as_str())
    );
    assert_eq!(hit.row.path.as_deref(), Some("Root › Payouts"));
    assert_eq!(
        hit.row.state,
        Some(StateChip {
            text: "FAILURE".to_owned(),
            tone: Some(Tone::Err)
        })
    );
    assert_eq!(
        read(&hit.excerpts),
        vec![(
            Some("line 382".to_owned()),
            format!("connection refused {t} at 10:02"),
            vec![t.clone()]
        )]
    );

    // Titles reads what is on screen, and a log is not.
    assert!(all_hits(&search(&pool, &t, true).await).is_empty());
}

#[tokio::test]
async fn a_pipeline_its_name_and_its_log_both_match_is_one_hit_with_the_log_lines() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let (id, run) = pipeline(&pool, &t, &format!("Deploy {t}")).await;
    log(&pool, &id, &run, 1, &format!("checkout\n{t} started\nok")).await;

    let response = search(&pool, &t, false).await;
    let hits = all_hits(&response);
    assert_eq!(hits.len(), 1, "{response:#?}");
    assert_eq!(response.groups[0].total, 1);
    assert_eq!(
        read(&hits[0].excerpts),
        vec![(
            Some("line 2".to_owned()),
            format!("{t} started"),
            vec![t.clone()]
        )]
    );
}

#[tokio::test]
async fn a_page_hit_names_the_section_and_the_comment_its_words_sit_in() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let body = format!(
        "Retry runbook\nHow payouts retry.\n\u{1e}Backoff policy\nWait, then {t} again.\n\
         \u{1f}Priya\nShould {t} be capped?\n\u{1d}ENG\u{1d}Other page\u{1d}"
    );
    let id = item(
        &pool,
        "confluence",
        "page",
        "4242",
        "Retry runbook",
        &body,
        serde_json::json!({}),
        Utc::now(),
    )
    .await;

    let response = search(&pool, &t, false).await;
    assert_eq!(
        read(&the_hit(&response, &id).excerpts),
        vec![
            (
                Some("Backoff policy".to_owned()),
                format!("Wait, then {t} again."),
                vec![t.clone()]
            ),
            (
                Some("Comment by Priya".to_owned()),
                format!("Should {t} be capped?"),
                vec![t.clone()]
            ),
        ]
    );
}

/// #2481, end to end: the index finds the row by a comment author's name, and
/// the hit quotes that line.
#[tokio::test]
async fn a_hit_by_a_comments_author_quotes_the_author_line() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let id = item(
        &pool,
        "jira",
        "ticket",
        &format!("{}-2", t.to_uppercase()),
        "Payout retries",
        &format!("Payout retries\n\nThe batch fails.\n\u{1f}{t} Nair\nLooks wrong."),
        serde_json::json!({}),
        Utc::now(),
    )
    .await;

    let response = search(&pool, &t, false).await;
    assert_eq!(
        read(&the_hit(&response, &id).excerpts),
        vec![(None, format!("Comment by {t} Nair"), vec![t.clone()])]
    );
}

/// The stem the excerpts' rule misses (#2481, `excerpt.rs`'s module doc): the
/// index stems `<t>xies` and `<t>xy` alike and finds the row, the reader's rule
/// looks for `<t>xi`, and the hit quotes nothing. Why the preview's sentence
/// for an empty list names no place: this row's title does not hold the word.
#[tokio::test]
async fn a_hit_through_a_stem_the_reader_misses_carries_no_excerpt() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let id = item(
        &pool,
        "jira",
        "ticket",
        &format!("{}-3", t.to_uppercase()),
        "Payout retries",
        &format!("Payout retries\n\nThe {t}xy batch fails."),
        serde_json::json!({}),
        Utc::now(),
    )
    .await;

    let response = search(&pool, &format!("{t}xies"), false).await;
    assert!(the_hit(&response, &id).excerpts.is_empty());
}

#[tokio::test]
async fn a_ticket_hit_quotes_its_text_and_not_its_title() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let id = item(
        &pool,
        "jira",
        "ticket",
        &format!("{}-1", t.to_uppercase()),
        "Payout retries",
        &format!("Payout retries\n\nThe {t} batch fails on AC04.\n\nSecond paragraph."),
        serde_json::json!({}),
        Utc::now(),
    )
    .await;

    let response = search(&pool, &format!("{t} ac04"), false).await;
    let excerpts = read(&the_hit(&response, &id).excerpts);
    assert_eq!(
        excerpts,
        vec![(
            None,
            format!("The {t} batch fails on AC04."),
            vec![t.clone(), "AC04".to_owned()]
        )]
    );
}

#[tokio::test]
async fn under_titles_a_hit_carries_no_excerpt() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let id = item(
        &pool,
        "jira",
        "ticket",
        &format!("{}-2", t.to_uppercase()),
        &format!("Payout {t}"),
        &format!("Payout {t}\n\nThe {t} batch."),
        serde_json::json!({}),
        Utc::now(),
    )
    .await;

    let titles = search(&pool, &t, true).await;
    assert!(the_hit(&titles, &id).excerpts.is_empty());
    let everything = search(&pool, &t, false).await;
    assert_eq!(the_hit(&everything, &id).excerpts.len(), 1);
}
