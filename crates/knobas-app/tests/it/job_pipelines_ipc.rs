//! *This job* on Builds (#850, R-9.1, R-9.2): `job_pipelines`, the reasons a
//! pipeline is on a job, over the embedded PostgreSQL with the declarations
//! the compiled-in adapters make. Every reason is paired with a pipeline one
//! step past its edge, which the job must not show.

use knobas_app::commands::entity::{JobPipeline, job_pipelines_inner};
use knobas_app::sources::Registry;
use knobas_core::entity::EntityRef;
use knobas_core::link::{self, Origin as LinkOrigin};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // The declarations' stored copy, written as bring-up writes it (ADR-0019):
    // what fills `sync.item_revision` for the PR-check reason.
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()
}

async fn configured(pool: &PgPool, adapter: &str) -> String {
    let id = format!("{adapter}{}", unique());
    crate::support::source_config(pool, &id, adapter, adapter, &serde_json::json!({})).await;
    id
}

/// One mirror item of `source`, titled `title`.
async fn item(
    pool: &PgPool,
    source: &str,
    key: &str,
    kind: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn draw(pool: &PgPool, from: &str, to: &str) {
    link::create(
        pool,
        &EntityRef::parse(from).unwrap(),
        &EntityRef::parse(to).unwrap(),
        "relates",
        LinkOrigin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
}

async fn config(pool: &PgPool, tc: &str, id: &str) -> String {
    item(
        pool,
        tc,
        &format!("buildType:{id}"),
        "build_config",
        id,
        serde_json::json!({ "id": id, "name": id, "projectId": "Payout", "projectName": "Payout" }),
    )
    .await
}

async fn run(pool: &PgPool, tc: &str, n: i64, config: &str, branch: &str, sha: &str) -> String {
    item(
        pool,
        tc,
        &format!("build:{n}"),
        "build",
        &format!("{config} #{n}"),
        serde_json::json!({
            "id": n, "number": n.to_string(), "buildTypeId": config, "state": "finished",
            "status": "SUCCESS", "branchName": branch,
            "revisions": { "revision": [{ "version": sha }] }
        }),
    )
    .await
}

/// Move run `n` of `tc` onto `branch`, as a later sync would, and read the
/// job's pipelines again.
async fn rebranch(pool: &PgPool, tc: &str, n: i64, branch: &str, job: &str) -> Vec<JobPipeline> {
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{branchName}', to_jsonb($1::text))
          where entity_id = $2",
    )
    .bind(branch)
    .bind(EntityRef::new(tc, &format!("build:{n}")).to_string())
    .execute(pool)
    .await
    .unwrap();
    job_pipelines_inner(pool, &Registry::for_process(), job)
        .await
        .unwrap()
}

fn reasons(all: &[JobPipeline], id: &str) -> Option<serde_json::Value> {
    all.iter()
        .find(|p| p.pipeline_id == id)
        .map(|p| serde_json::to_value(&p.reasons).unwrap())
}

#[tokio::test]
async fn a_pipeline_is_on_a_job_for_its_branch_its_pr_check_or_a_link() {
    let pool = pool().await;
    let jira = configured(&pool, "jira").await;
    let gitea = configured(&pool, "gitea").await;
    let tc = configured(&pool, "teamcity").await;
    let head_sha = format!("{}{}{}{}", unique(), unique(), unique(), unique());

    let ticket = item(
        &pool,
        &jira,
        "PAY-1",
        "ticket",
        "PAY-1",
        serde_json::json!({
            "key": "PAY-1", "fields": { "issuetype": { "name": "Story" } }
        }),
    )
    .await;
    let branch = item(
        &pool,
        &gitea,
        "acme/svc/branch/feature/PAY-1-x",
        "branch",
        "feature/PAY-1-x",
        serde_json::json!({ "name": "feature/PAY-1-x" }),
    )
    .await;
    let pr = item(
        &pool,
        &gitea,
        "acme/svc#7",
        "pr",
        "Retry",
        serde_json::json!({
            "number": 7, "head": { "ref": "feature/PAY-1-pr", "sha": head_sha }
        }),
    )
    .await;
    draw(&pool, &ticket, &branch).await;
    draw(&pool, &ticket, &pr).await;

    // A run on the job's branch.
    let on_branch = config(&pool, &tc, "OnBranch").await;
    run(&pool, &tc, 1, "OnBranch", "feature/PAY-1-x", "aaa").await;
    // A run on the pull request's branch, spelt as a ref, at its head commit.
    let on_pr = config(&pool, &tc, "OnPr").await;
    run(
        &pool,
        &tc,
        2,
        "OnPr",
        "refs/heads/feature/PAY-1-pr",
        &head_sha,
    )
    .await;
    // A run the job reaches through its pull request.
    let via_pr = config(&pool, &tc, "ViaPr").await;
    let linked_run = run(&pool, &tc, 3, "ViaPr", "main", "bbb").await;
    draw(&pool, &pr, &linked_run).await;
    // A pipeline linked to the ticket itself.
    let linked = config(&pool, &tc, "Linked").await;
    draw(&pool, &ticket, &linked).await;
    // One step past every edge: a branch that only starts like the job's, a
    // commit that is not the head, and nothing linked.
    let elsewhere = config(&pool, &tc, "Elsewhere").await;
    run(&pool, &tc, 4, "Elsewhere", "feature/PAY-1-x-other", "ccc").await;
    run(&pool, &tc, 5, "Elsewhere", "main", "ddd").await;
    // A run on the job's branch that has since been deleted at the source.
    let gone = config(&pool, &tc, "Gone").await;
    let gone_run = run(&pool, &tc, 6, "Gone", "feature/PAY-1-x", "eee").await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone_run)
        .execute(&pool)
        .await
        .unwrap();

    let all = job_pipelines_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &ticket,
    )
    .await
    .unwrap();
    assert_eq!(
        reasons(&all, &on_branch),
        Some(serde_json::json!([{ "kind": "branch", "branch": "feature/PAY-1-x" }]))
    );
    assert_eq!(
        reasons(&all, &on_pr),
        Some(serde_json::json!([
            { "kind": "branch", "branch": "feature/PAY-1-pr" },
            { "kind": "pr_check", "pr": pr }
        ])),
        "your branch first, then the PR check"
    );
    assert_eq!(
        reasons(&all, &via_pr),
        Some(serde_json::json!([{ "kind": "pr_check", "pr": pr }]))
    );
    assert_eq!(
        reasons(&all, &linked),
        Some(
            serde_json::json!([{ "kind": "linked", "step": linked, "origin": { "kind": "own" } }])
        )
    );
    assert_eq!(reasons(&all, &elsewhere), None);
    assert_eq!(reasons(&all, &gone), None, "a tombstoned run builds nothing");
    assert_eq!(all.len(), 4, "each pipeline once: {all:?}");

    // A run's branch is stored as the run is written (#2192): a later sync
    // that moves a run onto the job's branch puts its pipeline on the job,
    // and moving it off again takes it away.
    let moved = rebranch(&pool, &tc, 4, "refs/heads/feature/PAY-1-x", &ticket).await;
    assert_eq!(
        reasons(&moved, &elsewhere),
        Some(serde_json::json!([{ "kind": "branch", "branch": "feature/PAY-1-x" }])),
        "a run moved onto the job's branch: {moved:?}"
    );
    let back = rebranch(&pool, &tc, 4, "feature/PAY-1-x-other", &ticket).await;
    assert_eq!(reasons(&back, &elsewhere), None, "and off it again");

    // A ticket that reaches nothing has no pipelines.
    let lone = item(
        &pool,
        &jira,
        "PAY-9",
        "ticket",
        "PAY-9",
        serde_json::json!({ "key": "PAY-9" }),
    )
    .await;
    assert!(
        job_pipelines_inner(&pool, &knobas_app::sources::Registry::for_process(), &lone)
            .await
            .unwrap()
            .is_empty()
    );
}

