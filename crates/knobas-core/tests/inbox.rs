//! The inbox, against a real PostgreSQL.
//!
//! What is asserted here is **what appears in the stream and what happens when
//! the user answers it** -- never how an item was assembled. Each detection
//! rule gets a positive and a negative case in one test, because a rule that
//! fires for everything is as broken as one that never fires and only the
//! negative case catches the first.
//!
//! # Why every test gets a database of its own
//!
//! The derivation is a pass over the **whole** mirror -- that is the point of
//! it -- so the binary's shared database cannot isolate these tests the way
//! unique ids isolate the link battery: test A's stream would contain test B's
//! fixtures, and every "and nothing else is in the stream" assertion in this
//! file would be decided by scheduling. `scratch_database` costs one `create
//! database` per test on the same postmaster, and buys assertions that mean
//! what they say. It is the same reasoning `tests/suggestions.rs` records.
//!
//! # Why the clock is a fixture
//!
//! Every read takes `now`. A snooze test that read the wall clock would be a
//! coin flip; here "before its date" and "after its date" are two calls with
//! two clocks and nothing sleeps.

use chrono::{DateTime, Duration, TimeZone, Utc};
use knobas_core::entity::EntityRef;
use knobas_core::inbox::{self, Category, Shelf};
use sqlx::PgPool;

/// The accounts this reader is known by -- what `@me` resolves to.
const ME: &str = "mara.lindqvist";

/// Somebody else, for every negative control.
const THEM: &str = "jonas.becker";

fn me() -> Vec<String> {
    vec![ME.to_owned()]
}

/// A fixed clock. Every fixture is dated relative to this, so a test says
/// "eight days ago" rather than doing arithmetic on the wall clock.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
}

fn days_ago(n: i64) -> DateTime<Utc> {
    now() - Duration::days(n)
}

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("inbox")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One mirrored row, as a fixture writes it.
///
/// A struct rather than eight positional arguments, because the three shapes
/// below build it field by field and a test reading `Some(THEM), "", payload`
/// cannot say which of those is the author.
struct Mirrored<'a> {
    source: &'a str,
    kind: &'a str,
    key: &'a str,
    author: Option<&'a str>,
    body: &'a str,
    payload: serde_json::Value,
    updated: DateTime<Utc>,
}

/// One live mirror item: both halves, because every rule reads
/// What the sources in this file declare about where they keep a requested
/// reviewer and an assignee (#277) -- both rules read through the declaration
/// now, so a fixture's source has to have said where its own spellings are,
/// the way its adapter's descriptor does.
///
/// Gitea's `requested_reviewers[].login` and Jira Data Center's
/// `fields.assignee.name`, which are the shapes the fixtures below are written
/// in. `teamcity` declares nothing: a build has neither, and the failed-build
/// rule reads an outcome rather than a declared field.
fn declarations() -> knobas_core::payload::Declarations {
    use knobas_core::payload::{Declarations, KindPaths, ListPath, PayloadPath};
    Declarations::empty()
        .with(
            "gitea",
            vec![KindPaths {
                kind: "pr".to_owned(),
                reviewers: vec![ListPath {
                    at: PayloadPath::of(["requested_reviewers"]),
                    entry: PayloadPath::of(["login"]),
                }],
                ..KindPaths::default()
            }],
        )
        .with(
            "jira",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                assignee: vec![
                    PayloadPath::of(["fields", "assignee", "name"]),
                    PayloadPath::of(["fields", "assignee", "key"]),
                ],
                ..KindPaths::default()
            }],
        )
}

