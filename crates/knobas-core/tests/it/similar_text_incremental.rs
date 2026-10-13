//! The similarity rule compares only what changed (#745), and proposes what
//! it proposed when it compared everything.
//!
//! The oracle is the **pre-#745 statement, verbatim** ([`WHOLE_CORPUS`]): it
//! judged every pair of live prose items on every pass. Two databases go
//! through the same sequence of syncs -- items arrive, change, are tombstoned,
//! come back, have their source disabled -- one running the old statement
//! after each step and one running [`suggest::detect_rule`]; after every step
//! the two hold the same proposals, reason included. The batch size is varied
//! down to one, so the three joins that replace the old one each carry pairs
//! the others do not.

use std::collections::BTreeSet;

use knobas_core::suggest;
use sqlx::PgPool;

/// `similar_text` as it stood at 0f3cb73f, before #745: the driver's head and
/// tail around the whole-corpus candidate select, expanded by hand.
const WHOLE_CORPUS: &str = "insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 confirmed_at, rule, rule_class, reason)
         select distinct on (least(c.from_id, c.to_id), greatest(c.from_id, c.to_id), c.relation)
                c.from_id, c.to_id, c.relation, $1, $2,
                null, $3, $4, c.reason
           from (with prose as (
         select entity_id, title, fts from sync.live_item where kind in ('ticket','page','pr')
     ),
     anchor as (
         select entity_id, fts,
                (select string_agg(quote_literal(w), ' | ')
                   from unnest(tsvector_to_array(to_tsvector('english', title))) w) as q
           from prose
     ),
     pair as (
         select a.entity_id as from_id, o.entity_id as to_id,
                array(select x from unnest(tsvector_to_array(a.fts)) x
                       intersect
                      select y from unnest(tsvector_to_array(o.fts)) y) as shared
           from anchor a
           join prose o
             on o.entity_id > a.entity_id
            and o.fts @@ to_tsquery('english', a.q)
          where a.q is not null
     )
     select from_id, to_id, 'related',
            format('both mention %s',
                   (select string_agg(s, ', ' order by s)
                      from (select unnest(shared) as s order by 1 limit 4) top))
       from pair
      where cardinality(shared) >= 5) as c(from_id, to_id, relation, reason)
          where c.from_id <> c.to_id
            and not exists (
                  select 1
                    from knobas.link l
                   where l.relation = c.relation
                     and (   (l.from_id = c.from_id and l.to_id = c.to_id)
                          or (l.from_id = c.to_id   and l.to_id = c.from_id)))
          order by least(c.from_id, c.to_id), greatest(c.from_id, c.to_id),
                   c.relation, c.from_id, c.reason
         on conflict do nothing";

