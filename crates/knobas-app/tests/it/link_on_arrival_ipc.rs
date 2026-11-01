//! *Link to <job>* on *New branch…* and *Link the PR to <key>* on the compare
//! page (#984): `submit_write`'s `link_to`. The branch or pull request a
//! queued write makes has no id until the source mints one and a sync brings
//! it in (#707), so the link is drawn after the write has been sent and the
//! source re-read, against what the mirror then holds: the branch by its name
//! in the repository, the open pull request from the head into the base.
//!
//! Over a real mirror, with the queue rows written the way the flush writes
//! them (`knobas_core::write_queue`); what Gitea does with the write is
//! `just gitea-live`'s to witness (#707, #822).

use knobas_app::IpcErrorCode;
use knobas_app::sources::write_queue::{
    ArrivalLinks, arrival_linkable, hold_arrival_link, link_arrivals, link_on_arrival,
};
use knobas_core::entity::EntityRef;
use knobas_core::payload::{Declarations, KindPaths, PayloadPath, StatePath, StateValue};
use knobas_core::tone::Tone;
use sqlx::PgPool;

const TICKET: &str = "jira:PAY-231";
const REPO: &str = "gitea:tidewater/payout-service";
const BRANCH: &str = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-backoff";
const PR: &str = "gitea:tidewater/payout-service#150";

async fn corpus(name: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    item(
        &pool,
        TICKET,
        "ticket",
        "Retry failed SEPA payouts",
        serde_json::json!({}),
    )
    .await;
    item(
        &pool,
        REPO,
        "repo",
        "tidewater/payout-service",
        serde_json::json!({}),
    )
    .await;
    pool
}

/// One live mirror item.
async fn item(pool: &PgPool, id: &str, kind: &str, title: &str, payload: serde_json::Value) {
    let entity = EntityRef::parse(id).expect("an entity id");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(id)
    .bind(&entity.namespace)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

fn new_branch() -> serde_json::Value {
    serde_json::json!({ "CreateBranch": {
        "entity": REPO, "name": "feature/PAY-231-backoff", "from_ref": "main"
    }})
}

fn new_pull_request() -> serde_json::Value {
    serde_json::json!({ "CreatePullRequest": {
        "entity": REPO, "title": "PAY-231: Retry failed SEPA payouts", "body": "",
        "head": "feature/PAY-231-backoff", "base": "main"
    }})
}

/// A write as `queue` leaves it, in the repository.
async fn queued(pool: &PgPool, payload: serde_json::Value) -> i64 {
    let op = payload
        .as_object()
        .and_then(|o| o.keys().next())
        .map(|k| {
            if k == "CreateBranch" {
                "create_branch"
            } else {
                "create_pull_request"
            }
        })
        .unwrap();
    knobas_core::write_queue::queue(
        pool,
        "gitea",
        &EntityRef::parse(REPO).unwrap(),
        op,
        payload,
        serde_json::json!({}),
    )
    .await
    .unwrap()
    .id
}

/// Gitea's declared merged flag, state and branches (#2358), as
/// `start_work`'s look-up reads them.
fn declarations() -> Declarations {
    Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "pr".to_owned(),
            merged: vec![PayloadPath::of(["merged"])],
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("open", Tone::Info),
                    StateValue::of("closed", Tone::Muted),
                ],
            }],
            head: vec![PayloadPath::of(["head", "ref"])],
            base: vec![PayloadPath::of(["base", "ref"])],
            ..KindPaths::default()
        }],
    )
}

/// The entities the job reaches by a confirmed link.
async fn reached(pool: &PgPool) -> Vec<String> {
    knobas_core::reach::reach(pool, TICKET)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.entity_id)
        .collect()
}

async fn relations(pool: &PgPool, from: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "select relation, confirmed_at is not null from knobas.link
          where from_id = $1 and to_id = $2 and deleted_at is null order by relation",
    )
    .bind(from)
    .bind(TICKET)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A sent `CreateBranch` links the branch once the mirror holds it, and not
