//! The similarity rule compares only what changed (#745), and proposes each
//! item's ten most similar counterparts out of what it proposed when it
//! compared everything (#771).
//!
//! The oracle is the **pre-#745 statement, verbatim** ([`OLD_PAIRS`]): it
//! judged every pair of live prose items on every pass, and every pair it
//! judged similar was proposed. #771 caps that: each item ranks its similar
//! counterparts by distinct shared lexemes, ties to the newest, and a pair is
//! proposed when **either** end ranks the other in its ten. The ranking is
//! done here, in Rust, from the old statement's pairs, so it is not the
//! statement under test grading itself. One database goes through a sequence
//! of syncs -- items arrive, change, are tombstoned, come back, have their
//! source disabled, the reader accepts and dismisses -- with
//! [`suggest::detect_rule`] after each step, and after every step its
//! unanswered proposals are exactly the capped old set, each one written by
//! that pass with the old statement's reason, and nothing the reader answered
//! was touched. The batch size is varied down to
//! one, so the three joins that replace the old one each carry pairs the
//! others do not.

use std::collections::{BTreeMap, BTreeSet};

use knobas_core::suggest;
use sqlx::PgPool;

/// `similar_text`'s candidate select as it stood at 0f3cb73f, before #745,
/// expanded by hand, its last `select` widened by what #771 ranks on: the
/// number of shared lexemes and each end's time.
const OLD_PAIRS: &str = "with prose as (
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
     select from_id, to_id,
            format('both mention %s',
                   (select string_agg(s, ', ' order by s)
                      from (select unnest(shared) as s order by 1 limit 4) top)),
            cardinality(shared)::int8,
            (select coalesce(i.item_updated_at, i.synced_at) from sync.item i
              where i.entity_id = from_id),
            (select coalesce(i.item_updated_at, i.synced_at) from sync.item i
              where i.entity_id = to_id)
       from pair
      where cardinality(shared) >= 5";

type At = Option<chrono::DateTime<chrono::Utc>>;

/// One pair the old statement judged similar.
struct OldPair {
    from: String,
    to: String,
    reason: String,
    shared: i64,
    from_at: At,
    to_at: At,
}

/// The old set, and the pairs of it #771 keeps: those one of whose ends ranks
/// the other among its ten, by shared lexemes, then the newest, then the id.
/// Also the most counterparts any one item had, so a test can tell the cap
/// was reached.
fn capped(old: &[OldPair]) -> (BTreeMap<(String, String), String>, usize) {
    let mut counterparts: BTreeMap<&str, Vec<(i64, At, &str)>> = BTreeMap::new();
    for p in old {
        counterparts
            .entry(&p.from)
            .or_default()
            .push((p.shared, p.to_at, &p.to));
        counterparts
            .entry(&p.to)
            .or_default()
            .push((p.shared, p.from_at, &p.from));
    }
    let widest = counterparts.values().map(Vec::len).max().unwrap_or(0);
    let mut ranked: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (item, list) in &mut counterparts {
        // Most shared first; newest first, an item with no time last; then id.
        list.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| match (a.1, b.1) {
                    (Some(x), Some(y)) => y.cmp(&x),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
                .then_with(|| a.2.cmp(b.2))
        });
        for (_, _, other) in list.iter().take(suggest::SIMILAR_TEXT_CAP) {
            ranked.insert((item, other));
        }
    }
    let kept = old
        .iter()
        .filter(|p| {
            ranked.contains(&(p.from.as_str(), p.to.as_str()))
                || ranked.contains(&(p.to.as_str(), p.from.as_str()))
        })
        .map(|p| ((p.from.clone(), p.to.clone()), p.reason.clone()))
        .collect();
    (kept, widest)
}

async fn old_pairs(pool: &PgPool) -> Vec<OldPair> {
    let rows: Vec<(String, String, String, i64, At, At)> =
        sqlx::query_as(OLD_PAIRS).fetch_all(pool).await.unwrap();
    rows.into_iter()
        .map(|(from, to, reason, shared, from_at, to_at)| OldPair {
            from,
            to,
            reason,
            shared,
            from_at,
            to_at,
        })
        .collect()
}