/// `sync.live_item`, which is the join of the two.
async fn item(pool: &PgPool, row: Mirrored<'_>) -> String {
    let id = EntityRef::new(row.source, row.key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(row.kind)
        .bind(row.key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
             (entity_id, source_id, kind, title, body_text, author, item_updated_at, payload)
         values ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&id)
    .bind(row.source)
    .bind(row.kind)
    .bind(row.key)
    .bind(row.body)
    .bind(row.author)
    .bind(row.updated)
    .bind(row.payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A pull request with a reviewer list.
async fn pull_request(pool: &PgPool, key: &str, reviewers: &[&str], state: &str) -> String {
    item(
        pool,
        Mirrored {
            source: "gitea",
            kind: "pr",
            key,
            author: Some(THEM),
            body: "",
            payload: serde_json::json!({
                "state": state,
                "requested_reviewers": reviewers.iter()
                    .map(|r| serde_json::json!({"login": r}))
                    .collect::<Vec<_>>(),
            }),
            updated: days_ago(1),
        },
    )
    .await
}

/// A TeamCity-shaped build record.
async fn build(
    pool: &PgPool,
    key: &str,
    status: &str,
    config: &str,
    triggered_by: Option<&str>,
    updated: DateTime<Utc>,
) -> String {
    item(
        pool,
        Mirrored {
            source: "teamcity",
            kind: "build",
            key,
            author: triggered_by,
            body: "",
            payload: serde_json::json!({
                "status": status,
                "state": "finished",
                "buildTypeId": config,
                "statusText": "3 tests failed",
            }),
            updated,
        },
    )
    .await
}

/// A Jira-shaped ticket.
async fn ticket(
    pool: &PgPool,
    key: &str,
    reporter: Option<&str>,
    assignee: Option<&str>,
    body: &str,
    updated: DateTime<Utc>,
) -> String {
    item(
        pool,
        Mirrored {
            source: "jira",
            kind: "ticket",
            key,
            author: reporter,
            body,
            payload: match assignee {
                Some(name) => serde_json::json!({ "fields": { "assignee": { "name": name } } }),
                None => serde_json::json!({ "fields": { "assignee": serde_json::Value::Null } }),
            },
            updated,
        },
    )
    .await
}

/// A configured source, with an optional credential expiry.
async fn source(pool: &PgPool, id: &str, expires: Option<DateTime<Utc>>, enabled: bool) {
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, secret_expires_at, enabled)
         values ($1,$1,$1,'http://127.0.0.1:1','pat',$2,$3)",
    )
    .bind(id)
    .bind(expires)
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
}

/// A link between two entities, confirmed or merely proposed.
async fn link(pool: &PgPool, from: &str, to: &str, confirmed: bool) {
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by, confirmed_at,
              rule, rule_class, reason)
         values ($1,$2,'related','manual','user',
                 case when $3 then now() end,
                 case when $3 then null else 'branch_name_key' end,
                 case when $3 then null else 'exact_key' end,
                 case when $3 then null else 'a reason' end)",
    )
    .bind(from)
    .bind(to)
    .bind(confirmed)
    .execute(pool)
    .await
    .unwrap();
}

/// The stream, as the reader sees it.
async fn stream(pool: &PgPool) -> Vec<inbox::InboxItem> {
    inbox::items(pool, &me(), now(), Shelf::Stream, &declarations())
        .await
        .unwrap()
}

/// The stream at another moment -- what a snooze test moves.
async fn stream_at(pool: &PgPool, at: DateTime<Utc>) -> Vec<inbox::InboxItem> {
    inbox::items(pool, &me(), at, Shelf::Stream, &declarations())
        .await
        .unwrap()
}

/// Every key on the stream, so an assertion can read as a set.
fn keys(items: &[inbox::InboxItem]) -> Vec<String> {
    items.iter().map(|i| i.key.clone()).collect()
}

// -- the five detection rules, each with its negative control ---------------

/// Story 2. The positive is a review requested from one of my accounts; the
/// negative is the same pull request asking somebody else, which is the shape
/// a rule that fired on "has any reviewer at all" would also return.
#[tokio::test]
async fn a_review_request_names_me_and_not_merely_somebody() {
    let pool = &scratch().await;
    let mine = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    let theirs = pull_request(pool, "acme/payouts#145", &[THEM], "open").await;
    let nobody = pull_request(pool, "acme/payouts#146", &[], "open").await;

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("review_request:{mine}")),
        "a review asked of me is in the stream: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("review_request:{theirs}")),
        "a review asked of somebody else is not mine to answer: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("review_request:{nobody}")),
        "a pull request nobody was asked about is not a review request: {keys:?}"
    );
}

