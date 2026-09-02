//! The projects a corpus shows (#208), against a real PostgreSQL.
//!
//! The switcher cannot derive this from a room's own scan -- that read is a
//! window over the newest items and explicitly "not a census" (`Room.svelte`),
//! so a quiet project would silently have no room. What is asserted here is
//! the census: which projects the *live* corpus shows, per source, and what
//! the payload read does when it misses.
//!
//! Every test gets a database of its own, the trade `mini_board.rs` and
//! `contexts.rs` record: this read is a pass over the whole live corpus, so a
//! shared database would let one test's fixture decide another's answer.

use knobas_core::entity::EntityRef;
use knobas_core::project::{self, Project};
use sqlx::PgPool;

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("projects")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// A Jira Data Center issue's shape, as far as this read cares: the `project`
/// container under `fields`, carrying whatever the caller hands it.
fn jira(project: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "fields": { "project": project } })
}

/// A TeamCity build's shape: the project is named on the `buildType` the build
/// ran, in TeamCity's own two words.
fn teamcity(project: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "buildType": project })
}

/// A TeamCity build configuration's shape: the record *is* the `buildType`
/// object (`knobas_source_teamcity::map::build_config_item` stores it
/// verbatim), so it spells the project at the top level, in the same two
/// words a build spells one level down. The caller hands the project half;
/// the identity half is fixed so a fixture cannot mistake one for the other.
fn build_config(project: serde_json::Value) -> serde_json::Value {
    let mut raw = serde_json::json!({ "id": "Payout_Build", "name": "Build" });
    let project = project
        .as_object()
        .expect("the project half of a configuration is an object");
    raw.as_object_mut()
        .expect("the identity half is an object")
        .extend(project.iter().map(|(k, v)| (k.clone(), v.clone())));
    raw
}

/// One live mirror item of the given kind, carrying the payload it is given.
async fn item(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(format!("{key} title"))
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
    .bind(format!("{key} title"))
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A live Jira ticket carrying a project container.
async fn ticket(pool: &PgPool, key: &str, project: serde_json::Value) -> String {
    item(pool, "jira", "ticket", key, jira(project)).await
}

/// A live TeamCity build configuration, under the kind name the adapter
/// declares for one (`KIND_BUILD_CONFIG`), carrying the project half of its
/// own record.
async fn configuration(pool: &PgPool, key: &str, project: serde_json::Value) -> String {
    item(pool, "teamcity", "build_config", key, build_config(project)).await
}

/// Mark an entity deleted at its source -- what a sweep or a purge does.
async fn tombstone(pool: &PgPool, id: &str) {
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// A configuration row for a source, turned on or off.
///
/// `sync.item` has no foreign key to `knobas.source_config` -- `run_once`
/// mirrors unconfigured sources -- so most fixtures here have no row at all,
/// and migration `0012`'s `coalesce(s.enabled, true)` leaves those visible.
/// This is what the *user turning a source off* looks like.
async fn configure(pool: &PgPool, source: &str, enabled: bool) {
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, $1, $1, 'http://localhost', 'pat', $2)",
    )
    .bind(source)
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
}

/// When the source last touched this item. Set explicitly wherever a test
/// asserts an order, so the assertion stands on the fixture rather than on how
/// fast the rows happened to be inserted.
async fn touched(pool: &PgPool, id: &str, at: &str) {
    sqlx::query("update sync.item set item_updated_at = $2::timestamptz where entity_id = $1")
        .bind(id)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
}

/// The answer as `(source, key, name)`, which is the whole of the DTO.
fn rows(projects: &[Project]) -> Vec<(&str, &str, Option<&str>)> {
    projects
        .iter()
        .map(|project| {
            (
                project.source_id.as_str(),
                project.key.as_str(),
                project.name.as_deref(),
            )
        })
        .collect()
}

/// The census: every project the live corpus shows, one row each however many
/// items carry it, ordered by source then key.
#[tokio::test]
async fn every_project_the_corpus_shows_is_reported_once() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    ticket(
        &pool,
        "PAY-2",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    ticket(
        &pool,
        "INT-1",
        serde_json::json!({ "key": "INT", "name": "Integrations" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "INT", Some("Integrations")),
            ("jira", "PAY", Some("Payout")),
        ],
        "one row per project, ordered by source then key"
    );
}

