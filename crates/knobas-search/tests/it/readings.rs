//! `Searcher::readings` (#812, #1654) against a real PostgreSQL: what a
//! stored ticket view reads as today, and that all of them are counted in one
//! statement. One database for the file (#572), a project key unique to each
//! test, so tests never see each other's rows.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use knobas_search::{Reading, Searcher};

fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect();
    format!("R{letters}")
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // A row of a source that is not configured is not live.
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{}')
         on conflict (id) do update set kind = excluded.kind, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn store(pool: &sqlx::PgPool, project: &str, n: u32, issuetype: &str) {
    let key = format!("{project}-{n}");
    let id = format!("jira:{key}");
    let payload = serde_json::json!({
        "id": n.to_string(),
        "key": key,
        "fields": {
            "summary": format!("{key} summary"),
            "status": { "name": "To Do" },
            "issuetype": { "name": issuetype },
            "project": { "key": project, "name": project },
        },
    });
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(&key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'jira','ticket',$2,$2,$3, now())",
    )
    .bind(&id)
    .bind(&key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap()
}

/// The three arms, each beside the others so a reading lands on its own text:
/// a count, a count of nothing, and a text the grammar cannot read.
#[tokio::test]
async fn readings_count_unreadable_and_empty_texts_each_in_its_own_slot() {
    let pool = pool().await;
    let p = project();
    store(&pool, &p, 1, "Bug").await;
    store(&pool, &p, 2, "Bug").await;
    store(&pool, &p, 3, "Story").await;
    let searcher = searcher(&pool).await;

    let bugs = format!("project:{p} issuetype:bug");
    let none = format!("project:{p} zeppelins");
    let all = format!("project:{p}");
    let unknown = format!("project:{p} updated:soonish");
    let other_kind = format!("project:{p} type:note");
    let long = "x".repeat(10_000);
    let texts = [
        bugs.as_str(),
        unknown.as_str(),
        none.as_str(),
        all.as_str(),
        other_kind.as_str(),
        long.as_str(),
    ];
    let readings = searcher.readings(&texts, "ticket").await.unwrap();

    assert_eq!(readings.len(), texts.len());
    assert_eq!(readings[0], Reading::Count(2), "two bugs");
    assert_eq!(
        readings[1],
        Reading::Unreadable("updated:soonish".to_owned()),
        "the token the grammar did not know"
    );
    assert_eq!(readings[2], Reading::Count(0), "a readable text nothing matches");
    assert_eq!(readings[3], Reading::Count(3), "a filter alone counts its rows");
    assert_eq!(readings[4], Reading::Count(0), "another kind admits no ticket");
    assert!(matches!(readings[5], Reading::Unreadable(_)), "past the bound");
    assert_eq!(searcher.readings(&[], "ticket").await.unwrap(), vec![]);
}

/// Ten saved views cost one statement, not ten scans (#1654): the pool hands
/// out the same number of connections for ten views as for one.
#[tokio::test]
async fn ten_views_are_counted_in_one_statement() {
    let pool = pool().await;
    let p = project();
    for n in 1..=4 {
        store(&pool, &p, n, if n % 2 == 0 { "Bug" } else { "Story" }).await;
    }
    let acquired = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&acquired);
    let counted = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .before_acquire(move |_, _| {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(true) })
        })
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let searcher = searcher(&counted).await;

    let text = format!("project:{p} issuetype:bug");
    // Connections the pool hands out for one call.
    let used = async |texts: &[&str]| {
        let before = acquired.load(Ordering::SeqCst);
        let readings = searcher.readings(texts, "ticket").await.unwrap();
        (acquired.load(Ordering::SeqCst) - before, readings)
    };
    used(&[&text]).await; // warm: whatever the searcher caches is cached
    let (one, _) = used(&[&text]).await;
    let (ten, readings) = used(&[text.as_str(); 10]).await;
    assert_eq!(readings, vec![Reading::Count(2); 10]);
    assert_eq!(ten, one, "ten views take no more connections than one");
}