/// Story 17, for the category where the source resolves it by editing the
/// record: a closed pull request is not still waiting on a review.
#[tokio::test]
async fn a_review_request_on_a_closed_pull_request_has_left_the_stream() {
    let pool = &scratch().await;
    let open = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    let closed = pull_request(pool, "acme/payouts#140", &[ME], "closed").await;

    let keys = keys(&stream(pool).await);
    assert!(keys.contains(&format!("review_request:{open}")));
    assert!(
        !keys.contains(&format!("review_request:{closed}")),
        "the inbox must not accumulate work that no longer exists: {keys:?}"
    );
}

/// Story 3, and the rule with the sharpest negative control: *what counts as a
/// mention*.
///
/// A mention is the account name behind `@` or `[~`. The bare name in prose is
/// not one -- a ticket that merely says who wrote a library is not a question
/// addressed to them -- and neither is a longer name this one is a prefix of,
/// which is the failure that would put somebody else's mentions in my inbox.
#[tokio::test]
async fn a_mention_is_the_marked_name_and_not_every_appearance_of_it() {
    let pool = &scratch().await;
    let at = ticket(
        pool,
        "PAY-1",
        Some(THEM),
        None,
        "@mara.lindqvist ping?",
        days_ago(1),
    )
    .await;
    let jira = ticket(
        pool,
        "PAY-2",
        Some(THEM),
        None,
        "asked [~mara.lindqvist] to look",
        days_ago(1),
    )
    .await;
    let prose = ticket(
        pool,
        "PAY-3",
        Some(THEM),
        None,
        "mara.lindqvist wrote the retry code",
        days_ago(1),
    )
    .await;
    let other = ticket(
        pool,
        "PAY-4",
        Some(ME),
        None,
        "@jonas.becker ping?",
        days_ago(1),
    )
    .await;

    let keys = keys(&stream(pool).await);
    assert!(keys.contains(&format!("mention:{at}")), "{keys:?}");
    assert!(keys.contains(&format!("mention:{jira}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("mention:{prose}")),
        "being named in prose is not being asked a question: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("mention:{other}")),
        "somebody else's mention is not mine: {keys:?}"
    );
}

/// **A Confluence page is a mention on the same rule** (#287) -- the seam
/// between the adapter's rendering and this rule, asserted from this side.
///
/// A Confluence mention is not text at all: the storage format holds
/// `<ac:link><ri:user ri:userkey="…"/></ac:link>`, a *key*, and a body_text
/// built by stripping tags carries no trace of it. `knobas-source-confluence`
/// resolves that key against the account the credential is and renders it back
/// to `@name`, and the body below is a **transcription** of what its
/// `storage::to_text` produces.
///
/// A transcription is not a witness, and this test does not pretend to be one:
/// nothing here reads the adapter, so a renderer that changed its spelling
/// would leave this green. What it pins is this rule's **end** of the
/// agreement -- that a `page` from a Confluence carrying that spelling is a
/// mention, and that the spelling is what does it. The witness that the two
/// halves meet is `knobas-app/tests/atlassian_live.rs`'s
/// `a_comment_that_mentions_me_becomes_an_inbox_mention`, against the real
/// server (ADR-0013).
///
/// The kind is `page` and the source is a Confluence, both of which the rule
/// already admitted; nothing in `knobas-core` changed for this ticket, which
/// is the point.
#[tokio::test]
async fn a_confluence_page_whose_comment_names_me_is_a_mention_on_the_same_rule() {
    let pool = &scratch().await;
    let mentioning = item(
        pool,
        Mirrored {
            source: "confluence",
            kind: "page",
            key: "98307",
            author: Some(THEM),
            // What `storage::to_text` renders a key-shaped user link as.
            body: "SEPA payout retry design

Backoff policy

@mara.lindqvist can you add                    the SLA?",
            payload: serde_json::json!({ "space": { "key": "ENG" } }),
            updated: days_ago(1),
        },
    )
    .await;
    // The same page with the link left as markup: what the mirror held before
    // the adapter learned to resolve a key, and what the rule cannot see. This
    // is the negative control that says the *rendering* is load-bearing.
    let unrendered = item(
        pool,
        Mirrored {
            source: "confluence",
            kind: "page",
            key: "98311",
            author: Some(THEM),
            body: "Ledger reconciliation runbook

can you add the SLA?",
            payload: serde_json::json!({ "space": { "key": "ENG" } }),
            updated: days_ago(1),
        },
    )
    .await;

    let keys = keys(&stream(pool).await);
    assert!(keys.contains(&format!("mention:{mentioning}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("mention:{unrendered}")),
        "a body with the mention markup stripped out names nobody: {keys:?}"
    );
}

/// The prefix case, on its own because it is the one a reader will doubt: an
/// account called `mara` must not collect every `@mara.lindqvist`.
#[tokio::test]
async fn a_mention_of_a_longer_name_is_not_a_mention_of_its_prefix() {
    let pool = &scratch().await;
    let longer = ticket(
        pool,
        "PAY-1",
        Some(THEM),
        None,
        "@mara.lindqvist ping?",
        days_ago(1),
    )
    .await;
    let exact = ticket(pool, "PAY-2", Some(THEM), None, "@mara ping?", days_ago(1)).await;

    let items = inbox::items(
        pool,
        &["mara".to_owned()],
        now(),
        Shelf::Stream,
        &declarations(),
    )
    .await
    .unwrap();
    let keys = keys(&items);
    assert!(keys.contains(&format!("mention:{exact}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("mention:{longer}")),
        "@mara.lindqvist is not a mention of mara: {keys:?}"
    );
}

/// Story 4, first arm of "on my work": a build I triggered. The negative
/// control is the same configuration going green, which is what a rule keyed
/// on "is a build" would also return.
#[tokio::test]
async fn a_failed_build_i_triggered_is_mine_and_a_green_one_is_nobodys() {
    let pool = &scratch().await;
    let red = build(
        pool,
        "build:1187",
        "FAILURE",
        "Payout_Tests",
        Some(ME),
        days_ago(1),
    )
    .await;
    let green = build(
        pool,
        "build:1188",
        "SUCCESS",
        "Payout_Lint",
        Some(ME),
        days_ago(1),
    )
    .await;
    let theirs = build(
        pool,
        "build:1189",
        "FAILURE",
        "Ledger_Tests",
        Some(THEM),
        days_ago(1),
    )
    .await;

    let keys = keys(&stream(pool).await);
    assert!(keys.contains(&format!("failed_build:{red}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("failed_build:{green}")),
        "a build that passed is not a demand: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("failed_build:{theirs}")),
        "somebody else's red build is not on my work: {keys:?}"
    );
}

/// Story 4, second arm: a build CI triggered on my pull request finds me
/// through the link graph -- and **only** through a confirmed link.
///
/// A proposed link is a guess nobody has accepted. An inbox built on guesses
/// is one the reader stops trusting, and reading `knobas.link` rather than
/// `knobas.confirmed_link` is exactly the mistake #161 records elsewhere in
/// this codebase.
#[tokio::test]
async fn a_failed_build_reaches_me_through_a_confirmed_link_and_not_through_a_proposal() {
    let pool = &scratch().await;
    let mine = ticket(pool, "PAY-231", Some(ME), None, "", days_ago(2)).await;
    let guess = ticket(pool, "PAY-999", Some(ME), None, "", days_ago(2)).await;

    let confirmed = build(pool, "build:1187", "FAILURE", "A", None, days_ago(1)).await;
    let proposed = build(pool, "build:1188", "FAILURE", "B", None, days_ago(1)).await;
    link(pool, &confirmed, &mine, true).await;
    link(pool, &proposed, &guess, false).await;

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("failed_build:{confirmed}")),
        "a red build on something I wrote is on my work: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("failed_build:{proposed}")),
        "an unconfirmed guess must not put work in my inbox: {keys:?}"
    );
}

/// Story 17, for the category the source can never resolve on its own: a build
/// record is immutable once finished, so what clears a red build is a **later
/// green build of the same configuration**.
#[tokio::test]
async fn a_failed_build_leaves_when_the_same_configuration_goes_green_again() {
    let pool = &scratch().await;
    let fixed = build(
        pool,
        "build:1187",
        "FAILURE",
        "Payout_Tests",
        Some(ME),
        days_ago(2),
    )
    .await;
    let still = build(
        pool,
        "build:1180",
        "FAILURE",
        "Ledger_Tests",
        Some(ME),
        days_ago(2),
    )
    .await;
    build(
        pool,
        "build:1190",
        "SUCCESS",
        "Payout_Tests",
        Some(ME),
        days_ago(1),
    )
    .await;

    // The negative control's negative control: a re-run that is merely
    // *running* has not resolved anything. The mirror holds queued and running
    // builds too, and TeamCity gives a running build an interim `status` --
    // green *so far* -- so without the finished guard the red build would
    // vanish the moment its re-run started and flicker back if it failed.
    let inflight = build(
        pool,
        "build:1178",
        "FAILURE",
        "Fx_Tests",
        Some(ME),
        days_ago(2),
    )
    .await;
    item(
        pool,
        Mirrored {
            source: "teamcity",
            kind: "build",
            key: "build:1195",
            author: Some(ME),
            body: "",
            payload: serde_json::json!({
                "status": "SUCCESS",
                "state": "running",
                "buildTypeId": "Fx_Tests",
            }),
            updated: days_ago(0),
        },
    )
    .await;

    let keys = keys(&stream(pool).await);
    assert!(
        !keys.contains(&format!("failed_build:{fixed}")),
        "a configuration that has since gone green is not still asking: {keys:?}"
    );
    assert!(
        keys.contains(&format!("failed_build:{still}")),
        "a different configuration is still red: {keys:?}"
    );
    assert!(
        keys.contains(&format!("failed_build:{inflight}")),
        "a re-run that is still running has not resolved its red build: {keys:?}"
    );
}

/// Story 5, and *which assignment is new*: assigned to me, not raised by me,
/// and inside the window. All three negatives are here because each is a
/// different way the rule could fire for everything.
#[tokio::test]
async fn a_ticket_assigned_to_me_arrives_unless_i_raised_it_or_it_is_old() {
    let pool = &scratch().await;
    let arrived = ticket(pool, "PAY-240", Some(THEM), Some(ME), "", days_ago(1)).await;
    let my_own = ticket(pool, "PAY-241", Some(ME), Some(ME), "", days_ago(1)).await;
    let theirs = ticket(pool, "PAY-242", Some(THEM), Some(THEM), "", days_ago(1)).await;
    let ancient = ticket(pool, "PAY-100", Some(THEM), Some(ME), "", days_ago(90)).await;

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("new_assignment:{arrived}")),
        "{keys:?}"
    );
    assert!(
        !keys.contains(&format!("new_assignment:{my_own}")),
        "assigning my own ticket to myself is not news: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("new_assignment:{theirs}")),
        "somebody else's ticket is not my assignment: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("new_assignment:{ancient}")),
        "a ticket nothing has touched in three months did not just arrive: {keys:?}"
    );
}

/// The miss direction of the growth (#277), for both declared rules: a source
/// that has not said where its assignee or its requested reviewers live
/// produces **no** items of those categories, however Jira- or Gitea-shaped
/// its records happen to be.
///
/// This is the clause that makes "a kind that declares no path is a miss,
/// never a guess" visible from the outside. The two fixtures below are the
/// same rows the two positive tests above use -- an assignment and a review
/// request that both fire -- read against a declaration that names neither, so
/// what changes between the two answers is the declaration and nothing else.
/// A reader that fell back on a shape it recognised would keep producing them.
#[tokio::test]
async fn a_source_that_declares_no_paths_produces_no_assignments_and_no_review_requests() {
    let pool = &scratch().await;
    let assigned = ticket(pool, "PAY-250", Some(THEM), Some(ME), "", days_ago(1)).await;
    let review = pull_request(pool, "payout-service#7", &[ME], "open").await;

    let declared = keys(
        &inbox::items(pool, &me(), now(), Shelf::Stream, &declarations())
            .await
            .unwrap(),
    );
    assert!(
        declared.contains(&format!("new_assignment:{assigned}"))
            && declared.contains(&format!("review_request:{review}")),
        "the fixture has to produce both, or the silence below proves nothing: {declared:?}"
    );

    let silent = keys(
        &inbox::items(
            pool,
            &me(),
            now(),
            Shelf::Stream,
            &knobas_core::payload::Declarations::empty(),
        )
        .await
        .unwrap(),
    );
    assert!(
        !silent
            .iter()
            .any(|key| key.starts_with("new_assignment:") || key.starts_with("review_request:")),
        "a source that declares nothing has no assignee and no reviewers to read, whatever its \
         records look like: {silent:?}"
    );
}

/// An assignee at a path the declaration does not reach contributes nothing --
/// the failure direction ADR-0007 requires pinned, now that the path is the
/// source's to name. Jira Data Center's own second candidate, `assignee.key`,
/// is the control: it is declared, so it fires, which is what makes the first
/// half a statement about the *path* rather than about the shape.
#[tokio::test]
async fn an_assignee_the_declaration_does_not_reach_contributes_nothing() {
    let pool = &scratch().await;
    let elsewhere = item(
        pool,
        Mirrored {
            source: "jira",
            kind: "ticket",
            key: "PAY-260",
            author: Some(THEM),
            body: "",
            payload: serde_json::json!({ "fields": { "assignee": { "displayName": ME } } }),
            updated: days_ago(1),
        },
    )
    .await;
    let by_key = item(
        pool,
        Mirrored {
            source: "jira",
            kind: "ticket",
            key: "PAY-261",
            author: Some(THEM),
            body: "",
            payload: serde_json::json!({ "fields": { "assignee": { "key": ME } } }),
            updated: days_ago(1),
        },
    )
    .await;

    let keys = keys(&stream(pool).await);
    assert!(
        !keys.contains(&format!("new_assignment:{elsewhere}")),
        "an account spelled at a path nothing declares is not an assignment: {keys:?}"
    );
    assert!(
        keys.contains(&format!("new_assignment:{by_key}")),
        "the declaration's second candidate is a path, so it fires: {keys:?}"
    );
}

/// Story 6. The negative controls are an expiry far enough out that it is not
/// yet news, and a source the user switched off.
#[tokio::test]
async fn a_credential_expiring_soon_is_in_the_stream_and_a_distant_one_is_not() {
    let pool = &scratch().await;
    source(pool, "jira", Some(now() + Duration::days(9)), true).await;
    source(pool, "gitea", Some(now() + Duration::days(60)), true).await;
    source(pool, "teamcity", Some(now() + Duration::days(3)), false).await;
    source(pool, "mock", None, true).await;

    let keys = keys(&stream(pool).await);
    assert_eq!(
        keys,
        vec!["credential_expiry:jira".to_owned()],
        "only a credential expiring inside the window, on a source that is on"
    );
}

// -- what the user does about an item ---------------------------------------

/// Stories 13 and 15: a snoozed item is absent before its date and back after
/// it. Snoozing is deferral, not deletion -- so it is on the snoozed shelf in
/// the meantime, with the date it returns on.
#[tokio::test]
async fn a_snoozed_item_is_absent_before_its_date_and_present_after() {
    let pool = &scratch().await;
    let pr = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    let key = format!("review_request:{pr}");

    let until = now() + Duration::days(2);
    inbox::snooze(pool, &key, until).await.unwrap();

    assert!(
        !keys(&stream(pool).await).contains(&key),
        "a snoozed item is not on the stream"
    );
    let shelf = inbox::items(pool, &me(), now(), Shelf::Snoozed, &declarations())
        .await
        .unwrap();
    assert_eq!(
        keys(&shelf),
        vec![key.clone()],
        "it is deferred, not deleted"
    );
    assert_eq!(
        shelf[0].snoozed_until,
        Some(until),
        "the shelf says when it comes back"
    );

    assert!(
        keys(&stream_at(pool, until + Duration::minutes(1)).await).contains(&key),
        "it returns on its date without anybody doing anything"
    );
}

/// Story 20: the answer survives a restart. A second pool onto the same
/// database is what a restart is from this module's side -- the state is in
/// the database and nowhere else.
#[tokio::test]
async fn a_snooze_survives_the_database_being_reopened() {
    let db = knobas_db::test_util::scratch_database("inbox-restart").await;
    let pool = db.pool(2).await.unwrap();
    let pr = pull_request(&pool, "acme/payouts#144", &[ME], "open").await;
    let key = format!("review_request:{pr}");
    inbox::snooze(&pool, &key, now() + Duration::days(2))
        .await
        .unwrap();
    pool.close().await;

    let reopened = db.pool(2).await.unwrap();
    assert!(
        !keys(&stream(&reopened).await).contains(&key),
        "a snooze that did not survive the restart is not a snooze"
    );
}

/// Story 19, and the one the spec names as load-bearing: **the count excludes
/// snoozed items**, because the number means "needs me now".
#[tokio::test]
async fn the_count_excludes_snoozed_items() {
    let pool = &scratch().await;
    let a = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    pull_request(pool, "acme/payouts#145", &[ME], "open").await;
    assert_eq!(
        inbox::count(pool, &me(), now(), &declarations())
            .await
            .unwrap(),
        2
    );

    inbox::snooze(
        pool,
        &format!("review_request:{a}"),
        now() + Duration::days(1),
    )
    .await
    .unwrap();
    assert_eq!(
        inbox::count(pool, &me(), now(), &declarations())
            .await
            .unwrap(),
        1,
        "a snoozed item is not something that needs me now"
    );
    assert_eq!(
        inbox::count(pool, &me(), now(), &declarations())
            .await
            .unwrap() as usize,
        stream(pool).await.len(),
        "the count and the stream are one predicate, so they cannot disagree"
    );
}

/// Story 16, and the reason *done* is a timestamp: an item I handled leaves,
/// and comes back when its subject moves again. Anything else would make
/// *done* mean "mute this for ever", which is the one thing an inbox must not
/// quietly do.
#[tokio::test]
async fn an_item_marked_done_leaves_and_returns_when_its_subject_moves() {
    let pool = &scratch().await;
    let id = ticket(
        pool,
        "PAY-1",
        Some(THEM),
        None,
        "@mara.lindqvist ping?",
        days_ago(2),
    )
    .await;
    let key = format!("mention:{id}");
    assert!(keys(&stream(pool).await).contains(&key));

    inbox::complete(pool, &key, now()).await.unwrap();
    assert!(
        !keys(&stream(pool).await).contains(&key),
        "a handled item leaves the stream"
    );

    sqlx::query("update sync.item set item_updated_at = $2 where entity_id = $1")
        .bind(&id)
        .bind(now() + Duration::hours(1))
        .execute(pool)
        .await
        .unwrap();
    assert!(
        keys(&stream_at(pool, now() + Duration::hours(2)).await).contains(&key),
        "somebody said something new: that is a fresh demand, not a handled one"
    );
}

/// Story 17 in its plainest form: the entity is withdrawn upstream and the
/// item leaves on the next read, with nobody acting.
#[tokio::test]
async fn an_item_leaves_when_its_entity_is_tombstoned_at_the_source() {
    let pool = &scratch().await;
    let pr = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    assert!(keys(&stream(pool).await).contains(&format!("review_request:{pr}")));

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&pr)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        stream(pool).await.is_empty(),
        "the inbox does not accumulate work that no longer exists"
    );
}

/// Story 24: one broken source does not empty the stream. The derivation reads
/// the mirror rather than the sources, so a rejected credential changes
/// nothing about what is already known -- including that source's own items.
#[tokio::test]
async fn a_source_whose_credential_is_rejected_does_not_empty_the_stream() {
    let pool = &scratch().await;
    source(pool, "gitea", None, true).await;
    let pr = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    let assigned = ticket(pool, "PAY-240", Some(THEM), Some(ME), "", days_ago(1)).await;

    sqlx::query("update knobas.source_config set auth_state = 'unauthorized' where id = 'gitea'")
        .execute(pool)
        .await
        .unwrap();

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("review_request:{pr}")),
        "the broken source's last-known items are still what knobas knows: {keys:?}"
    );
    assert!(
        keys.contains(&format!("new_assignment:{assigned}")),
        "and every other source is untouched: {keys:?}"
    );
}