/// A source that spells it differently is one more `coalesce` arm in one place
/// (ADR-0007 requirement 2), and nothing anywhere else. TeamCity names the
/// project on the `buildType` a build ran, in its own two words; without this
/// the second arm would be untested code.
#[tokio::test]
async fn teamcitys_own_two_words_read_as_well_as_jiras() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    // The shape `knobas_source_teamcity` puts in `payload` verbatim.
    item(
        &pool,
        "teamcity",
        "build",
        "buildType:Payout_Build:1188",
        teamcity(serde_json::json!({
            "id": "Payout_Build",
            "name": "Build",
            "projectId": "Payout",
            "projectName": "Payout pipeline",
        })),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "PAY", Some("Payout")),
            ("teamcity", "Payout", Some("Payout pipeline")),
        ]
    );
}

/// The third spelling, kind-scoped (#232): a build configuration's record is
/// the `buildType` object itself, so it names its project at the top level,
/// and a project whose configurations have no synced build is reported
/// through them rather than through nothing. Before this arm such a project
/// had no room and no census line -- the absence #208 recorded as bounded.
#[tokio::test]
async fn a_build_configurations_own_top_level_words_are_read_for_its_kind() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Payout_Build",
        serde_json::json!({ "projectId": "Payout", "projectName": "Payout pipeline" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "PAY", Some("Payout")),
            ("teamcity", "Payout", Some("Payout pipeline")),
        ],
        "a configuration-only project is a project"
    );
}