/// before: the branch is in the job's reach, confirmed.
#[tokio::test]
async fn a_new_branch_is_linked_to_the_job_once_it_is_mirrored() {
    let pool = corpus("arrival_branch").await;
    let id = queued(&pool, new_branch()).await;
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();

    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap(),
        None
    );
    assert!(
        reached(&pool).await.is_empty(),
        "nothing mirrored, nothing linked"
    );

    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap()
            .as_deref(),
        Some(BRANCH)
    );
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
    assert_eq!(
        relations(&pool, BRANCH).await,
        vec![("related".to_owned(), true)]
    );
    // Twice is still one link.
    link_on_arrival(&pool, &declarations(), id, TICKET)
        .await
        .unwrap();
    assert_eq!(relations(&pool, BRANCH).await.len(), 1);
}

/// The key in the branch's name has already raised a proposal (the
/// `branch_name_key` rule, `related`): the box confirms it rather than
/// drawing a second edge beside it.
#[tokio::test]
async fn the_proposal_the_branch_name_raised_is_confirmed_not_doubled() {
    let pool = corpus("arrival_proposal").await;
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at,
                                  rule, rule_class, reason)
         values ($1, $2, 'related', 'suggested', 'knobas', null, 'branch_name_key', 'exact_key',
                 'the branch name contains PAY-231')",
    )
    .bind(BRANCH)
    .bind(TICKET)
    .execute(&pool)
    .await
    .unwrap();
    let id = queued(&pool, new_branch()).await;
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();

    link_on_arrival(&pool, &declarations(), id, TICKET)
        .await
        .unwrap();
    assert_eq!(
        relations(&pool, BRANCH).await,
        vec![("related".to_owned(), true)]
    );
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
}

/// A sent `CreatePullRequest` links the pull request open from its head into
/// its base; one there that is already merged is not this write's, and is not
/// linked.
#[tokio::test]
async fn a_new_pull_request_is_linked_once_it_is_mirrored_open_on_its_head() {
    let pool = corpus("arrival_pr").await;
    item(
        &pool,
        "gitea:tidewater/payout-service#7",
        "pr",
        "an older one",
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": "feature/PAY-231-backoff"}, "base": {"ref": "main"}}),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap(),
        None
    );
    assert!(
        reached(&pool).await.is_empty(),
        "a merged pull request is not this one"
    );

    item(
        &pool,
        PR,
        "pr",
        "PAY-231: Retry failed SEPA payouts",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": "feature/PAY-231-backoff"}, "base": {"ref": "main"}}),
    )
    .await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap()
            .as_deref(),
        Some(PR)
    );
    assert_eq!(reached(&pool).await, vec![PR.to_owned()]);
}

/// A sent `CreatePullRequest` whose receipt named nothing is found by its head
/// **and base** (#1952): a colleague's pull request open from the same head
/// into a release branch is not this write's, and is not linked; the one into
/// the write's own base is.
#[tokio::test]
async fn a_new_pull_request_the_receipt_did_not_name_is_not_one_into_another_base() {
    let pool = corpus("arrival_pr_other_base").await;
    item(
        &pool,
        "gitea:tidewater/payout-service#149",
        "pr",
        "PAY-231: backport to release/2026.08",
        serde_json::json!({
            "merged": false, "state": "open",
            "head": {"ref": "feature/PAY-231-backoff"}, "base": {"ref": "release/2026.08"}
        }),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap(),
        None,
        "a pull request into another base was linked as this write's"
    );
    assert!(reached(&pool).await.is_empty());

    item(
        &pool,
        PR,
        "pr",
        "PAY-231: Retry failed SEPA payouts",
        serde_json::json!({
            "merged": false, "state": "open",
            "head": {"ref": "feature/PAY-231-backoff"}, "base": {"ref": "main"}
        }),
    )
    .await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap()
            .as_deref(),
        Some(PR)
    );
    assert_eq!(reached(&pool).await, vec![PR.to_owned()]);
}