/// With no identity configured, the four rules that ask "is this mine" match
/// nothing -- which is the honest answer, not a silent everything. Credential
/// expiry needs no identity and is still there.
#[tokio::test]
async fn with_no_identity_only_the_rule_that_needs_none_produces_anything() {
    let pool = &scratch().await;
    source(pool, "jira", Some(now() + Duration::days(5)), true).await;
    pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    ticket(
        pool,
        "PAY-1",
        Some(THEM),
        Some(ME),
        "@mara.lindqvist ping?",
        days_ago(1),
    )
    .await;

    let items = inbox::items(pool, &[], now(), Shelf::Stream, &declarations())
        .await
        .unwrap();
    assert_eq!(keys(&items), vec!["credential_expiry:jira".to_owned()]);
}

/// The item carries what a reader needs to judge it without opening it
/// (story 7) and a way in (story 8).
#[tokio::test]
async fn an_item_says_where_it_came_from_and_what_it_is_about() {
    let pool = &scratch().await;
    let pr = pull_request(pool, "acme/payouts#144", &[ME], "open").await;

    let items = stream(pool).await;
    let item = items.first().expect("one review request");
    assert_eq!(item.category, Category::ReviewRequest);
    assert_eq!(item.source_id, "gitea");
    assert_eq!(item.entity_id.as_deref(), Some(pr.as_str()));
    assert_eq!(item.kind.as_deref(), Some("pr"));
    assert_eq!(item.title, "acme/payouts#144");
    assert!(
        item.reason.contains(THEM),
        "the reason names who is waiting: {}",
        item.reason
    );
}

/// A credential expiry is the one item with no entity behind it, and it says
/// so by carrying none rather than by pointing somewhere wrong.
#[tokio::test]
async fn a_credential_expiry_carries_its_source_and_no_entity() {
    let pool = &scratch().await;
    source(pool, "jira", Some(now() + Duration::days(4)), true).await;

    let items = stream(pool).await;
    let item = items.first().expect("one expiry");
    assert_eq!(item.category, Category::CredentialExpiry);
    assert_eq!(item.source_id, "jira");
    assert_eq!(item.entity_id, None);
    assert_eq!(item.kind, None);
    assert!(item.reason.contains("expires"), "{}", item.reason);
}

/// One rule at a time, which is what makes a negative control a statement
/// about *that* rule rather than about the union.
#[tokio::test]
async fn one_rule_sees_only_its_own_category() {
    let pool = &scratch().await;
    pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    ticket(
        pool,
        "PAY-1",
        Some(THEM),
        None,
        "@mara.lindqvist ping?",
        days_ago(1),
    )
    .await;

    let rule = inbox::rule(Category::Mention).expect("a rule for mentions");
    let items = inbox::items_from(pool, rule, &me(), now(), Shelf::Stream, &declarations())
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].category, Category::Mention);
}
