//! The suggestion engine, against a real PostgreSQL.
//!
//! What is asserted here is **which suggestions are proposed and what happens
//! when the user acts** -- never how a detector scanned text. Each rule gets a
//! positive and a negative case in one test, because a rule that fires for
//! everything is as broken as one that never fires and only the negative case
//! catches the first.
//!
//! # Why every test gets a database of its own
//!
//! Detection is a pass over the **whole** mirror, and that is the point of it:
//! a rule reads every branch, not the branches one caller nominated. The
//! binary's shared database therefore cannot isolate these tests the way
//! unique ids isolate the link battery -- test A's `detect` run scans test B's
//! corpus and proposes into it, so B's `detect` then finds its own suggestion
//! already there and *correctly* writes nothing. Every negative assertion in
//! this file would be decided by scheduling.
//!
//! `scratch_database` costs one `create database` per test on the same
//! postmaster -- no second server -- and buys assertions that mean what they
//! say. It is also what lets the fixtures read as `PAY-231` rather than as a
//! uuid.

use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::suggest::{self, RuleClass};
use knobas_core::{CoreError, link};
use sqlx::PgPool;
use uuid::Uuid;

/// The source every fixture below is mirrored under.
const SOURCE: &str = "jira";

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("suggest")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// Put one live mirror item in the corpus, and return its address.
///
/// Both halves, because every rule reads `sync.live_item`, which is the join of
/// the two: an entity row alone is linkable but invisible to detection, which
/// is exactly the state a note or a context is in.
async fn item(pool: &PgPool, kind: &str, key: &str, title: &str, body: &str) -> String {
    from(pool, SOURCE, kind, key, title, body, serde_json::json!({})).await
}

/// As [`item`], with a payload -- what the source-recorded rule reads.
async fn with_payload(
    pool: &PgPool,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    from(pool, SOURCE, kind, key, title, "", payload).await
}

/// An entity that exists but has never been mirrored -- knobas' own kinds, and
/// anything a link may point at that detection cannot see.
async fn entity_only(pool: &PgPool, namespace: &str, kind: &str, key: &str) -> String {
    let id = EntityRef::new(namespace, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// One mirrored item, in the source named.
async fn from(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
) -> String {
    let id = entity_only(pool, source, kind, key).await;
    sqlx::query("update knobas.entity set title = $2 where id = $1")
        .bind(&id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Run one named rule. Panics on a name no rule carries, which is a typo in a
/// test rather than a failure worth a message.
async fn run_rule(pool: &PgPool, name: &str) -> u64 {
    let rule = suggest::rule(name).expect("a rule by that name");
    suggest::detect_rule(pool, rule).await.unwrap()
}

/// Every proposal in the corpus, newest first.
async fn tray(pool: &PgPool) -> Vec<suggest::SuggestionEntry> {
    suggest::proposals(pool, &[], 200).await.unwrap()
}

/// The `(from, to)` pairs the tray holds, as a set.
fn pairs(entries: &[suggest::SuggestionEntry]) -> std::collections::BTreeSet<(String, String)> {
    entries
        .iter()
        .map(|e| (e.link.from_id.clone(), e.link.to_id.clone()))
        .collect()
}

/// The one proposal connecting `a` and `b`, in either direction.
fn between<'e>(
    entries: &'e [suggest::SuggestionEntry],
    a: &str,
    b: &str,
) -> Option<&'e suggest::SuggestionEntry> {
    entries.iter().find(|e| {
        (e.link.from_id == a && e.link.to_id == b) || (e.link.from_id == b && e.link.to_id == a)
    })
}

// -- one test per rule, each with its negative control ----------------------

#[tokio::test]
async fn a_branch_name_proposes_the_ticket_it_names_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let named = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    // Two negative controls, and the second is the one that matters: a branch
    // with no key at all is caught by any rule that reads the right column,
    // but only a *longer* key catches a match with no word boundary.
    let unnamed = item(&pool, "branch", "b2", "feature/retry-storm", "").await;
    let longer = item(&pool, "branch", "b3", "feature/PAY-2311-other", "").await;

    run_rule(&pool, "branch_name_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(named.clone(), ticket.clone())].into_iter().collect(),
        "only the branch that names PAY-231 proposes it"
    );
    assert!(between(&entries, &unnamed, &ticket).is_none());
    assert!(
        between(&entries, &longer, &ticket).is_none(),
        "PAY-2311 is not PAY-231"
    );

    let proposal = &entries[0];
    assert_eq!(
        proposal.link.reason.as_deref(),
        Some("the branch name contains PAY-231"),
        "the reason names the key, not the rule"
    );
    assert_eq!(proposal.link.rule.as_deref(), Some("branch_name_key"));
    assert_eq!(proposal.link.rule_class, Some(RuleClass::ExactKey));
    assert_eq!(proposal.link.origin, Origin::Suggested);
    assert_eq!(proposal.link.confirmed_at, None);
    assert_eq!(proposal.from.entity_id, named);
    assert_eq!(proposal.to.title, "Payout retry storm");
}

#[tokio::test]
async fn a_commit_message_proposes_the_ticket_it_mentions_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    // The key is in the trailer, not the subject: a rule reading only the title
    // would pass a test written the other way round.
    let names = item(
        &pool,
        "commit",
        "c1",
        "Retry the payout once",
        "Refs PAY-231, and nothing else.",
    )
    .await;
    let silent = item(
        &pool,
        "commit",
        "c2",
        "Retry the payout once",
        "No ticket was harmed.",
    )
    .await;

    run_rule(&pool, "commit_message_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(names.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &silent, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("the commit message mentions PAY-231")
    );
    assert_eq!(entries[0].link.rule_class, Some(RuleClass::ExactKey));
}