/// Few enough words that about half the pairs clear the floor of five shared
/// stems and half do not.
const WORDS: &[&str] = &[
    "payout",
    "ledger",
    "retry",
    "storm",
    "invoice",
    "refund",
    "carrier",
    "freight",
    "customs",
    "tariff",
    "webhook",
    "partner",
    "release",
    "staging",
    "rollback",
    "migration",
    "index",
    "session",
    "token",
    "upload",
    "thumbnail",
    "latency",
    "dashboard",
    "alert",
    "monitor",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn words(&mut self, n: usize) -> String {
        (0..n)
            .map(|_| WORDS[(self.next() % WORDS.len() as u64) as usize])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("simincr")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One step of a sync, applied identically to both databases.
enum Step {
    /// Items `0..n` of `source` arrive, or are rewritten, with text from `seed`.
    Write {
        source: &'static str,
        keys: std::ops::Range<u32>,
        seed: u64,
    },
    Tombstone(&'static str, u32),
    Restore(&'static str, u32),
    Enabled(&'static str, bool),
}

async fn apply(pool: &PgPool, step: &Step) {
    match step {
        Step::Write { source, keys, seed } => {
            for key in keys.clone() {
                let mut rng = Rng(seed ^ (u64::from(key) + 1).wrapping_mul(0x9E37_79B9));
                let id = format!("{source}:K-{key}");
                let kind = ["ticket", "page", "pr", "branch"][(key % 4) as usize];
                let title = rng.words(4);
                let body = rng.words(12);
                sqlx::query(
                    "insert into knobas.entity (id, kind, title) values ($1, $2, $3)
                     on conflict (id) do update set title = excluded.title",
                )
                .bind(&id)
                .bind(kind)
                .bind(&title)
                .execute(pool)
                .await
                .unwrap();
                sqlx::query(
                    "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
                     values ($1, $2, $3, $4, $5, '{}')
                     on conflict (entity_id) do update
                        set title = excluded.title, body_text = excluded.body_text,
                            synced_at = now()",
                )
                .bind(&id)
                .bind(source)
                .bind(kind)
                .bind(&title)
                .bind(&body)
                .execute(pool)
                .await
                .unwrap();
            }
        }
        Step::Tombstone(source, key) | Step::Restore(source, key) => {
            let gone = matches!(step, Step::Tombstone(..));
            sqlx::query(
                "update knobas.entity set deleted_at = case when $2 then now() end where id = $1",
            )
            .bind(format!("{source}:K-{key}"))
            .bind(gone)
            .execute(pool)
            .await
            .unwrap();
        }
        Step::Enabled(source, enabled) => {
            sqlx::query(
                "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
                 values ($1, 'jira', $1, 'http://x', 'Pat', $2)
                 on conflict (id) do update set enabled = excluded.enabled",
            )
            .bind(source)
            .bind(enabled)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

/// Every similarity proposal: the pair as stored, and its reason.
async fn proposed(pool: &PgPool) -> BTreeSet<(String, String, String)> {
    sqlx::query_as("select from_id, to_id, reason from knobas.link where rule = 'similar_text'")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

async fn whole_corpus_pass(pool: &PgPool) {
    let rule = suggest::rule("similar_text").unwrap();
    sqlx::query(WHOLE_CORPUS)
        .bind(rule.origin.as_str())
        .bind(suggest::DETECTOR)
        .bind(rule.name)
        .bind(rule.class.as_str())
        .execute(pool)
        .await
        .unwrap();
}

/// The sequence both databases go through.
fn script() -> Vec<Step> {
    vec![
        Step::Write {
            source: "jira",
            keys: 0..24,
            seed: 1,
        },
        Step::Write {
            source: "wiki",
            keys: 0..12,
            seed: 2,
        },
        // New items beside settled ones, and settled ones rewritten.
        Step::Write {
            source: "jira",
            keys: 20..36,
            seed: 3,
        },
        Step::Tombstone("jira", 5),
        Step::Tombstone("wiki", 2),
        Step::Write {
            source: "wiki",
            keys: 4..8,
            seed: 4,
        },
        // Back with the text it left with: compared again, not trusted.
        Step::Restore("jira", 5),
        Step::Enabled("wiki", false),
        Step::Write {
            source: "jira",
            keys: 0..4,
            seed: 5,
        },
        Step::Enabled("wiki", true),
        Step::Restore("wiki", 2),
    ]
}

/// Batched passes over a mirror that keeps changing propose exactly what the
/// whole-corpus statement proposed over the same mirror, reason for reason.
#[tokio::test]
async fn incremental_passes_propose_what_the_whole_corpus_pass_proposed() {
    for batch in [1, 4, 1_000] {
        let (oracle, under_test) = (scratch().await, scratch().await);
        let rule = suggest::rule("similar_text").unwrap();
        for (n, step) in script().iter().enumerate() {
            apply(&oracle, step).await;
            apply(&under_test, step).await;
            whole_corpus_pass(&oracle).await;
            suggest::detect_rule_batched(&under_test, rule, batch)
                .await
                .unwrap();
            let (want, got) = (proposed(&oracle).await, proposed(&under_test).await);
            assert!(
                want.len() > 10,
                "step {n}: the corpus must propose enough to compare ({} proposals)",
                want.len()
            );
            assert_eq!(
                got, want,
                "step {n}, batch {batch}: the incremental passes disagree with the whole corpus"
            );
        }
    }
}

/// A pass with nothing new does not judge a single pair again.
///
/// Witnessed by withdrawing every proposal behind the detector's back: a pass
/// that re-judged the corpus would write them all again, one that compares
/// only what changed writes nothing until an item changes -- and then only
/// that item's pairs.
#[tokio::test]
async fn a_pass_with_nothing_new_judges_nothing() {
    let pool = scratch().await;
    let rule = suggest::rule("similar_text").unwrap();
    apply(
        &pool,
        &Step::Write {
            source: "jira",
            keys: 0..30,
            seed: 7,
        },
    )
    .await;
    let first = suggest::detect_rule(&pool, rule).await.unwrap();
    assert!(first > 10, "the corpus proposes something ({first})");

    sqlx::query("delete from knobas.link where rule = 'similar_text'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        suggest::detect_rule(&pool, rule).await.unwrap(),
        0,
        "nothing changed, so nothing was judged"
    );

    // One item (a pr) rewritten: its pairs, and only its pairs, are judged again.
    sqlx::query(
        "update sync.item set body_text = body_text || ' payout' where entity_id = 'jira:K-2'",
    )
    .execute(&pool)
    .await
    .unwrap();
    suggest::detect_rule(&pool, rule).await.unwrap();
    let touched = proposed(&pool).await;
    assert!(!touched.is_empty(), "the changed item's pairs were judged");
    assert!(
        touched
            .iter()
            .all(|(from, to, _)| from == "jira:K-2" || to == "jira:K-2"),
        "only the changed item's pairs: {touched:?}"
    );
}