/// Few enough words that about half the pairs clear the floor of five shared
/// stems and half do not. The last six are tokens the parser splits or keeps
/// whole in ways a plain word does not -- a hyphenated word, a version, a
/// path, a URL, a host, an address. A title query built from them holds
/// phrases, and the index that finds a changed item's settled anchors must
/// still find every anchor whose query the item matches.
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
    "foo-bar",
    "v1.2.3",
    "ci/cd",
    "http://x.example.com/a/b?c=d",
    "x.example.com",
    "mara@example.com",
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
    /// The reader accepts the `n`th unanswered proposal and dismisses the one
    /// after it, in id order.
    Answer(usize),
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
                // Half the items carry a source time, from a few days so
                // that times tie as well; the rest rank by `synced_at`.
                let at = (rng.next() % 2 == 0).then(|| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(
                        1_760_000_000 + 86_400 * (rng.next() % 3) as i64,
                        0,
                    )
                    .unwrap()
                });
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
                    "insert into sync.item
                            (entity_id, source_id, kind, title, body_text, item_updated_at, payload)
                     values ($1, $2, $3, $4, $5, $6, '{}')
                     on conflict (entity_id) do update
                        set title = excluded.title, body_text = excluded.body_text,
                            item_updated_at = excluded.item_updated_at, synced_at = now()",
                )
                .bind(&id)
                .bind(source)
                .bind(kind)
                .bind(&title)
                .bind(&body)
                .bind(at)
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
        Step::Answer(n) => {
            let open: Vec<uuid::Uuid> = sqlx::query_scalar(
                "select id from knobas.proposed_link where rule = 'similar_text' order by id",
            )
            .fetch_all(pool)
            .await
            .unwrap();
            suggest::accept(pool, open[*n]).await.unwrap().unwrap();
            suggest::dismiss(pool, open[n + 1]).await.unwrap().unwrap();
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

/// Every unanswered similarity proposal: the pair as stored, and its reason.
async fn proposed(pool: &PgPool) -> BTreeSet<(String, String, String)> {
    sqlx::query_as(
        "select from_id, to_id, reason from knobas.proposed_link where rule = 'similar_text'",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .collect()
}

/// Every similarity row the reader answered, as it stands.
async fn answered(pool: &PgPool) -> BTreeSet<(uuid::Uuid, String, String, bool, bool)> {
    sqlx::query_as(
        "select id, from_id, to_id, confirmed_at is not null, deleted_at is not null
           from knobas.link
          where rule = 'similar_text' and (confirmed_at is not null or deleted_at is not null)",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .collect()
}

/// The sequence the database goes through.
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
        Step::Answer(0),
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
        Step::Answer(3),
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
        Step::Write {
            source: "jira",
            keys: 36..48,
            seed: 6,
        },
    ]
}

/// Batched passes over a mirror that keeps changing propose exactly the old
/// whole-corpus set capped at each item's ten, with the old reasons: every pair
/// proposed is in the old set and within at least one end's ten, every such
/// pair is proposed, and what the reader answered stays as answered.
#[tokio::test]
async fn incremental_passes_propose_the_whole_corpus_set_capped_at_ten() {
    for batch in [1, 4, 1_000] {
        let pool = scratch().await;
        let rule = suggest::rule("similar_text").unwrap();
        let mut reader = BTreeSet::new();
        let mut answers = 0;
        let mut capped_somewhere = false;
        for (n, step) in script().iter().enumerate() {
            apply(&pool, step).await;
            if matches!(step, Step::Answer(_)) {
                reader = answered(&pool).await;
                answers += 2;
                assert_eq!(reader.len(), answers, "step {n}: answered");
            }
            let before: BTreeSet<(String, String)> = proposed(&pool)
                .await
                .into_iter()
                .map(|(from, to, _)| (from, to))
                .collect();
            suggest::detect_rule_batched(&pool, rule, batch)
                .await
                .unwrap();

            let old = old_pairs(&pool).await;
            let (kept, widest) = capped(&old);
            capped_somewhere |= widest > suggest::SIMILAR_TEXT_CAP;
            let answered_pairs: BTreeSet<(String, String)> = reader
                .iter()
                .map(|(_, from, to, _, _)| (from.clone(), to.clone()))
                .collect();
            let want: BTreeSet<(String, String)> = kept
                .keys()
                .filter(|(from, to)| {
                    !answered_pairs.contains(&(from.clone(), to.clone()))
                        && !answered_pairs.contains(&(to.clone(), from.clone()))
                })
                .cloned()
                .collect();
            // A proposal with an end that left the corpus stays, as it always
            // has (the tray marks the withdrawn end); the cap is a statement
            // about the live corpus.
            let live: BTreeSet<String> = sqlx::query_scalar(
                "select entity_id from sync.live_item where kind in ('ticket','page','pr')",
            )
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
            let got_rows: BTreeSet<(String, String, String)> = proposed(&pool)
                .await
                .into_iter()
                .filter(|(from, to, _)| live.contains(from) && live.contains(to))
                .collect();
            let got: BTreeSet<(String, String)> = got_rows
                .iter()
                .map(|(from, to, _)| (from.clone(), to.clone()))
                .collect();
            assert!(
                want.len() > 10,
                "step {n}: the corpus must propose enough to compare ({} proposals)",
                want.len()
            );
            let old_set: BTreeMap<(String, String), String> = old
                .iter()
                .map(|p| ((p.from.clone(), p.to.clone()), p.reason.clone()))
                .collect();
            for (from, to, reason) in &got_rows {
                let pair = (from.clone(), to.clone());
                assert!(
                    old_set.contains_key(&pair),
                    "step {n}, batch {batch}: {from} / {to} is not in the old set"
                );
                // A proposal keeps the reason it was written with, as it
                // always has: the suppression does not rewrite a row. One
                // written by this pass carries the old statement's reason.
                if !before.contains(&pair) {
                    assert_eq!(
                        old_set.get(&pair),
                        Some(reason),
                        "step {n}, batch {batch}: {from} / {to} is proposed for another reason"
                    );
                }
                assert!(
                    kept.contains_key(&(from.clone(), to.clone())),
                    "step {n}, batch {batch}: {from} / {to} is in neither end's ten"
                );
            }
            assert_eq!(
                got, want,
                "step {n}, batch {batch}: the passes disagree with the capped whole corpus"
            );
            assert_eq!(
                answered(&pool).await,
                reader,
                "step {n}, batch {batch}: a pass touched what the reader answered"
            );
        }
        assert!(
            capped_somewhere,
            "batch {batch}: no item ever had more than ten counterparts, so the cap was never \
             witnessed"
        );
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

    // One item (a pr) rewritten: its pairs are judged again, and so are the
    // pairs of the items whose ten it held a place in (#771) -- a place its
    // new text may no longer earn, and the eleventh is not stored -- and
    // nothing else.
    let rankers: BTreeSet<String> = sqlx::query_scalar(
        "select entity_id from sync.similar_text_top where other_id = 'jira:K-2'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    sqlx::query(
        "update sync.item set body_text = body_text || ' payout' where entity_id = 'jira:K-2'",
    )
    .execute(&pool)
    .await
    .unwrap();
    suggest::detect_rule(&pool, rule).await.unwrap();
    let touched = proposed(&pool).await;
    assert!(!touched.is_empty(), "the changed item's pairs were judged");
    let judged = |id: &String| id == "jira:K-2" || rankers.contains(id);
    assert!(
        touched
            .iter()
            .all(|(from, to, _)| judged(from) || judged(to)),
        "only the changed item's pairs and its rankers': {touched:?}"
    );
}

/// #755: a batch that can never finish must not wedge the rule. A stalled
/// batch is halved and the pass gives up; at one item the item is settled
/// without proposals and the pass moves on; a completed pass grows the batch
/// back.
#[tokio::test]
async fn a_batch_that_always_times_out_is_halved_then_its_item_skipped() {
    let pool = scratch().await;
    let rule = suggest::rule("similar_text").unwrap();
    apply(
        &pool,
        &Step::Write {
            source: "jira",
            keys: 0..8,
            seed: 7,
        },
    )
    .await;
    let mut pacing = suggest::Pacing::new(8);
    // Any batch holding K-2 never finishes.
    pacing.stall = Some(Box::new(|ids| ids.iter().any(|id| id == "jira:K-2")));
    let mut conn = pool.acquire().await.unwrap();

    // Six prose items (the branches are not prose). A batch of 6 is halved to
    // 3, then 3 to 1; each pass gives up on the batch and says so.
    assert!(
        suggest::detect_rule_paced(&mut conn, rule, &pacing)
            .await
            .is_err()
    );
    assert_eq!(pacing.batch(), 3);
    assert!(
        suggest::detect_rule_paced(&mut conn, rule, &pacing)
            .await
            .is_err()
    );
    assert_eq!(pacing.batch(), 1);

    // At one item K-2 is skipped, the pass finishes, the rest was compared.
    suggest::detect_rule_paced(&mut conn, rule, &pacing)
        .await
        .expect("the pass advances past the item");
    assert_eq!(pacing.batch(), 2, "a completed pass grows the batch back");
    let seen: Vec<String> =
        sqlx::query_scalar("select entity_id from sync.similar_text_seen order by 1")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        seen.len(),
        6,
        "every item, the skipped one included, is settled"
    );
    let proposals = proposed(&pool).await;
    assert!(!proposals.is_empty());
    // The skipped item's own comparison never ran, so it is not paired with
    // the items settled before it (K-0, K-1); a later item may still find it.
    let earlier = ["jira:K-0", "jira:K-1"];
    assert!(
        proposals.iter().all(|(from, to, _)| {
            let paired = |a: &str, b: &str| a == "jira:K-2" && earlier.contains(&b);
            !paired(from, to) && !paired(to, from)
        }),
        "the skipped item was not compared with its predecessors: {proposals:?}"
    );

    // Nothing is left to compare, and the size recovers to its ceiling.
    for _ in 0..3 {
        suggest::detect_rule_paced(&mut conn, rule, &pacing)
            .await
            .unwrap();
    }
    assert_eq!(pacing.batch(), 8);
}