#[tokio::test]
async fn a_builds_parameters_propose_the_ticket_they_name_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    // What an adapter puts in a build's `body_text`: the branch it ran on and
    // its status text.
    let onto = item(
        &pool,
        "build",
        "b1",
        "Payments :: Verify #4821",
        "SUCCESS\nrefs/heads/feature/PAY-231-retry",
    )
    .await;
    let elsewhere = item(
        &pool,
        "build",
        "b2",
        "Payments :: Verify #4822",
        "SUCCESS\nrefs/heads/main",
    )
    .await;

    run_rule(&pool, "build_parameter_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(onto.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &elsewhere, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("this build's parameters name PAY-231")
    );
}

#[tokio::test]
async fn a_page_proposes_the_ticket_its_text_mentions_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let documents = item(
        &pool,
        "page",
        "ENG:Retries",
        "Retry policy",
        "The storm behaviour is tracked in PAY-231.",
    )
    .await;
    let unrelated = item(
        &pool,
        "page",
        "ENG:Onboarding",
        "Onboarding",
        "Where the coffee is.",
    )
    .await;

    run_rule(&pool, "page_text_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(documents.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &unrelated, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("this page mentions PAY-231")
    );
}

/// A key found by the wrong rule is not found at all.
///
/// The four exact-key rules are one mechanism pointed at four kinds, and the
/// cheapest way for that to go wrong is a `where kind = ...` that drifts. Every
/// kind is present, every one of them names PAY-231, and each rule must claim
/// exactly its own.
#[tokio::test]
async fn each_exact_key_rule_reads_only_its_own_kind() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231", "").await;
    let commit = item(&pool, "commit", "c1", "Fix PAY-231", "").await;
    let build = item(&pool, "build", "d1", "Verify #1", "PAY-231").await;
    let page = item(&pool, "page", "p1", "Notes", "PAY-231").await;

    for (rule, expected) in [
        ("branch_name_key", &branch),
        ("commit_message_key", &commit),
        ("build_parameter_key", &build),
        ("page_text_key", &page),
    ] {
        let before = pairs(&tray(&pool).await);
        run_rule(&pool, rule).await;
        let after = pairs(&tray(&pool).await);
        let fresh: Vec<(String, String)> = after.difference(&before).cloned().collect();
        assert_eq!(
            fresh,
            vec![(expected.clone(), ticket.clone())],
            "{rule} proposed something that is not its kind"
        );
    }
}

#[tokio::test]
async fn a_relation_the_source_records_becomes_a_proposal_with_the_sources_own_wording() {
    let pool = scratch().await;
    let blocker = with_payload(
        &pool,
        "ticket",
        "PAY-231",
        "Payout retry storm",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
             "outwardIssue": {"key": "PAY-232"}}
        ]}}),
    )
    .await;
    let blocked = item(&pool, "ticket", "PAY-232", "Ledger drift", "").await;
    // Negative control: an issue whose payload records no relation at all.
    let alone = item(&pool, "ticket", "PAY-233", "Unrelated", "").await;

    run_rule(&pool, "source_recorded_relation").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(blocker.clone(), blocked.clone())].into_iter().collect(),
        "the direction follows the outward issue"
    );
    assert!(between(&entries, &alone, &blocker).is_none());

    let proposal = &entries[0];
    assert_eq!(proposal.link.relation, "blocks");
    assert_eq!(
        proposal.link.reason.as_deref(),
        Some("jira already records this link (Blocks)")
    );
    assert_eq!(proposal.link.rule_class, Some(RuleClass::SourceRelation));
    assert_eq!(
        proposal.link.origin,
        Origin::Source,
        "a relation the source states came from the source, however it is confirmed"
    );
    assert_eq!(
        proposal.link.confirmed_at, None,
        "evidence is still not consent: nothing enters the graph unconfirmed"
    );
}