/// The pull request the receipt named is linked (#1949), and not another one
/// open on the same head into a different base, which the head alone cannot
/// tell from it. Until the named one is mirrored nothing is linked: a named
/// pull request is waited for, never looked for by its head.
#[tokio::test]
async fn a_new_pull_request_the_receipt_named_is_linked_and_not_another_open_on_its_head() {
    let pool = corpus("arrival_pr_named").await;
    let open_on_head = serde_json::json!({
        "merged": false, "state": "open", "head": {"ref": "feature/PAY-231-backoff"}
    });
    // A colleague's pull request from the same head into a release branch.
    item(
        &pool,
        "gitea:tidewater/payout-service#149",
        "pr",
        "PAY-231: backport to release/2026.08",
        open_on_head.clone(),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, Some("tidewater/payout-service#150"))
        .await
        .unwrap();
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap(),
        None,
        "the pull request the receipt named is not mirrored yet, and #149 is not it"
    );
    assert!(reached(&pool).await.is_empty());

    item(
        &pool,
        PR,
        "pr",
        "PAY-231: Retry failed SEPA payouts",
        open_on_head,
    )
    .await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap()
            .as_deref(),
        Some(PR),
        "the named pull request, although #149 is open on the same head"
    );
    assert_eq!(reached(&pool).await, vec![PR.to_owned()]);
}

/// A named pull request already merged when the sync brings it in is linked
/// all the same (#1949): the receipt says it is this write's own, so it is
/// not the older one on a re-used head that the head read refuses (#359).
#[tokio::test]
async fn a_named_pull_request_merged_before_its_sync_is_linked() {
    let pool = corpus("arrival_pr_named_merged").await;
    item(
        &pool,
        PR,
        "pr",
        "PAY-231: Retry failed SEPA payouts",
        serde_json::json!({
            "merged": true, "state": "closed", "head": {"ref": "feature/PAY-231-backoff"}
        }),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, Some("tidewater/payout-service#150"))
        .await
        .unwrap();
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap()
            .as_deref(),
        Some(PR)
    );
    assert_eq!(reached(&pool).await, vec![PR.to_owned()]);
}

/// The id a receipt named is taken only as the kind the write makes: a mirror
/// item of another kind under that id is not the pull request, and nothing is
/// linked (#1949).
#[tokio::test]
async fn a_named_id_the_mirror_holds_as_another_kind_is_not_linked() {
    let pool = corpus("arrival_pr_named_kind").await;
    item(
        &pool,
        PR,
        "ticket",
        "PAY-231: Retry failed SEPA payouts",
        serde_json::json!({}),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, Some("tidewater/payout-service#150"))
        .await
        .unwrap();
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET)
            .await
            .unwrap(),
        None
    );
    assert!(reached(&pool).await.is_empty());
}

/// A write that did not land links nothing, even with its entity mirrored:
/// still waiting, refused (the branch already existed), or discarded.
#[tokio::test]
async fn a_write_that_did_not_land_links_nothing() {
    let pool = corpus("arrival_refused").await;
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    let waiting = queued(&pool, new_branch()).await;
    let refused = queued(&pool, new_branch()).await;
    knobas_core::write_queue::refuse(&pool, refused, "branch already exists")
        .await
        .unwrap();
    let discarded = queued(&pool, new_branch()).await;
    knobas_core::write_queue::discard(&pool, discarded)
        .await
        .unwrap();
    for id in [waiting, refused, discarded, 999_999] {
        assert_eq!(
            link_on_arrival(&pool, &declarations(), id, TICKET)
                .await
                .unwrap(),
            None,
            "write {id}"
        );
    }
    assert!(reached(&pool).await.is_empty());
}

// -- the intent held with the write (#1050) ---------------------------------

