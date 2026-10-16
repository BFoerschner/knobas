//! Proposal evidence (#836): why a proposal was made, read beside it.
//!
//! Each test builds a corpus, runs the detector's own rule over it, and reads
//! the evidence for the proposal the rule wrote: what an exact key or shared
//! wording looks like in the text, with the match marked, what supports the
//! proposal and what speaks against it.
//!
//! A database of its own per test, for the reason `suggestions.rs` gives: a
//! detection pass reads the whole mirror.

use knobas_core::CoreError;
use knobas_core::entity::EntityRef;
use knobas_core::evidence::{self, Excerpt, ProposalEvidence};
use knobas_core::suggest;
use sqlx::PgPool;

const SOURCE: &str = "jira";

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("evidence")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One live mirror item, as `suggestions.rs` writes one.
async fn item(pool: &PgPool, kind: &str, key: &str, title: &str, body: &str) -> String {
    let id = EntityRef::new(SOURCE, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,'{}')",
    )
    .bind(&id)
    .bind(SOURCE)
    .bind(kind)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn run_rule(pool: &PgPool, name: &str) {
    let rule = suggest::rule(name).expect("a rule by that name");
    suggest::detect_rule(pool, rule).await.unwrap();
}

/// The proposal between `a` and `b`, either way round.
async fn proposal(pool: &PgPool, a: &str, b: &str) -> uuid::Uuid {
    let (id,): (uuid::Uuid,) = sqlx::query_as(
        "select id from knobas.link
          where (from_id = $1 and to_id = $2) or (from_id = $2 and to_id = $1)",
    )
    .bind(a)
    .bind(b)
    .fetch_one(pool)
    .await
    .expect("a proposal between the two");
    id
}

async fn read(pool: &PgPool, id: uuid::Uuid) -> ProposalEvidence {
    evidence::proposal_evidence(pool, id).await.unwrap()
}

/// The marked text of every excerpt, in order.
fn marked(excerpts: &[Excerpt]) -> Vec<String> {
    excerpts
        .iter()
        .flat_map(|e| e.parts.iter().filter(|p| p.marked).map(|p| p.text.clone()))
        .collect()
}

/// An excerpt as plain text.
fn plain(excerpt: &Excerpt) -> String {
    excerpt.parts.iter().map(|p| p.text.as_str()).collect()
}

#[tokio::test]
async fn a_key_in_a_commit_message_is_quoted_with_the_key_marked() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Retry failed SEPA payouts", "").await;
    item(&pool, "ticket", "PAY-240", "Payout dashboard latency", "").await;
    let commit = item(
        &pool,
        "commit",
        "a41f2c",
        "Backoff jitter",
        "Spread the retries out. Refs PAY-231; see also PAY-240. Not PAY-2311.",
    )
    .await;
    run_rule(&pool, "commit_message_key").await;

    let found = read(&pool, proposal(&pool, &commit, &ticket).await).await;

    assert_eq!(
        found.reason.as_deref(),
        Some("the commit message mentions PAY-231")
    );
    assert_eq!(
        marked(&found.excerpts),
        ["PAY-231"],
        "PAY-2311 is not PAY-231"
    );
    assert_eq!(found.excerpts[0].entity_id, commit);
    assert!(
        plain(&found.excerpts[0]).contains("Refs PAY-231; see also"),
        "the excerpt quotes the text around the key: {:?}",
        plain(&found.excerpts[0])
    );
    assert_eq!(
        found.against,
        ["It also names PAY-240 and PAY-2311."],
        "a text naming other keys is weaker evidence for any one of them"
    );
}

#[tokio::test]
async fn items_found_the_same_way_and_already_linked_support_a_proposal() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Retry failed SEPA payouts", "").await;
    let first = item(&pool, "commit", "c90d11", "PAY-231 retry once", "").await;
    let second = item(&pool, "commit", "7be0e4", "PAY-231 log the retry", "").await;
    let open = item(&pool, "commit", "a41f2c", "PAY-231 backoff jitter", "").await;
    run_rule(&pool, "commit_message_key").await;
    for done in [&first, &second] {
        suggest::accept(&pool, proposal(&pool, done, &ticket).await)
            .await
            .unwrap();
    }

    let found = read(&pool, proposal(&pool, &open, &ticket).await).await;

    assert_eq!(
        found.support,
        [
            "Found the same way and already linked to PAY-231: 7be0e4 “PAY-231 log the retry”, c90d11 “PAY-231 retry once”."
        ]
    );
    assert_eq!(found.against, ["Nothing found against it."]);
}

#[tokio::test]
async fn proposals_dismissed_at_either_end_speak_against_one_found_the_same_way() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Retry failed SEPA payouts", "").await;
    let dismissed = item(&pool, "branch", "b1", "feature/PAY-231-old", "").await;
    let open = item(&pool, "branch", "b2", "feature/PAY-231-new", "").await;
    run_rule(&pool, "branch_name_key").await;
    suggest::dismiss(&pool, proposal(&pool, &dismissed, &ticket).await)
        .await
        .unwrap();

    let found = read(&pool, proposal(&pool, &open, &ticket).await).await;

    assert_eq!(marked(&found.excerpts), ["PAY-231"]);
    assert_eq!(
        found.against,
        ["1 other proposal found the same way at these ends was dismissed."]
    );
}

#[tokio::test]
async fn shared_wording_is_quoted_from_both_ends_with_the_shared_words_marked() {
    let pool = scratch().await;
    let ticket = item(
        &pool,
        "ticket",
        "PAY-261",
        "Payout retry storm floods the ledger",
        "The payout retry storm floods the ledger with duplicate transfers.",
    )
    .await;
    let page = item(
        &pool,
        "page",
        "ENG:Storm",
        "Retry storm runbook",
        "When the payout retry storm floods the ledger, duplicate transfers appear.",
    )
    .await;
    run_rule(&pool, "similar_text").await;

    let found = read(&pool, proposal(&pool, &ticket, &page).await).await;

    let ends: Vec<&str> = found
        .excerpts
        .iter()
        .map(|e| e.entity_id.as_str())
        .collect();
    let (from, to) = if ticket < page {
        (&ticket, &page)
    } else {
        (&page, &ticket)
    };
    assert_eq!(
        ends,
        [from.as_str(), to.as_str()],
        "one excerpt per end, in the link's order"
    );
    let words: std::collections::BTreeSet<String> = marked(&found.excerpts)
        .iter()
        .map(|w| w.to_lowercase())
        .collect();
    for word in ["payout", "retry", "storm", "floods", "ledger"] {
        assert!(
            words.contains(word),
            "{word} is shared and marked: {words:?}"
        );
    }
    assert!(
        !words.contains("runbook"),
        "a word only one end has is not marked"
    );
    let support = found.support.join(" ");
    assert!(
        support.starts_with("Shared words: ") && support.contains("ledger"),
        "{support}"
    );
    assert_eq!(
        found.against,
        ["Wording alone often matches unrelated items."]
    );
}

#[tokio::test]
async fn evidence_for_an_id_no_link_carries_is_not_found() {
    let pool = scratch().await;
    let id = uuid::Uuid::new_v4();
    assert!(matches!(
        evidence::proposal_evidence(&pool, id).await,
        Err(CoreError::LinkNotFound(missing)) if missing == id
    ));
}