/// The inward side of a Jira link is the same relation read backwards, and it
/// must produce the same edge rather than a second one spelled the other way.
#[tokio::test]
async fn an_inward_source_relation_is_stored_as_the_outward_phrase() {
    let pool = scratch().await;
    let blocked = with_payload(
        &pool,
        "ticket",
        "PAY-241",
        "Ledger drift",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
             "inwardIssue": {"key": "PAY-240"}}
        ]}}),
    )
    .await;
    let blocker = item(&pool, "ticket", "PAY-240", "Retry storm", "").await;

    run_rule(&pool, "source_recorded_relation").await;

    let entries = tray(&pool).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].link.from_id, blocker,
        "the blocker is the from end"
    );
    assert_eq!(entries[0].link.to_id, blocked);
    assert_eq!(
        entries[0].link.relation, "blocks",
        "one relationship, one spelling -- never the inward phrase as a second relation"
    );
}

/// The other end of a source-recorded relation is addressed in the **same**
/// source, so two Jiras cannot cross-link by bare key.
#[tokio::test]
async fn a_source_relation_never_reaches_into_another_source() {
    let pool = scratch().await;
    let issuer = with_payload(
        &pool,
        "ticket",
        "PAY-251",
        "Ours",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "outward": "blocks"},
             "outwardIssue": {"key": "PAY-252"}}
        ]}}),
    )
    .await;
    // The same bare key in a different namespace: a different ticket in a
    // different system that happens to be numbered alike.
    let impostor = from(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-252",
        "Theirs",
        "",
        serde_json::json!({}),
    )
    .await;

    run_rule(&pool, "source_recorded_relation").await;

    assert!(
        between(&tray(&pool).await, &issuer, &impostor).is_none(),
        "PAY-252 in another Jira is not the PAY-252 this issue links to"
    );
}

#[tokio::test]
async fn text_that_reads_alike_is_proposed_and_labelled_as_a_guess() {
    let pool = scratch().await;
    let incident = item(
        &pool,
        "ticket",
        "PAY-261",
        "Payout retry storm floods the ledger",
        "The payout retry storm floods the ledger with duplicate transfers.",
    )
    .await;
    let echo = item(
        &pool,
        "page",
        "ENG:Storm",
        "Retry storm runbook",
        "When the payout retry storm floods the ledger, duplicate transfers appear.",
    )
    .await;
    // Negative control: prose with nothing in common. Same source, same
    // corpus, same pass -- only the words differ.
    let unrelated = item(
        &pool,
        "page",
        "ENG:Coffee",
        "Kitchen rota",
        "Whoever finishes the milk buys the milk.",
    )
    .await;

    run_rule(&pool, "similar_text").await;

    let entries = tray(&pool).await;
    let found = between(&entries, &incident, &echo).expect("the two documents read alike");
    assert_eq!(found.link.rule_class, Some(RuleClass::Similarity));
    assert_eq!(found.link.rule.as_deref(), Some("similar_text"));
    let reason = found.link.reason.clone().unwrap();
    assert!(
        reason.starts_with("both mention "),
        "a similarity reason names the overlap: {reason}"
    );
    assert!(
        reason.contains("payout") || reason.contains("ledger") || reason.contains("storm"),
        "the reason must be specific rather than 'these look related': {reason}"
    );

    assert!(
        between(&entries, &incident, &unrelated).is_none(),
        "a rule that fires for everything is as broken as one that never fires"
    );
    assert!(between(&entries, &echo, &unrelated).is_none());
}

/// Two documents that merely share a couple of words are not similar.
///
/// The floor is the whole of this rule's judgement, and a floor of one would
/// pass every assertion in the test above.
#[tokio::test]
async fn a_couple_of_shared_words_is_below_the_similarity_floor() {
    let pool = scratch().await;
    let one = item(&pool, "ticket", "PAY-271", "Payout ledger", "").await;
    let two = item(&pool, "page", "ENG:Two", "Payout ledger", "").await;

    run_rule(&pool, "similar_text").await;

    assert!(
        between(&tray(&pool).await, &one, &two).is_none(),
        "two shared stems is under the floor of {}",
        suggest::SIMILARITY_FLOOR
    );
}