/// What `link_to` still says on write `id`.
async fn owed(pool: &PgPool, id: i64) -> Option<String> {
    sqlx::query_scalar("select link_to from knobas.write_queue where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The issue's first criterion: a `CreateBranch` that waits (its source
/// down), lands on a later tick, and is linked by the first sync that brings
/// the branch in -- with no task alive from the send, since the intent is the
/// row's. A link the reader then removes is not drawn again.
#[tokio::test]
async fn a_branch_that_lands_on_a_later_tick_is_linked_by_the_sync_that_brings_it_in() {
    let pool = corpus("arrival_later_tick").await;
    let id = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, id, TICKET).await.unwrap();
    knobas_core::write_queue::wait(
        &pool,
        id,
        knobas_core::write_queue::WaitReason::Unreachable,
        Some("connection refused"),
    )
    .await
    .unwrap();
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    assert!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap()
            .is_empty(),
        "a write still waiting links nothing, even with a same-named branch mirrored"
    );
    assert!(reached(&pool).await.is_empty());

    // A later tick sends it; this sync brings nothing for it yet.
    for statement in [
        "delete from sync.item where entity_id = $1",
        "delete from knobas.entity where id = $1",
    ] {
        sqlx::query(statement)
            .bind(BRANCH)
            .execute(&pool)
            .await
            .unwrap();
    }
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();
    assert!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(owed(&pool, id).await.as_deref(), Some(TICKET), "still owed");

    // The next sync brings the branch in.
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    assert!(
        link_arrivals(&pool, &declarations(), "jira")
            .await
            .unwrap()
            .is_empty(),
        "another source's sync resolves nothing of gitea's"
    );
    assert_eq!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap(),
        vec![BRANCH.to_owned()]
    );
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
    assert_eq!(owed(&pool, id).await, None, "drawn, so no longer owed");

    // The reader removes the link; the next sync leaves it removed.
    sqlx::query("update knobas.link set deleted_at = now() where from_id = $1")
        .bind(BRANCH)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(reached(&pool).await.is_empty());
}

/// The second criterion: a discarded or refused write links nothing, however
/// many syncs bring a same-named branch in; a discard drops the intent. So
/// does a job that has left the mirror, and a write sent so long ago that a
/// branch of that name turning up now is not what it made.
#[tokio::test]
async fn a_discarded_refused_orphaned_or_stale_write_links_nothing() {
    let pool = corpus("arrival_never").await;
    let refused = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, refused, TICKET).await.unwrap();
    knobas_core::write_queue::refuse(&pool, refused, "branch already exists")
        .await
        .unwrap();
    let discarded = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, discarded, TICKET).await.unwrap();
    knobas_core::write_queue::discard(&pool, discarded)
        .await
        .unwrap();
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;
    for _ in 0..2 {
        assert!(
            link_arrivals(&pool, &declarations(), "gitea")
                .await
                .unwrap()
                .is_empty()
        );
    }
    assert!(reached(&pool).await.is_empty());
    assert_eq!(owed(&pool, discarded).await, None, "a discard drops it");

    // Sent, but the job is gone from the mirror: the intent is dropped and
    // stays dropped if a ticket of that id comes back.
    let gone = "jira:PAY-404";
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'gone')")
        .bind(gone)
        .execute(&pool)
        .await
        .unwrap();
    let orphan = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, orphan, gone).await.unwrap();
    knobas_core::write_queue::sent(&pool, orphan, None)
        .await
        .unwrap();
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(gone)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(owed(&pool, orphan).await, None);

    // Sent eight days ago and never seen: dropped, not linked.
    let stale = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, stale, TICKET).await.unwrap();
    knobas_core::write_queue::sent(&pool, stale, None)
        .await
        .unwrap();
    sqlx::query(
        "update knobas.write_queue set settled_at = now() - interval '8 days' where id = $1",
    )
    .bind(stale)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        link_arrivals(&pool, &declarations(), "gitea")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(reached(&pool).await.is_empty());
    assert_eq!(owed(&pool, stale).await, None);
}

