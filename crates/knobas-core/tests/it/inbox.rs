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
//! it -- so the file's shared database cannot isolate these tests the way
//! unique ids isolate the link battery: test A's stream would contain test B's
//! fixtures, and every "and nothing else is in the stream" assertion in this
//! file would be decided by scheduling. `scratch_database` costs one `create
//! database` per test on the same postmaster, and buys assertions that mean
//! what they say. It is the same reasoning `tests/it/suggestions.rs` records.
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
/// in. A pull request's state and a build's state and pipeline are read
/// through the declaration too (#2364), so `gitea` declares its state as its
/// adapter does, and `teamcity` and `teamcity2` TeamCity's build state and its
/// pipeline: `buildTypeId`, and `buildType.id` second, the spelling
/// [`every_build_mine_names_what_the_per_build_statement_named`] holds.
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
                state: gitea_pr_state(),
                ..KindPaths::default()
            }],
        )
        .with("teamcity", vec![teamcity_build()])
        .with("teamcity2", vec![teamcity_build()])
        .with("forge", forge_kinds())
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

/// Gitea's pull request state, as its descriptor declares it: the merged
/// flag first, then `state`.
fn gitea_pr_state() -> Vec<knobas_core::payload::StatePath> {
    use knobas_core::payload::{PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    vec![
        StatePath {
            at: PayloadPath::of(["merged"]),
            values: vec![StateValue::of(true, Tone::Ok).shown_as("merged")],
        },
        StatePath {
            at: PayloadPath::of(["state"]),
            values: vec![
                StateValue::of("open", Tone::Info),
                StateValue::of("closed", Tone::Muted),
            ],
        },
    ]
}

/// A TeamCity build's state and pipeline, as its descriptor declares them,
/// with `buildType.id` as a second pipeline candidate.
fn teamcity_build() -> knobas_core::payload::KindPaths {
    use knobas_core::payload::{KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    KindPaths {
        kind: "build".to_owned(),
        state: vec![
            StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("running", Tone::Info),
                    StateValue::of("queued", Tone::Info),
                ],
            },
            StatePath {
                at: PayloadPath::of(["status"]),
                values: vec![
                    StateValue::of("SUCCESS", Tone::Ok),
                    StateValue::of("FAILURE", Tone::Err),
                    StateValue::of("UNKNOWN", Tone::Muted),
                ],
            },
        ],
        pipeline: vec![
            PayloadPath::of(["buildTypeId"]),
            PayloadPath::of(["buildType", "id"]),
        ],
        ..KindPaths::default()
    }
}

/// A source that spells everything its own way (#2364): a pull request's
/// reviewers at `asked`, its state at `phase` (`active` shown as `open`,
/// `done`), and a build's outcome at `result` and its pipeline at `job`.
/// Neither kind carries Gitea's or TeamCity's keys where it means them.
fn forge_kinds() -> Vec<knobas_core::payload::KindPaths> {
    use knobas_core::payload::{KindPaths, ListPath, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    vec![
        KindPaths {
            kind: "pr".to_owned(),
            reviewers: vec![ListPath {
                at: PayloadPath::of(["asked"]),
                entry: PayloadPath::default(),
            }],
            state: vec![StatePath {
                at: PayloadPath::of(["phase"]),
                values: vec![
                    StateValue::of("active", Tone::Info).shown_as("open"),
                    StateValue::of("done", Tone::Muted),
                ],
            }],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "build".to_owned(),
            state: vec![StatePath {
                at: PayloadPath::of(["result"]),
                values: vec![
                    StateValue::of("running", Tone::Info),
                    StateValue::of("broken", Tone::Err),
                    StateValue::of("passed", Tone::Ok),
                ],
            }],
            pipeline: vec![PayloadPath::of(["job"])],
            ..KindPaths::default()
        },
    ]
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
    link_as(pool, from, to, "related", confirmed).await;
}

/// The same, asserting a named relation -- `monitored-by` is what attaches a
/// monitor to the asset it watches (`CONTEXT.md`, **Monitor**).
async fn link_as(pool: &PgPool, from: &str, to: &str, relation: &str, confirmed: bool) {
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by, confirmed_at,
              rule, rule_class, reason)
         values ($1,$2,$4,'manual','user',
                 case when $3 then now() end,
                 case when $3 then null else 'branch_name_key' end,
                 case when $3 then null else 'exact_key' end,
                 case when $3 then null else 'a reason' end)",
    )
    .bind(from)
    .bind(to)
    .bind(confirmed)
    .bind(relation)
    .execute(pool)
    .await
    .unwrap();
}

// -- the estate, for the alert rule (#446) ----------------------------------

/// One asset, under `parent` or at the top of the estate.
///
/// `path_text` is composed from the parent's the way `knobas_app::assets` does
/// it -- ancestor names, outermost first -- because the alert rule reads it
/// back as the asset's whole path and a fixture that left it empty could not
/// witness that.
async fn asset(pool: &PgPool, key: &str, name: &str, parent: Option<&str>) -> String {
    let id = EntityRef::new("asset", key).to_string();
    let path_text: String = match parent {
        None => String::new(),
        Some(parent) => sqlx::query_scalar(
            "select case when path_text = '' then name else path_text || ' / ' || name end
               from knobas.asset where id = $1",
        )
        .bind(parent)
        .fetch_one(pool)
        .await
        .unwrap(),
    };
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, parent_id, type_id, name, path_text)
         values ($1,$2,'vm',$3,$4)",
    )
    .bind(&id)
    .bind(parent)
    .bind(name)
    .bind(&path_text)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One mirrored Uptime Kuma monitor: the entity carries the *name* the alert
