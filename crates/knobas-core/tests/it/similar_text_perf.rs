//! A detection pass with nothing new costs under a second at 10,000 items
//! (#745).
//!
//! Before #745 `similar_text` judged every pair of prose items on every pass,
//! so a pass over an unchanged mirror cost what the first one did: 68 s at
//! 2,000 items, 127 s at 4,000 (the #745 investigation's readings). Now a pass
//! compares only what changed, and with nothing changed it reads the corpus
//! once to find that out.
//!
//! **A reading, so `#[ignore]`d** for the reason `knobas-search`'s
//! `tests/perf.rs` is (`docs/agents/working-model.md`): a wall-clock budget
//! inside a gate measures the machine. Take it with `just suggest-perf`. What
//! the gate keeps is the behaviour behind the number,
//! `similar_text_incremental::a_pass_with_nothing_new_judges_nothing`.

use std::time::{Duration, Instant};

use knobas_core::suggest;
use sqlx::PgPool;

const ITEMS: i32 = 10_000;
const BUDGET: Duration = Duration::from_secs(1);

/// `ITEMS` mirrored items, seven in ten prose (ticket, page, pull request)
/// and the rest branches and commits naming ticket keys, so every rule has
/// rows to read. Words are drawn from 4,000 synthetic ones, six to a title and
/// forty to a body; `setseed` makes the corpus the same on every run.
async fn seed(pool: &PgPool) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("select setseed(0.745)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "create temporary table seed on commit drop as
         select g,
                'jira:PAY-' || g as id,
                (array['ticket','page','pr','ticket','page','pr','ticket','branch','commit','branch'])
                  [1 + g % 10] as kind,
                (select string_agg('w' || (random() * 4000)::int, ' ')
                   from generate_series(1, 6 + g * 0)) as title,
                (select string_agg('w' || (random() * 4000)::int, ' ')
                   from generate_series(1, 40 + g * 0)) as body
           from generate_series(1, $1) g",
    )
    .bind(ITEMS)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) select id, kind, title from seed")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select id, 'jira', kind,
                case when kind in ('branch','commit') then 'PAY-' || (g / 3) || ' ' || title
                     else title end,
                body, '{}'
           from seed",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("analyze").execute(pool).await.unwrap();
}

#[tokio::test]
#[ignore = "a wall-clock reading; run by `just suggest-perf`"]
async fn a_pass_with_nothing_new_takes_under_a_second_at_ten_thousand_items() {
    let pool = knobas_db::test_util::scratch_database("simperf")
        .await
        .pool(2)
        .await
        .unwrap();
    seed(&pool).await;

    let start = Instant::now();
    let written = suggest::detect(&pool).await.unwrap();
    println!(
        "suggest-perf: first pass over {ITEMS} items wrote {written} in {} ms",
        start.elapsed().as_millis()
    );

    // The fastest of five, as `builds-perf` takes the fastest of ten: what is
    // read is the pass, not the machine's worst moment.
    let mut fastest = Duration::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        let mut conn = pool.acquire().await.unwrap();
        for rule in suggest::RULES {
            let rule_start = Instant::now();
            let written = suggest::detect_rule(&mut *conn, rule).await.unwrap();
            assert_eq!(
                written, 0,
                "{} proposed something on an unchanged mirror",
                rule.name
            );
            println!(
                "suggest-perf:   {:<26} {:>5} ms",
                rule.name,
                rule_start.elapsed().as_millis()
            );
        }
        let took = start.elapsed();
        println!("suggest-perf: nothing-new pass {} ms", took.as_millis());
        fastest = fastest.min(took);
    }
    println!(
        "suggest-perf: fastest nothing-new pass at {ITEMS} items: {} ms (budget {} ms)",
        fastest.as_millis(),
        BUDGET.as_millis()
    );
    assert!(
        fastest < BUDGET,
        "a pass with nothing new took {} ms at {ITEMS} items",
        fastest.as_millis()
    );
}