/// The wire: the scheduler's events, wrapped the way `sources::start` wraps
/// them, resolve what is owed when a run of the source ends, and pass every
/// event on. A run starting resolves nothing.
#[tokio::test]
async fn the_end_of_a_run_resolves_what_the_source_owes() {
    use knobas_sync::scheduler::SyncEvents;

    struct Told(std::sync::Arc<std::sync::Mutex<Vec<bool>>>);
    impl SyncEvents for Told {
        fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
            self.0.lock().unwrap().push(status.running);
        }
        fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
        fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    }

    let pool = corpus("arrival_run_end").await;
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let background = knobas_sync::background::Background::new();
    let events = ArrivalLinks::new(
        Told(std::sync::Arc::clone(&told)),
        pool.clone(),
        std::sync::Arc::new(knobas_app::sources::Registry::for_process()),
        background.clone(),
    );
    let id = queued(&pool, new_branch()).await;
    hold_arrival_link(&pool, id, TICKET).await.unwrap();
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();
    item(
        &pool,
        BRANCH,
        "branch",
        "feature/PAY-231-backoff",
        serde_json::json!({}),
    )
    .await;

    let started = knobas_sync::SourceSyncStatus::started("gitea", 1);
    events.sync_state(started.clone());
    // A resolution is a task in the set, spawned inside `sync_state`: none is
    // there, and none ran to the end, which would have linked the branch. So
    // nothing the start did can link it later (#1927). Before, a 300 ms look
    // stood for this, and a resolution that linked later than the look went
    // unseen.
    assert_eq!(
        background.running(),
        0,
        "a run starting spawned a resolution"
    );
    assert!(
        reached(&pool).await.is_empty(),
        "a run starting links nothing"
    );

    events.sync_state(knobas_sync::SourceSyncStatus {
        running: false,
        ..started
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while reached(&pool).await.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the run's end links the branch");
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
    assert_eq!(*told.lock().unwrap(), vec![true, false], "both passed on");
}

/// Refused before anything is queued: a write that makes nothing to link, a
/// job that is not an id, and a job the mirror does not hold. A new branch,
/// pull request or page (#1121) may carry one.
#[tokio::test]
async fn only_a_new_branch_pull_request_page_or_ticket_is_linked_and_only_to_a_mirrored_job() {
    let pool = corpus("arrival_check").await;
    for made in [new_branch(), new_pull_request(), new_page(), new_ticket()] {
        arrival_linkable(&pool, &made, TICKET).await.unwrap();
    }

    let comment = serde_json::json!({ "Comment": { "entity": PR, "body": "hi" } });
    let refused = arrival_linkable(&pool, &comment, TICKET).await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid, "{refused:?}");
    let refused = arrival_linkable(&pool, &new_branch(), "PAY-231")
        .await
        .unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid, "{refused:?}");
    let refused = arrival_linkable(&pool, &new_branch(), "jira:PAY-999")
        .await
        .unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::NotFound, "{refused:?}");
    let count: i64 = sqlx::query_scalar("select count(*) from knobas.write_queue")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "a check queues nothing");
}

// -- *Link it to <job>* on *New page* (#862, #1121) --------------------------

const WIKI: &str = "wiki";
const PARENT: &str = "wiki:100";
const NEW_PAGE: &str = "wiki:4242";

fn new_page() -> serde_json::Value {
    serde_json::json!({ "CreatePage": {
        "parent": PARENT, "space": "ENG", "title": "Retry runbook", "body": "<p>Steps</p>"
    }})
}

/// A `CreatePage` as `queue` leaves it, under its parent.
async fn queued_page(pool: &PgPool) -> i64 {
    knobas_core::write_queue::queue(
        pool,
        WIKI,
        &EntityRef::parse(PARENT).unwrap(),
        "create_page",
        new_page(),
        serde_json::json!({}),
    )
    .await
    .unwrap()
    .id
}