/// row is drawn with, which is what makes the title assertion below mean
/// something.
async fn monitor(pool: &PgPool, key: &str, name: &str) -> String {
    item(
        pool,
        Mirrored {
            source: "kuma",
            kind: "monitor",
            key,
            author: None,
            body: "",
            payload: serde_json::json!({ "active": true }),
            updated: days_ago(1),
        },
    )
    .await;
    let id = EntityRef::new("kuma", key).to_string();
    sqlx::query("update knobas.entity set title = $2 where id = $1")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// An open alert on `monitor`, opened at `at`.
async fn alert(pool: &PgPool, monitor: &str, state: &str, at: DateTime<Utc>) -> i64 {
    sqlx::query_scalar(
        "insert into knobas.monitor_alert (entity_id, state, opened_at)
         values ($1,$2,$3) returning id",
    )
    .bind(monitor)
    .bind(state)
    .bind(at)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A context, and -- with `holds` -- what it was told to hold.
async fn context(pool: &PgPool, key: &str, holds: &[&str]) -> String {
    let id = format!("ctx:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ctx',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.context (id, kind, title) values ($1,'adhoc',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    for held in holds {
        link(pool, &id, held, true).await;
    }
    id
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

@mara.lindqvist can you add the SLA?",
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

/// **A name is matched as the characters it is** (#1170). The rule looks for
/// every name at once in one expression built from all of them, and a name
/// can carry the expression's own syntax: unescaped, a domain-style account
/// `corp\mara_l` reads `\m` as a word boundary and misses its own mention.
/// Two names, so the alternation has two branches; the negative control is
/// another character where the `_` is, which is somebody else.
#[tokio::test]
async fn a_name_with_pattern_characters_is_matched_literally() {
    let pool = &scratch().await;
    let mine = ticket(
        pool,
        "PAY-1",
        Some(THEM),
        None,
        r"@corp\mara_l ping?",
        days_ago(1),
    )
    .await;
    let theirs = ticket(
        pool,
        "PAY-2",
        Some(THEM),
        None,
        r"@corp\maraXl ping?",
        days_ago(1),
    )
    .await;

    let items = inbox::items(
        pool,
        &[r"corp\mara_l".to_owned(), "50%".to_owned()],
        now(),
        Shelf::Stream,
        &declarations(),
    )
    .await
    .unwrap();
    let keys = keys(&items);
    assert!(keys.contains(&format!("mention:{mine}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("mention:{theirs}")),
        "an underscore in a name is not a wildcard: {keys:?}"
    );
}

/// The names are joined on `chr(1)` to be looked for at once (#1170), so a
/// name holding that character is looked for as its two halves. What decides
/// is still the whole name: a mention of one half is nobody's.
#[tokio::test]
async fn a_name_holding_the_joining_character_is_still_one_name() {
    let pool = &scratch().await;
    let half = ticket(pool, "PAY-1", Some(THEM), None, "@ab ping?", days_ago(1)).await;
    let whole = ticket(
        pool,
        "PAY-2",
        Some(THEM),
        None,
        "@ab\u{1}cd ping?",
        days_ago(1),
    )
    .await;

    let items = inbox::items(
        pool,
        &["ab\u{1}cd".to_owned()],
        now(),
        Shelf::Stream,
        &declarations(),
    )
    .await
    .unwrap();
    let keys = keys(&items);
    assert!(keys.contains(&format!("mention:{whole}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("mention:{half}")),
        "half a name is not the name: {keys:?}"
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

/// The link's other direction (#800): my work linked to the red build reaches
/// me as the build linked to my work does, and a red build linked to
/// somebody else's work does not.
#[tokio::test]
async fn a_failed_build_reaches_me_through_a_link_from_my_work() {
    let pool = &scratch().await;
    let mine = ticket(pool, "PAY-231", Some(ME), None, "", days_ago(2)).await;
    let theirs = ticket(pool, "PAY-232", Some("someone-else"), None, "", days_ago(2)).await;

    let on_mine = build(pool, "build:1187", "FAILURE", "A", None, days_ago(1)).await;
    let on_theirs = build(pool, "build:1188", "FAILURE", "B", None, days_ago(1)).await;
    link(pool, &mine, &on_mine, true).await;
    link(pool, &theirs, &on_theirs, true).await;

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("failed_build:{on_mine}")),
        "a red build my work links to is on my work: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("failed_build:{on_theirs}")),
        "a red build on somebody else's work is not mine: {keys:?}"
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

/// A pull request's state and a build's outcome and pipeline are read where
/// the kind **declares** them (#2364), not at Gitea's `state` or TeamCity's
/// `status`, `state` and `buildTypeId`. Every `forge` record carries those
/// keys too, saying the opposite of its declared ones, so a rule that still
/// read them answers every case here the other way round.
#[tokio::test]
async fn a_review_request_and_a_failed_build_are_read_where_their_kind_declares_them() {
    let pool = &scratch().await;
    let forge = |key: &'static str, kind: &'static str, payload: serde_json::Value, days| {
        (key, kind, payload, days)
    };
    let rows = [
        forge(
            "forge:pr-1",
            "pr",
            serde_json::json!({ "asked": [ME], "phase": "active", "state": "closed" }),
            1,
        ),
        forge(
            "forge:pr-2",
            "pr",
            serde_json::json!({ "asked": [ME], "phase": "done", "state": "open" }),
            1,
        ),
        // Red, and nothing newer of its job is green: still asking.
        forge(
            "forge:b-1",
            "build",
            serde_json::json!({ "result": "broken", "job": "lint",
                                "status": "SUCCESS", "buildTypeId": "tests" }),
            3,
        ),
        // Red, and its own job went green a day later: resolved.
        forge(
            "forge:b-2",
            "build",
            serde_json::json!({ "result": "broken", "job": "tests",
                                "status": "SUCCESS", "buildTypeId": "lint" }),
            3,
        ),
        forge(
            "forge:b-3",
            "build",
            serde_json::json!({ "result": "passed", "job": "tests",
                                "status": "FAILURE", "state": "finished",
                                "buildTypeId": "lint" }),
            2,
        ),
    ];
    let mut ids = Vec::new();
    for (key, kind, payload, days) in rows {
        ids.push(
            item(
                pool,
                Mirrored {
                    source: "forge",
                    kind,
                    key,
                    author: Some(ME),
                    body: "",
                    payload,
                    updated: days_ago(days),
                },
            )
            .await,
        );
    }

    let mut got = keys(&stream(pool).await);
    got.sort();
    let mut want = vec![
        format!("review_request:{}", ids[0]),
        format!("failed_build:{}", ids[2]),
    ];
    want.sort();
    assert_eq!(
        got, want,
        "the open pull request and the red build of a job nothing has fixed, by the declared \
         paths alone"
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

/// The assignee is the **first** declared candidate that holds a non-blank
/// string, and only that one decides. The rule first asks whether any of a
/// ticket's own source's candidates names me, and reads the first-string rule
/// only for the tickets that pass (#1164), so each case below is one way that
/// shortcut could answer differently from the rule:
///
/// * `name` names somebody else and `key` names me: `key` passes the first
///   question, and the ticket is still not mine;
/// * `name` is blank, or not a string, and `key` names me: `name` is skipped,
///   so the assignee is me;
/// * `name` names me padded with spaces: the rule trims, so it is me;
/// * `name` and `key` both name me: one item, not one per candidate;
/// * a ticket of another source that spells me only at a path *this* source
///   declares and its own does not: each source's own paths, never another's.
#[tokio::test]
async fn the_first_declared_assignee_decides_and_only_for_its_own_source() {
    let pool = &scratch().await;
    let assigned = |key: &'static str, assignee: serde_json::Value| Mirrored {
        source: "jira",
        kind: "ticket",
        key,
        author: Some(THEM),
        body: "",
        payload: serde_json::json!({ "fields": { "assignee": assignee } }),
        updated: days_ago(1),
    };
    let overruled = item(
        pool,
        assigned("PAY-270", serde_json::json!({ "name": THEM, "key": ME })),
    )
    .await;
    let blank_name = item(
        pool,
        assigned("PAY-271", serde_json::json!({ "name": "  ", "key": ME })),
    )
    .await;
    let numeric_name = item(
        pool,
        assigned("PAY-272", serde_json::json!({ "name": 42, "key": ME })),
    )
    .await;
    let padded = item(
        pool,
        assigned("PAY-273", serde_json::json!({ "name": format!(" {ME} ") })),
    )
    .await;
    let twice = item(
        pool,
        assigned("PAY-274", serde_json::json!({ "name": ME, "key": ME })),
    )
    .await;
    let foreign_path = item(
        pool,
        Mirrored {
            source: "jira-eu",
            ..assigned("EU-1", serde_json::json!({ "key": ME }))
        },
    )
    .await;
    let declared = declarations().with(
        "jira-eu",
        vec![knobas_core::payload::KindPaths {
            kind: "ticket".to_owned(),
            assignee: vec![knobas_core::payload::PayloadPath::of([
                "fields", "assignee", "name",
            ])],
            ..knobas_core::payload::KindPaths::default()
        }],
    );

    let items = inbox::items(pool, &me(), now(), Shelf::Stream, &declared)
        .await
        .unwrap();
    let keys = keys(&items);
    let has = |subject: &str| keys.contains(&format!("new_assignment:{subject}"));
    assert!(
        !has(&overruled),
        "the first candidate names somebody else, so a later one naming me does not count: {keys:?}"
    );
    assert!(
        has(&blank_name),
        "a blank first candidate is skipped: {keys:?}"
    );
    assert!(
        has(&numeric_name),
        "a first candidate that is not a string is skipped: {keys:?}"
    );
    assert!(has(&padded), "the assignee is read trimmed: {keys:?}");
    assert_eq!(
        keys.iter()
            .filter(|key| **key == format!("new_assignment:{twice}"))
            .count(),
        1,
        "two candidates naming me are one assignment: {keys:?}"
    );
    assert!(
        !has(&foreign_path),
        "jira-eu declares only `name`; `key` is another source's path: {keys:?}"
    );
    let count = inbox::count(pool, &me(), now(), &declared).await.unwrap();
    assert_eq!(
        usize::try_from(count).unwrap(),
        items.len(),
        "the count is the stream's own rows"
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

// -- the sixth rule: an alert, and only when a context holds its asset -------

/// Story 59 and its negative control, story 60: *"an alert in my inbox only
/// when some context holds the affected asset"*, and *"a fresh install with no
/// contexts shows alerts in the Assets view and the top strip and never in the
/// inbox"*.
///
/// Four monitors, all of them in trouble, and one item. The three that stay
/// out are the three ways a rule reading "is there an open alert" would be
/// wrong: an asset nobody has put in a context, a monitor attached to no asset
/// at all, and a monitor whose link to a held asset is only *proposed* -- the
/// same distinction the failed-build rule draws, and for the same reason (#41,
/// #161: an inbox built on guesses is one the reader stops trusting).
#[tokio::test]
async fn an_alert_is_in_the_stream_only_when_a_context_holds_its_asset() {
    let pool = &scratch().await;
    let held = asset(pool, "db-01", "db-01", None).await;
    let loose = asset(pool, "spare", "spare", None).await;
    let guessed = asset(pool, "guess", "guess", None).await;
    context(pool, "payouts", &[&held, &guessed]).await;

    let watched = monitor(pool, "1", "db-01 ping").await;
    let unheld = monitor(pool, "2", "spare ping").await;
    let orphan = monitor(pool, "3", "nothing at all").await;
    let proposed = monitor(pool, "4", "guess ping").await;
    link_as(pool, &watched, &held, "monitored-by", true).await;
    link_as(pool, &unheld, &loose, "monitored-by", true).await;
    link_as(pool, &proposed, &guessed, "monitored-by", false).await;
    for id in [&watched, &unheld, &orphan, &proposed] {
        alert(pool, id, "down", days_ago(1)).await;
    }

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("alert:{watched}")),
        "an alert on an asset a context holds is in the stream: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("alert:{unheld}")),
        "an alert on an asset no context holds is not mine to answer: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("alert:{orphan}")),
        "a monitor watching nothing reaches no inbox: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("alert:{proposed}")),
        "a proposed monitored-by link is a guess, not a routing rule: {keys:?}"
    );
}

/// *"directly or through an ancestor"* -- `CONTEXT.md`, **Alert**, over
/// ADR-0008's asset clause and ADR-0014's parent column.
///
/// The context holds the **site**; the monitor watches a container three
/// levels below it. The negative control is a whole sibling subtree, deep
/// enough to be reached by any rule that expanded over links instead of over
/// the parent field.
#[tokio::test]
async fn an_alert_reaches_the_stream_through_the_assets_ancestors() {
    let pool = &scratch().await;
    let site = asset(pool, "hel", "hel", None).await;
    let vm = asset(pool, "hel1", "hel1", Some(&site)).await;
    let container = asset(pool, "jira", "knobas-jira", Some(&vm)).await;
    let elsewhere = asset(pool, "fsn", "fsn", None).await;
    let sibling = asset(pool, "fsn1", "fsn1", Some(&elsewhere)).await;
    context(pool, "payouts", &[&site]).await;

    let deep = monitor(pool, "1", "jira (tunnel)").await;
    let away = monitor(pool, "2", "fsn1 ping").await;
    link_as(pool, &deep, &container, "monitored-by", true).await;
    link_as(pool, &away, &sibling, "monitored-by", true).await;
    alert(pool, &deep, "down", days_ago(1)).await;
    alert(pool, &away, "down", days_ago(1)).await;

    let keys = keys(&stream(pool).await);
    assert!(
        keys.contains(&format!("alert:{deep}")),
        "a context holding the site holds what is inside it: {keys:?}"
    );
    assert!(
        !keys.contains(&format!("alert:{away}")),
        "a sibling subtree is not in the context: {keys:?}"
    );
}

/// Story 62 and story 64, as the *rule* sees them: **ack** is seen and
/// **recovery** is fixed, and both take the item out without anybody deleting
/// one.
///
/// Three alerts on three monitors watching three assets one context holds, so
/// the only difference between them is the two columns under test.
#[tokio::test]
async fn an_acked_alert_and_a_recovered_one_have_both_left_the_stream() {
    let pool = &scratch().await;
    let ids: Vec<String> = {
        let mut out = Vec::new();
        for key in ["a", "b", "c"] {
            out.push(asset(pool, key, key, None).await);
        }
        out
    };
    context(
        pool,
        "payouts",
        &ids.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await;

    let open = monitor(pool, "1", "a ping").await;
    let acked = monitor(pool, "2", "b ping").await;
    let recovered = monitor(pool, "3", "c ping").await;
    for (id, asset_id) in [(&open, &ids[0]), (&acked, &ids[1]), (&recovered, &ids[2])] {
        link_as(pool, id, asset_id, "monitored-by", true).await;
    }
    alert(pool, &open, "down", days_ago(1)).await;
    let acked_id = alert(pool, &acked, "down", days_ago(1)).await;
    let closed_id = alert(pool, &recovered, "down", days_ago(2)).await;
    sqlx::query("update knobas.monitor_alert set acked_at = now() where id = $1")
        .bind(acked_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("update knobas.monitor_alert set closed_at = now() where id = $1")
        .bind(closed_id)
        .execute(pool)
        .await
        .unwrap();

    let keys = keys(&stream(pool).await);
    assert_eq!(
        keys,
        vec![format!("alert:{open}")],
        "only the open, un-acked alert is a demand: {keys:?}"
    );
}

/// Story 65: *"snooze an alert like any inbox item"*. It is the inbox's
/// snooze and not a second one -- the same key, the same table, the same
/// clock.
#[tokio::test]
async fn an_alert_is_snoozed_and_comes_back_like_any_other_item() {
    let pool = &scratch().await;
    let vm = asset(pool, "db-01", "db-01", None).await;
    context(pool, "payouts", &[&vm]).await;
    let watching = monitor(pool, "1", "db-01 ping").await;
    link_as(pool, &watching, &vm, "monitored-by", true).await;
    alert(pool, &watching, "down", days_ago(1)).await;

    let key = format!("alert:{watching}");
    let until = now() + Duration::days(2);
    inbox::snooze(pool, &key, until).await.unwrap();

    assert!(
        !keys(&stream(pool).await).contains(&key),
        "a snoozed alert is off the stream"
    );
    let shelved = inbox::items(pool, &me(), now(), Shelf::Snoozed, &declarations())
        .await
        .unwrap();
    assert_eq!(keys(&shelved), vec![key.clone()]);
    assert_eq!(shelved[0].snoozed_until, Some(until));
    assert!(
        keys(&stream_at(pool, until + Duration::hours(1)).await).contains(&key),
        "and it is back when the maintenance window is over"
    );
}

/// What the row says: story 7 (judge it without opening it) and story 61 (the
/// way in is the **asset**, not the monitor).
#[tokio::test]
async fn an_alert_names_its_monitor_and_the_path_of_the_asset_it_watches() {
    let pool = &scratch().await;
    let site = asset(pool, "hel", "hel", None).await;
    let vm = asset(pool, "hel1", "hel1", Some(&site)).await;
    context(pool, "payouts", &[&site]).await;
    let watching = monitor(pool, "1", "jira (tunnel)").await;
    link_as(pool, &watching, &vm, "monitored-by", true).await;
    alert(pool, &watching, "warn", days_ago(1)).await;

    let items = stream(pool).await;
    let item = items.first().expect("one alert");
    assert_eq!(item.category, Category::Alert);
    assert_eq!(item.source_id, "kuma");
    assert_eq!(item.title, "jira (tunnel)", "the monitor's name");
    assert_eq!(
        item.entity_id.as_deref(),
        Some(vm.as_str()),
        "opening an alert lands at the affected asset"
    );
    assert_eq!(item.kind.as_deref(), Some("asset"));
    assert_eq!(
        item.reason, "hel / hel1 is warn",
        "the reason is the asset's whole path and the trouble it is in"
    );
    assert_eq!(item.occurred_at, days_ago(1));
    assert_eq!(item.web_url, None);
}

/// A context the reader archived is one they put away, and an alert must not
/// go on interrupting them because of it.
///
/// The one place `held_by_any_context!` and `member_ids` differ on purpose --
/// see the macro's own note. The positive half is in the same test so this is
/// a statement about *archiving* and not about the fixture.
#[tokio::test]
async fn an_alert_leaves_the_stream_when_the_only_context_holding_it_is_archived() {
    let pool = &scratch().await;
    let vm = asset(pool, "db-01", "db-01", None).await;
    let ctx = context(pool, "payouts", &[&vm]).await;
    let watching = monitor(pool, "1", "db-01 ping").await;
    link_as(pool, &watching, &vm, "monitored-by", true).await;
    alert(pool, &watching, "down", days_ago(1)).await;

    let key = format!("alert:{watching}");
    assert!(keys(&stream(pool).await).contains(&key));

    sqlx::query("update knobas.context set archived_at = now() where id = $1")
        .bind(&ctx)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        !keys(&stream(pool).await).contains(&key),
        "an archived context holds nothing anybody is still working on"
    );
}

/// A monitor may watch several assets -- a VM and the container on it can
/// honestly both be behind one HTTP check -- and that is still **one** demand.
#[tokio::test]
async fn an_alert_on_a_monitor_watching_several_assets_is_one_item() {
    let pool = &scratch().await;
    let vm = asset(pool, "hel1", "hel1", None).await;
    let container = asset(pool, "jira", "knobas-jira", Some(&vm)).await;
    context(pool, "payouts", &[&vm]).await;
    let watching = monitor(pool, "1", "jira (tunnel)").await;
    link_as(pool, &watching, &vm, "monitored-by", true).await;
    link_as(pool, &watching, &container, "monitored-by", true).await;
    alert(pool, &watching, "down", days_ago(1)).await;

    let items = stream(pool).await;
    assert_eq!(keys(&items), vec![format!("alert:{watching}")]);
    assert_eq!(
        items[0].entity_id.as_deref(),
        Some(vm.as_str()),
        "and the asset it opens is the same one on every read"
    );
}

/// The sixth category is an addition and not a rewrite: one item of each of
/// the six, all six on the stream, and the count over them.
#[tokio::test]
async fn the_five_existing_categories_are_untouched_by_the_sixth() {
    let pool = &scratch().await;
    source(pool, "jira", Some(now() + Duration::days(4)), true).await;
    let pr = pull_request(pool, "acme/payouts#144", &[ME], "open").await;
    let mention = ticket(
        pool,
        "PAY-9",
        Some(THEM),
        None,
        "@mara.lindqvist ping?",
        days_ago(1),
    )
    .await;
    let assigned = ticket(pool, "PAY-1", Some(THEM), Some(ME), "", days_ago(1)).await;
    let red = build(
        pool,
        "teamcity:Payouts_Build:9",
        "FAILURE",
        "Payouts_Build",
        Some(ME),
        days_ago(1),
    )
    .await;
    let vm = asset(pool, "db-01", "db-01", None).await;
    context(pool, "payouts", &[&vm]).await;
    let watching = monitor(pool, "1", "db-01 ping").await;
    link_as(pool, &watching, &vm, "monitored-by", true).await;
    alert(pool, &watching, "down", days_ago(1)).await;

    let mut keys = keys(&stream(pool).await);
    keys.sort();
    let mut want = vec![
        format!("review_request:{pr}"),
        format!("mention:{mention}"),
        format!("new_assignment:{assigned}"),
        format!("failed_build:{red}"),
        "credential_expiry:jira".to_owned(),
        format!("alert:{watching}"),
    ];
    want.sort();
    assert_eq!(keys, want);
    assert_eq!(
        inbox::count(pool, &me(), now(), &declarations())
            .await
            .unwrap(),
        6,
        "and the count is the same statement, counted"
    );
}

/// The failed-build rule as it read before #785, through `sync.live_item`, per
/// build: the oracle the reshaped rule is held to. Kept verbatim in its
/// predicates, so this test compares two statements and not a statement with
/// a second reading of it.
const FAILED_BUILD_BEFORE_785: &str = "
    select i.entity_id
      from sync.live_item i
     where i.kind = 'build'
       and i.payload->>'status' = 'FAILURE'
       and coalesce(i.payload->>'state', 'finished') = 'finished'
       and (   i.author = any($1)
            or exists (
                  select 1
                    from knobas.confirmed_link l
                    join sync.live_item mine
                      on mine.entity_id = case when l.from_id = i.entity_id
                                               then l.to_id else l.from_id end
                   where (l.from_id = i.entity_id or l.to_id = i.entity_id)
                     and mine.author = any($1)))
       and not exists (
             select 1
               from sync.live_item newer
              where newer.kind = 'build'
                and newer.source_id = i.source_id
                and newer.payload->>'status' = 'SUCCESS'
                and coalesce(newer.payload->>'state', 'finished') = 'finished'
                and coalesce(newer.payload->>'buildTypeId',
                             newer.payload->'buildType'->>'id')
                  = coalesce(i.payload->>'buildTypeId',
                             i.payload->'buildType'->>'id')
                and coalesce(i.payload->>'buildTypeId',
                             i.payload->'buildType'->>'id') is not null
                and coalesce(newer.item_updated_at, newer.synced_at)
                  > coalesce(i.item_updated_at, i.synced_at))
     order by 1";

/// **Every build mine, and the answer unchanged** (#785). The reshaped rule
/// reads the newer green build of a configuration once per statement rather
/// than once per failed build, and the confirmed links once rather than per
/// build; on a mirror where every build is the reader's -- the case that read
/// 195 ms -- it must name exactly the builds the per-build statement named.
///
/// The mirror holds each way a red build is resolved or not: a later green of
/// the same configuration in the same source and not in another source, a
/// green that is only running, a green that is older, a tombstoned green, a
/// green of a turned-off source, the configuration spelled under `buildType`,
/// a build with no configuration, and a build reached only through a
/// confirmed link in either direction, a link to itself and a proposal.
#[tokio::test]
async fn every_build_mine_names_what_the_per_build_statement_named() {
    let pool = &scratch().await;
    source(pool, "teamcity", None, true).await;
    source(pool, "teamcity2", None, true).await;
    source(pool, "offci", None, false).await;
    let mut n = 0;
    let mut row = |source: &'static str,
                   status: &'static str,
                   state: &'static str,
                   config: Option<&'static str>,
                   nested: bool,
                   author: Option<&'static str>,
                   hours: i64| {
        n += 1;
        let mut payload = serde_json::json!({ "status": status, "state": state });
        match (config, nested) {
            (Some(c), false) => payload["buildTypeId"] = c.into(),
            (Some(c), true) => payload["buildType"] = serde_json::json!({ "id": c }),
            (None, _) => {}
        }
        (
            source,
            format!("build:{n}"),
            author,
            payload,
            now() - Duration::hours(hours),
        )
    };
    let mut rows = Vec::new();
    // Twelve configurations, red and green interleaved, all mine.
    for k in 0..60_i64 {
        let config =
            ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"][(k % 12) as usize];
        let status = if k % 5 == 0 || k % 7 == 0 {
            "FAILURE"
        } else {
            "SUCCESS"
        };
        rows.push(row(
            "teamcity",
            status,
            "finished",
            Some(config),
            k % 3 == 0,
            Some(ME),
            100 - k,
        ));
    }
    // Red builds whose only newer green is running, older, in another source,
    // tombstoned, or in a turned-off source.
    let running = rows.len();
    rows.push(row(
        "teamcity",
        "FAILURE",
        "finished",
        Some("Run"),
        false,
        Some(ME),
        10,
    ));
    rows.push(row(
        "teamcity",
        "SUCCESS",
        "running",
        Some("Run"),
        false,
        Some(ME),
        1,
    ));
    rows.push(row(
        "teamcity",
        "SUCCESS",
        "finished",
        Some("Old"),
        false,
        Some(ME),
        20,
    ));
    let older = rows.len();
    rows.push(row(
        "teamcity",
        "FAILURE",
        "finished",
        Some("Old"),
        false,
        Some(ME),
        10,
    ));
    let elsewhere = rows.len();
    rows.push(row(
        "teamcity",
        "FAILURE",
        "finished",
        Some("Other"),
        false,
        Some(ME),
        10,
    ));
    rows.push(row(
        "teamcity2",
        "SUCCESS",
        "finished",
        Some("Other"),
        false,
        Some(ME),
        1,
    ));
    let tombstoned = rows.len();
    rows.push(row(
        "teamcity",
        "FAILURE",
        "finished",
        Some("Gone"),
        false,
        Some(ME),
        10,
    ));
    let gone = rows.len();
    rows.push(row(
        "teamcity",
        "SUCCESS",
        "finished",
        Some("Gone"),
        false,
        Some(ME),
        1,
    ));
    let off = rows.len();
    rows.push(row(
        "offci",
        "FAILURE",
        "finished",
        Some("Off"),
        false,
        Some(ME),
        10,
    ));
    rows.push(row(
        "offci",
        "SUCCESS",
        "finished",
        Some("Off"),
        false,
        Some(ME),
        1,
    ));
    // No configuration: nothing can resolve it.
    let unconfigured = rows.len();
    rows.push(row(
        "teamcity",
        "FAILURE",
        "finished",
        None,
        false,
        Some(ME),
        10,
    ));
    rows.push(row(
        "teamcity",
        "SUCCESS",
        "finished",
        None,
        false,
        Some(ME),
        1,
    ));
    // Somebody else's, reached only by a link.
    let linked = rows.len();
    for _ in 0..4 {
        rows.push(row(
            "teamcity",
            "FAILURE",
            "finished",
            Some("Link"),
            false,
            Some(THEM),
            10,
        ));
    }
    let mut ids = Vec::new();
    for (source, key, author, payload, updated) in rows {
        ids.push(
            item(
                pool,
                Mirrored {
                    source,
                    kind: "build",
                    key: &key,
                    author,
                    body: "",
                    payload,
                    updated,
                },
            )
            .await,
        );
    }
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&ids[gone])
        .execute(pool)
        .await
        .unwrap();
    let mine = ticket(pool, "PAY-1", Some(ME), None, "", days_ago(1)).await;
    let theirs = ticket(pool, "PAY-2", Some(THEM), None, "", days_ago(1)).await;
    link(pool, &ids[linked], &mine, true).await;
    link(pool, &mine, &ids[linked + 1], true).await;
    link(pool, &ids[linked + 2], &mine, false).await;
    link(pool, &ids[linked + 3], &ids[linked + 3], true).await;
    link(pool, &ids[linked + 3], &theirs, true).await;

    let before: Vec<String> = sqlx::query_scalar(FAILED_BUILD_BEFORE_785)
        .bind(me())
        .fetch_all(pool)
        .await
        .unwrap();
    let rule = inbox::rule(Category::FailedBuild).unwrap();
    let mut after: Vec<String> =
        inbox::items_from(pool, rule, &me(), now(), Shelf::Stream, &declarations())
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.entity_id.unwrap())
            .collect();
    after.sort();
    assert_eq!(after, before);
    // Worked by hand from the mirror above, so the comparison cannot pass by
    // both statements naming nothing.
    for (what, i, wanted) in [
        ("the newest red of H, never green again", 55, true),
        ("a red of A that A's next green resolved", 0, false),
        ("a red whose re-run is still running", running, true),
        ("a red with only an older green", older, true),
        ("a red whose green is in another source", elsewhere, true),
        ("a red whose green is tombstoned", tombstoned, true),
        ("a red of a turned-off source", off, false),
        ("a red with no configuration", unconfigured, true),
        ("a red linked from me", linked, true),
        ("a red linked to me", linked + 1, true),
        ("a red only proposed to me", linked + 2, false),
        (
            "a red linked to itself and somebody else",
            linked + 3,
            false,
        ),
    ] {
        assert_eq!(after.contains(&ids[i]), wanted, "{what}: {after:?}");
    }
}

/// **The stream, the shelf and the count are planned for their values every
/// time** (#785). Kept as one prepared statement, the stream and the snoozed
/// shelf shared it, and from its sixth execution PostgreSQL ran its generic
/// plan: on the probe's 22,000 items never analyzed the shelf read 70 ms
/// against 12 for the stream's custom plan. What is pinned is the mechanism,
/// the board's (#753): after eight reads of each on one connection, it holds
/// no prepared statement of the inbox's.
#[tokio::test]
async fn the_inbox_statements_are_never_kept_as_prepared_statements() {
    let pool = knobas_db::test_util::scratch_database("inbox")
        .await
        .pool(1)
        .await
        .expect("a one-connection pool");
    build(&pool, "build:1", "FAILURE", "A", Some(ME), days_ago(1)).await;
    for _ in 0..8 {
        for shelf in [Shelf::Stream, Shelf::Snoozed] {
            inbox::items(&pool, &me(), now(), shelf, &declarations())
                .await
                .unwrap();
        }
        inbox::count(&pool, &me(), now(), &declarations())
            .await
            .unwrap();
    }
    let kept: i64 = sqlx::query_scalar(
        "select count(*) from pg_prepared_statements where statement like '%candidate (category%'",
    )
    .persistent(false)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        kept, 0,
        "an inbox statement was kept, and goes generic on its sixth run"
    );
}

// -- the seventh rule: a failed sign-in, once per episode (#1571) ------------

/// Record a credential verdict as the one writer does: `auth_state` and
/// `auth_checked_at` move together (`knobas_sync::config::set_health`). The
/// episode's start is not written here -- migration `0055`'s trigger keeps it.
async fn verdict(pool: &PgPool, id: &str, state: &str) {
    sqlx::query(
        "update knobas.source_config
            set auth_state = $2, auth_checked_at = now()
          where id = $1",
    )
    .bind(id)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

/// The failed sign-in rule, alone.
async fn sign_ins(pool: &PgPool) -> Vec<inbox::InboxItem> {
    let rule = inbox::rule(Category::FailedSignIn).expect("a rule for failed sign-ins");
    inbox::items_from(pool, rule, &me(), now(), Shelf::Stream, &declarations())
        .await
        .unwrap()
}

/// A sign-in fails when the credential was rejected or none is stored -- the
/// shell's `signInFails` -- and in no other state. The negatives are the three
/// states that are not a failed sign-in (`ok`, `unreachable`, which is retried
/// on its own, and `unknown`) and a rejected source the user switched off.
#[tokio::test]
async fn a_rejected_or_missing_credential_is_an_item_and_nothing_else_is() {
    let pool = &scratch().await;
    for (id, state, enabled) in [
        ("rejected", "unauthorized", true),
        ("absent", "missing_secret", true),
        ("fine", "ok", true),
        ("down", "unreachable", true),
        ("untested", "unknown", true),
        ("off", "unauthorized", false),
    ] {
        source(pool, id, None, enabled).await;
        verdict(pool, id, state).await;
    }

    let mut keys = keys(&sign_ins(pool).await);
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "failed_sign_in:absent".to_owned(),
            "failed_sign_in:rejected".to_owned()
        ]
    );
}

/// What the row says: the source, no entity, and which of the two failures it
/// is -- a rejected credential and a missing one ask for the same act but are
/// different sentences, as the sources view's two prompts are.
#[tokio::test]
async fn a_failed_sign_in_names_its_source_and_says_which_failure_it_is() {
    let pool = &scratch().await;
    source(pool, "teamcity", None, true).await;
    verdict(pool, "teamcity", "unauthorized").await;
    source(pool, "kuma", None, true).await;
    verdict(pool, "kuma", "missing_secret").await;

    let items = sign_ins(pool).await;
    let rejected = items
        .iter()
        .find(|i| i.source_id == "teamcity")
        .expect("teamcity");
    assert_eq!(rejected.category, Category::FailedSignIn);
    assert_eq!(rejected.entity_id, None);
    assert_eq!(rejected.kind, None);
    assert_eq!(rejected.title, "teamcity");
    assert_eq!(rejected.reason, "the teamcity sign-in was rejected");
    let absent = items.iter().find(|i| i.source_id == "kuma").expect("kuma");
    assert_eq!(absent.reason, "no credential is stored for kuma");
}

/// **Once per episode.** An episode is the run of consecutive failing
/// verdicts; the item is dated at its first, so a second rejection inside it
/// (a retry, *Test connection*) neither moves the row nor brings back an item
/// the reader marked done -- and a rejection changing into a missing secret is
/// the same episode. A verdict that is not a failure ends it and takes the
/// item with it by construction; the next failure is a new episode and comes
/// back past the old *done*.
#[tokio::test]
async fn a_failed_sign_in_is_raised_once_per_episode() {
    let pool = &scratch().await;
    source(pool, "jira", None, true).await;
    verdict(pool, "jira", "unauthorized").await;

    let first = sign_ins(pool).await;
    assert_eq!(keys(&first), vec!["failed_sign_in:jira".to_owned()]);
    let began = first[0].occurred_at;

    verdict(pool, "jira", "unauthorized").await;
    verdict(pool, "jira", "missing_secret").await;
    let same = sign_ins(pool).await;
    assert_eq!(
        same[0].occurred_at, began,
        "a repeated failure is the same episode, dated at its start"
    );

    inbox::complete(pool, "failed_sign_in:jira", began)
        .await
        .unwrap();
    verdict(pool, "jira", "unauthorized").await;
    assert!(
        sign_ins(pool).await.is_empty(),
        "done holds for the rest of the episode"
    );

    verdict(pool, "jira", "ok").await;
    assert!(sign_ins(pool).await.is_empty(), "a working sign-in ends it");

    verdict(pool, "jira", "unauthorized").await;
    let again = sign_ins(pool).await;
    assert_eq!(keys(&again), vec!["failed_sign_in:jira".to_owned()]);
    assert!(
        again[0].occurred_at > began,
        "the next failure is a new episode, past the old done"
    );
}

/// An item, not a nag: an episode that is still failing and was never answered
/// stays on the stream, and leaves it the moment the credential works, with no
/// answer from the reader -- the derived inbox's promise (#45, story 17).
#[tokio::test]
async fn a_failed_sign_in_leaves_when_the_sign_in_works_again() {
    let pool = &scratch().await;
    source(pool, "gitea", None, true).await;
    verdict(pool, "gitea", "missing_secret").await;
    assert_eq!(stream(pool).await.len(), 1);

    verdict(pool, "gitea", "ok").await;
    assert!(stream(pool).await.is_empty());
    assert_eq!(
        inbox::count(pool, &me(), now(), &declarations())
            .await
            .unwrap(),
        0
    );
}