/// A sub-task shows its parent ticket's pipelines (R-7.3), labelled with the
/// parent, as every other *This job* does.
#[tokio::test]
async fn a_sub_task_shows_the_pipelines_its_parent_links() {
    let pool = pool().await;
    let jira = configured(&pool, "jira").await;
    let tc = configured(&pool, "teamcity").await;
    let parent = item(
        &pool,
        &jira,
        "PAY-2",
        "ticket",
        "PAY-2",
        serde_json::json!({
            "key": "PAY-2", "fields": { "issuetype": { "name": "Story" } }
        }),
    )
    .await;
    let child = item(&pool, &jira, "PAY-3", "ticket", "PAY-3", serde_json::json!({
        "key": "PAY-3", "fields": { "parent": { "key": "PAY-2" }, "issuetype": { "name": "Sub-task" } }
    }))
    .await;
    let linked = config(&pool, &tc, "ParentLinked").await;
    draw(&pool, &parent, &linked).await;

    let all = job_pipelines_inner(&pool, &knobas_app::sources::Registry::for_process(), &child)
        .await
        .unwrap();
    assert_eq!(
        reasons(&all, &linked),
        Some(serde_json::json!([{
            "kind": "linked", "step": parent, "origin": { "kind": "parent", "ticket": parent }
        }]))
    );
}