/// The issue's criterion: a new page whose write waits (its source down) and
/// lands on a later tick is linked to its job by the first sync that brings
/// the page in, by the `documents` relation *Link it to <job>* draws. The page
/// is the one the write's settle named (`remote_id`), so a page of the same
/// title that some other hand made is not taken for it.
#[tokio::test]
async fn a_new_page_that_lands_on_a_later_tick_is_linked_to_its_job() {
    let pool = corpus("arrival_page").await;
    item(&pool, PARENT, "page", "Payments", serde_json::json!({})).await;
    let id = queued_page(&pool).await;
    hold_arrival_link(&pool, id, TICKET).await.unwrap();
    knobas_core::write_queue::wait(
        &pool,
        id,
        knobas_core::write_queue::WaitReason::Unreachable,
        Some("connection refused"),
    )
    .await
    .unwrap();
    item(
        &pool,
        "wiki:999",
        "page",
        "Retry runbook",
        serde_json::json!({}),
    )
    .await;
    assert!(
        link_arrivals(&pool, &declarations(), WIKI)
            .await
            .unwrap()
            .is_empty(),
        "a waiting write links nothing"
    );

    // A later tick sends it; Confluence names the page it made.
    knobas_core::write_queue::sent(&pool, id, Some("4242"))
        .await
        .unwrap();
    assert!(
        link_arrivals(&pool, &declarations(), WIKI)
            .await
            .unwrap()
            .is_empty(),
        "the page is not mirrored yet, and the same-titled one is not it"
    );
    assert_eq!(owed(&pool, id).await.as_deref(), Some(TICKET), "still owed");

    item(
        &pool,
        NEW_PAGE,
        "page",
        "Retry runbook",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        link_arrivals(&pool, &declarations(), WIKI).await.unwrap(),
        vec![NEW_PAGE.to_owned()]
    );
    assert_eq!(
        relations(&pool, NEW_PAGE).await,
        vec![("documents".to_owned(), true)]
    );
    assert!(relations(&pool, "wiki:999").await.is_empty());
    assert_eq!(owed(&pool, id).await, None, "drawn, so no longer owed");
}

// -- *Create ticket* on a failed run (#1451) ----------------------------------

const RUN: &str = "teamcity:build:Payout_IntegrationTests-1187";
const NEW_TICKET: &str = "jira:PAY-342";
const FAILED_TITLE: &str = "sepa_retry_backs_off fails in Payout_IntegrationTests #1187";

fn new_ticket() -> serde_json::Value {
    serde_json::json!({ "CreateTicket": {
        "entity": "jira:PAY", "title": FAILED_TITLE, "body": "assertion failed", "ticket_type": "Task"
    }})
}

/// A `CreateTicket` as `queue` leaves it, in its project.
async fn queued_ticket(pool: &PgPool) -> i64 {
    knobas_core::write_queue::queue(
        pool,
        "jira",
        &EntityRef::parse("jira:PAY").unwrap(),
        "create_ticket",
        new_ticket(),
        serde_json::json!({}),
    )
    .await
    .unwrap()
    .id
}

/// When the source says it last changed the item.
async fn changed(pool: &PgPool, id: &str, ago: &str) {
    sqlx::query("update sync.item set item_updated_at = now() - $2::interval where entity_id = $1")
        .bind(id)
        .bind(ago)
        .execute(pool)
        .await
        .unwrap();
}