/// Miss direction, for the third arm: a configuration whose top-level
/// `projectId` is absent, blank, whitespace-only or not a string contributes
/// no project -- the same four refusals the Jira path is pinned by below,
/// all through the one `string_at!`, so the new path cannot be laxer than
/// the old ones.
#[tokio::test]
async fn a_build_configuration_with_no_readable_project_contributes_nothing() {
    let pool = scratch().await;
    configuration(
        &pool,
        "buildType:Bare",
        serde_json::json!({ "projectName": "Payout pipeline" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Blank",
        serde_json::json!({ "projectId": "", "projectName": "Payout pipeline" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Spaces",
        serde_json::json!({ "projectId": "   ", "projectName": "Payout pipeline" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Object",
        serde_json::json!({ "projectId": { "id": "Payout" }, "projectName": "Payout pipeline" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Array",
        serde_json::json!({ "projectId": ["Payout"], "projectName": "Payout pipeline" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![], "none of the five is a project");
}

/// A configuration with a readable key and no readable name is reported by
/// its key, with the name `None` on the wire -- the same carriage the Jira
/// case has. A real server may send a `buildType` with no `projectName` at
/// all (`a_bare_build_configuration_still_maps` in the adapter).
#[tokio::test]
async fn a_build_configuration_with_no_readable_name_is_reported_by_its_key() {
    let pool = scratch().await;
    configuration(
        &pool,
        "buildType:Payout_Build",
        serde_json::json!({ "projectId": "Payout" }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Ledger_Deploy",
        serde_json::json!({ "projectId": "Ledger", "projectName": { "id": 3 } }),
    )
    .await;
    configuration(
        &pool,
        "buildType:Erp_Build",
        serde_json::json!({ "projectId": "Erp", "projectName": "   " }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("teamcity", "Erp", None),
            ("teamcity", "Ledger", None),
            ("teamcity", "Payout", None),
        ]
    );
}

/// A build and a configuration of the same project are one project, not two:
/// the two arms spell one fact in two places, and the census keys on the
/// value, not on which arm read it. The name is the configuration's here
/// because a configuration carries no date, so its `synced_at` stands in,
/// and the build is touched older -- so the newest readable name wins, the
/// way `a_renamed_project_stays_one_project_under_its_newest_name` says it
/// does.
#[tokio::test]
async fn a_build_and_its_configuration_are_one_project() {
    let pool = scratch().await;
    let build = item(
        &pool,
        "teamcity",
        "build",
        "build:1188",
        teamcity(serde_json::json!({
            "id": "Payout_Build",
            "projectId": "Payout",
            "projectName": "Payout",
        })),
    )
    .await;
    configuration(
        &pool,
        "buildType:Payout_Build",
        serde_json::json!({ "projectId": "Payout", "projectName": "Payout pipeline" }),
    )
    .await;
    touched(&pool, &build, "2026-08-01T09:00:00Z").await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![("teamcity", "Payout", Some("Payout pipeline"))],
        "one project however many of its records spell it"
    );
}

/// The third arm is scoped to the one kind whose record spells the project
/// at the top level, and no other kind reaches it (#232, ruled at triage).
///
/// A top-level `projectId` is a less distinctive path than the two
/// container-scoped ones, and a future adapter's incidental top-level
/// `projectId` must not silently open a room -- the risk #208 named when it
/// deferred the arm. So a *build* carrying the words at the top level, and a
/// record of some other source altogether, both contribute nothing; only a
/// `build_config` does. This is the test that pins the guard: with the guard
/// removed both rows below become projects.
#[tokio::test]
async fn a_top_level_project_id_on_any_other_kind_contributes_nothing() {
    let pool = scratch().await;
    item(
        &pool,
        "teamcity",
        "build",
        "build:1188",
        serde_json::json!({ "id": 1188, "projectId": "Payout", "projectName": "Payout pipeline" }),
    )
    .await;
    item(
        &pool,
        "gitea",
        "pr",
        "tidewater/payout-service#142",
        serde_json::json!({ "number": 142, "projectId": "tidewater", "projectName": "Tidewater" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![],
        "the top-level words are read for a build configuration and for nothing else"
    );
}

/// Miss direction: a key that is not a string contributes no project.
///
/// `->>` yields an object's or an array's *text form* rather than nothing, so
/// without the type check a record spelled some other way would open a room
/// called `{"id":3}` -- a guess dressed as an observation. The ticket stays in
/// *All work* and in its source's room, which is the whole of what absence
/// costs here.
#[tokio::test]
async fn a_project_key_that_is_not_a_string_contributes_nothing() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": { "id": 3 }, "name": "Payout" }),
    )
    .await;
    ticket(
        &pool,
        "PAY-2",
        serde_json::json!({ "key": ["PAY"], "name": "Payout" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![], "neither shape is a project");
}

/// One project is one row, whatever its items disagree about.
///
/// A project renamed upstream leaves older items carrying the older name, and
/// an item can carry the key with no name at all. Neither may split the
/// project in two: the switcher offers one room per project, and two rows for
/// one project would be two rooms holding the same work. The name reported is
/// the newest readable one -- a rename shows the new name, and a nameless item
/// does not erase a name another item does carry.
#[tokio::test]
async fn a_renamed_project_stays_one_project_under_its_newest_name() {
    let pool = scratch().await;
    let old = ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    let new = ticket(
        &pool,
        "PAY-2",
        serde_json::json!({ "key": "PAY", "name": "Payouts" }),
    )
    .await;
    let nameless = ticket(&pool, "PAY-3", serde_json::json!({ "key": "PAY" })).await;
    touched(&pool, &old, "2026-08-01T09:00:00Z").await;
    touched(&pool, &new, "2026-08-15T09:00:00Z").await;
    touched(&pool, &nameless, "2026-08-30T09:00:00Z").await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![("jira", "PAY", Some("Payouts"))]);
}

/// Miss direction: a blank key contributes no project.
///
/// The same refusal the type check is, for the other way a source can say
/// nothing while the path still leads somewhere. A room headed by an empty
/// string is not addressable by a person.
#[tokio::test]
async fn a_blank_project_key_contributes_nothing() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "", "name": "Payout" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![]);
}

/// Miss direction: a whitespace-only key contributes no project.
///
/// Its own test rather than a second case of the blank one: `''` is refused by
/// a plain emptiness check and `'   '` is not, so a read that lost the `btrim`
/// would still pass the test above while offering a room labelled with three
/// spaces.
#[tokio::test]
async fn a_whitespace_only_project_key_contributes_nothing() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "   ", "name": "Payout" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![]);
}

/// Miss direction: a record that carries no project at all contributes none.
///
/// The ordinary case rather than a malformed one -- a Gitea repository, a note
/// mirrored from a source with no such grouping (ADR-0010) -- and the one that
/// says the whole failure direction out loud: the item is in no project room,
/// and is still in *All work* and its source's room, because neither of those
/// is narrowed by a dimension it does not carry.
#[tokio::test]
async fn a_record_with_no_project_at_all_contributes_nothing() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    item(
        &pool,
        "gitea",
        "pr",
        "tidewater/payout-service#142",
        serde_json::json!({ "number": 142, "title": "Retry SEPA payouts" }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![("jira", "PAY", Some("Payout"))],
        "the source that spells no project shows none, and says nothing about the one that does"
    );
}

/// A project whose records carry a key and no readable name is still reported,
/// carried by its key.
///
/// The name is `None` on the wire rather than a copy of the key: what to
/// *label* the room with is the shell's decision (#209), and a name the source
/// never said would be indistinguishable from one it did.
#[tokio::test]
async fn a_project_with_no_readable_name_is_reported_by_its_key() {
    let pool = scratch().await;
    ticket(&pool, "PAY-1", serde_json::json!({ "key": "PAY" })).await;
    ticket(
        &pool,
        "INT-1",
        serde_json::json!({ "key": "INT", "name": { "id": 3 } }),
    )
    .await;
    ticket(
        &pool,
        "ERP-1",
        serde_json::json!({ "key": "ERP", "name": "   " }),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "ERP", None),
            ("jira", "INT", None),
            ("jira", "PAY", None),
        ],
        "a project reachable by its key, never one dropped for being nameless"
    );
}

/// The census is of the **live** corpus: a tombstoned item contributes no
/// project, and a project only a tombstoned item showed is gone with it.
#[tokio::test]
async fn a_tombstoned_item_contributes_no_project() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    let withdrawn = ticket(
        &pool,
        "INT-1",
        serde_json::json!({ "key": "INT", "name": "Integrations" }),
    )
    .await;
    tombstone(&pool, &withdrawn).await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![("jira", "PAY", Some("Payout"))]);
}

/// A source the user has turned off shows no projects.
///
/// Migration `0012` gave `sync.live_item` its second reason to hide a row, and
/// this read joins that view rather than `sync.item` -- so "turned off" means
/// the same thing here as everywhere else (#202), and the source's projects
/// come back when it does. Beside the tombstone case above because they are
/// the same join doing two jobs.
#[tokio::test]
async fn a_disabled_source_shows_no_projects() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    item(
        &pool,
        "teamcity",
        "build",
        "buildType:Payout_Build:1188",
        teamcity(serde_json::json!({ "projectId": "Payout", "projectName": "Payout pipeline" })),
    )
    .await;
    configure(&pool, "teamcity", false).await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(rows(&projects), vec![("jira", "PAY", Some("Payout"))]);

    sqlx::query("update knobas.source_config set enabled = true where id = 'teamcity'")
        .execute(&pool)
        .await
        .unwrap();
    let projects = project::list(&pool).await.unwrap();
    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "PAY", Some("Payout")),
            ("teamcity", "Payout", Some("Payout pipeline")),
        ],
        "nothing was stored about the disabled source, so re-enabling it is enough"
    );
}

/// Two sources that happen to use one project key stay two projects.
///
/// A key is unique only inside its own source, which is why the pair is the
/// identity and why a project room names both halves.
#[tokio::test]
async fn one_key_in_two_sources_is_two_projects() {
    let pool = scratch().await;
    ticket(
        &pool,
        "PAY-1",
        serde_json::json!({ "key": "PAY", "name": "Payout" }),
    )
    .await;
    item(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-1",
        jira(serde_json::json!({ "key": "PAY", "name": "Payments (EU)" })),
    )
    .await;

    let projects = project::list(&pool).await.unwrap();

    assert_eq!(
        rows(&projects),
        vec![
            ("jira", "PAY", Some("Payout")),
            ("jira-eu", "PAY", Some("Payments (EU)")),
        ]
    );
}
