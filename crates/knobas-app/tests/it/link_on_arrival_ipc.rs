//! *Link to <job>* on *New branch…* and *Link the PR to <key>* on the compare
//! page (#984): `submit_write`'s `link_to`. The branch or pull request a
//! queued write makes has no id until the source mints one and a sync brings
//! it in (#707), so the link is drawn after the write has been sent and the
//! source re-read, against what the mirror then holds: the branch by its name
//! in the repository, the open pull request on the head.
//!
//! Over a real mirror, with the queue rows written the way the flush writes
//! them (`knobas_core::write_queue`); what Gitea does with the write is
//! `just gitea-live`'s to witness (#707, #822).

use knobas_app::IpcErrorCode;
use knobas_app::sources::write_queue::{arrival_linkable, link_on_arrival};
use knobas_core::entity::EntityRef;
use knobas_core::payload::{Declarations, KindPaths, PayloadPath};
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
    item(&pool, TICKET, "ticket", "Retry failed SEPA payouts", serde_json::json!({})).await;
    item(&pool, REPO, "repo", "tidewater/payout-service", serde_json::json!({})).await;
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
        .map(|k| if k == "CreateBranch" { "create_branch" } else { "create_pull_request" })
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

/// Gitea's declared merged flag, as `start_work`'s look-up reads it.
fn declarations() -> Declarations {
    Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "pr".to_owned(),
            merged: vec![PayloadPath::of(["merged"])],
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
    knobas_core::write_queue::sent(&pool, id, None).await.unwrap();

    assert_eq!(link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap(), None);
    assert!(reached(&pool).await.is_empty(), "nothing mirrored, nothing linked");

    item(&pool, BRANCH, "branch", "feature/PAY-231-backoff", serde_json::json!({})).await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap().as_deref(),
        Some(BRANCH)
    );
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
    assert_eq!(relations(&pool, BRANCH).await, vec![("related".to_owned(), true)]);
    // Twice is still one link.
    link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap();
    assert_eq!(relations(&pool, BRANCH).await.len(), 1);
}

/// The key in the branch's name has already raised a proposal (the
/// `branch_name_key` rule, `related`): the box confirms it rather than
/// drawing a second edge beside it.
#[tokio::test]
async fn the_proposal_the_branch_name_raised_is_confirmed_not_doubled() {
    let pool = corpus("arrival_proposal").await;
    item(&pool, BRANCH, "branch", "feature/PAY-231-backoff", serde_json::json!({})).await;
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
    knobas_core::write_queue::sent(&pool, id, None).await.unwrap();

    link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap();
    assert_eq!(relations(&pool, BRANCH).await, vec![("related".to_owned(), true)]);
    assert_eq!(reached(&pool).await, vec![BRANCH.to_owned()]);
}

/// A sent `CreatePullRequest` links the pull request open on its head; one
/// on the head that is already merged is not this write's, and is not
/// linked.
#[tokio::test]
async fn a_new_pull_request_is_linked_once_it_is_mirrored_open_on_its_head() {
    let pool = corpus("arrival_pr").await;
    item(
        &pool,
        "gitea:tidewater/payout-service#7",
        "pr",
        "an older one",
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": "feature/PAY-231-backoff"}}),
    )
    .await;
    let id = queued(&pool, new_pull_request()).await;
    knobas_core::write_queue::sent(&pool, id, None).await.unwrap();
    assert_eq!(link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap(), None);
    assert!(reached(&pool).await.is_empty(), "a merged pull request is not this one");

    item(
        &pool,
        PR,
        "pr",
        "PAY-231: Retry failed SEPA payouts",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": "feature/PAY-231-backoff"}}),
    )
    .await;
    assert_eq!(
        link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap().as_deref(),
        Some(PR)
    );
    assert_eq!(reached(&pool).await, vec![PR.to_owned()]);
}

/// A write that did not land links nothing, even with its entity mirrored:
/// still waiting, refused (the branch already existed), or discarded.
#[tokio::test]
async fn a_write_that_did_not_land_links_nothing() {
    let pool = corpus("arrival_refused").await;
    item(&pool, BRANCH, "branch", "feature/PAY-231-backoff", serde_json::json!({})).await;
    let waiting = queued(&pool, new_branch()).await;
    let refused = queued(&pool, new_branch()).await;
    knobas_core::write_queue::refuse(&pool, refused, "branch already exists")
        .await
        .unwrap();
    let discarded = queued(&pool, new_branch()).await;
    knobas_core::write_queue::discard(&pool, discarded).await.unwrap();
    for id in [waiting, refused, discarded, 999_999] {
        assert_eq!(
            link_on_arrival(&pool, &declarations(), id, TICKET).await.unwrap(),
            None,
            "write {id}"
        );
    }
    assert!(reached(&pool).await.is_empty());
}

/// Refused before anything is queued: a write that makes nothing to link, a
/// job that is not an id, and a job the mirror does not hold.
#[tokio::test]
async fn only_a_new_branch_or_pull_request_is_linked_and_only_to_a_mirrored_job() {
    let pool = corpus("arrival_check").await;
    arrival_linkable(&pool, &new_branch(), TICKET).await.unwrap();
    arrival_linkable(&pool, &new_pull_request(), TICKET).await.unwrap();

    let comment = serde_json::json!({ "Comment": { "entity": PR, "body": "hi" } });
    let refused = arrival_linkable(&pool, &comment, TICKET).await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid, "{refused:?}");
    let refused = arrival_linkable(&pool, &new_branch(), "PAY-231").await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid, "{refused:?}");
    let refused = arrival_linkable(&pool, &new_branch(), "jira:PAY-999").await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::NotFound, "{refused:?}");
    let count: i64 = sqlx::query_scalar("select count(*) from knobas.write_queue")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "a check queues nothing");
}