/// The issue's link criterion: a ticket filed from a failed run is linked to
/// the run, `related`, by the first sync that brings the ticket in. Here the
/// source named no key in its receipt (a write sent before #1754, or a source
/// that answers none), so the ticket is found by its title in the write's
/// source; one of the same title that the source last changed before the
/// write was queued (the same failure filed last week) is not it.
#[tokio::test]
async fn a_new_ticket_is_linked_to_the_run_it_was_filed_from_once_it_is_mirrored() {
    let pool = corpus("arrival_ticket").await;
    item(
        &pool,
        RUN,
        "build",
        "Payout_IntegrationTests #1187",
        serde_json::json!({}),
    )
    .await;
    item(
        &pool,
        "jira:PAY-300",
        "ticket",
        FAILED_TITLE,
        serde_json::json!({}),
    )
    .await;
    changed(&pool, "jira:PAY-300", "7 days").await;
    arrival_linkable(&pool, &new_ticket(), RUN).await.unwrap();
    let id = queued_ticket(&pool).await;
    hold_arrival_link(&pool, id, RUN).await.unwrap();
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();

    assert!(
        link_arrivals(&pool, &declarations(), "jira")
            .await
            .unwrap()
            .is_empty(),
        "the ticket is not mirrored yet, and last week's of the same title is not it"
    );
    assert_eq!(owed(&pool, id).await.as_deref(), Some(RUN), "still owed");

    item(
        &pool,
        NEW_TICKET,
        "ticket",
        FAILED_TITLE,
        serde_json::json!({}),
    )
    .await;
    changed(&pool, NEW_TICKET, "0 seconds").await;
    assert_eq!(
        link_arrivals(&pool, &declarations(), "jira").await.unwrap(),
        vec![NEW_TICKET.to_owned()]
    );
    let drawn: Vec<(String, String, bool)> = sqlx::query_as(
        "select from_id, relation, confirmed_at is not null from knobas.link
          where to_id = $1 and deleted_at is null order by from_id",
    )
    .bind(RUN)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        drawn,
        vec![(NEW_TICKET.to_owned(), "related".to_owned(), true)]
    );
    assert_eq!(owed(&pool, id).await, None, "drawn, so no longer owed");
}

/// #1754: a source that named the ticket it made is taken at its word. The
/// receipt says `PAY-342`, so a ticket of the same title that the source
/// changed later (the same failure filed twice, a project that already holds
/// that summary) is not taken for it, and neither is the newest same-titled
/// ticket while `PAY-342` is not mirrored yet: a named ticket waits for its
/// sync and never falls back to the title.
#[tokio::test]
async fn a_new_ticket_the_receipt_named_is_linked_and_not_a_newer_one_of_the_same_title() {
    let pool = corpus("arrival_ticket_named").await;
    item(
        &pool,
        RUN,
        "build",
        "Payout_IntegrationTests #1187",
        serde_json::json!({}),
    )
    .await;
    let id = queued_ticket(&pool).await;
    hold_arrival_link(&pool, id, RUN).await.unwrap();
    knobas_core::write_queue::sent(&pool, id, Some("PAY-342"))
        .await
        .unwrap();

    // Someone files the same failure a moment later, and it is mirrored first.
    item(
        &pool,
        "jira:PAY-343",
        "ticket",
        FAILED_TITLE,
        serde_json::json!({}),
    )
    .await;
    changed(&pool, "jira:PAY-343", "0 seconds").await;
    assert!(
        link_arrivals(&pool, &declarations(), "jira")
            .await
            .unwrap()
            .is_empty(),
        "the ticket the receipt named is not mirrored yet, and a same-titled one is not it"
    );
    assert_eq!(owed(&pool, id).await.as_deref(), Some(RUN), "still owed");

    item(
        &pool,
        NEW_TICKET,
        "ticket",
        FAILED_TITLE,
        serde_json::json!({}),
    )
    .await;
    changed(&pool, NEW_TICKET, "1 minute").await;
    assert_eq!(
        link_arrivals(&pool, &declarations(), "jira").await.unwrap(),
        vec![NEW_TICKET.to_owned()],
        "the named ticket, although PAY-343 is newer and titled the same"
    );
    let drawn: Vec<(String, String, bool)> = sqlx::query_as(
        "select from_id, relation, confirmed_at is not null from knobas.link
          where to_id = $1 and deleted_at is null order by from_id",
    )
    .bind(RUN)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        drawn,
        vec![(NEW_TICKET.to_owned(), "related".to_owned(), true)]
    );
    assert_eq!(owed(&pool, id).await, None, "drawn, so no longer owed");
}
