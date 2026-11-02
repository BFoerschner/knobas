//! `under:<build configuration>` against a real PostgreSQL (ADR-0018, #597):
//! it scoped a search to the configuration's builds, and since #2422 a build
//! is in no launcher answer, so it answers with the configuration alone, as
//! the pipeline it is.
//!
//! TeamCity declares a build's configuration as its parent, read off the
//! `buildType` object every build record nests, answered to by its `name` and
//! its `id`. These items are shaped the way the real adapter stores them -- a
//! build's raw record with its nested `buildType`, a configuration's raw
//! `buildType` record -- and the declaration is that adapter's own
//! descriptor, not a copy of it. One database for the file (one per test
//! module, #572), a unique token per test, nothing truncated, as in `under.rs`.

use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        titles: false,
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token() -> String {
    format!("tc{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('teamcity', 'teamcity', 'TeamCity', 'http://x', 'Pat')
         on conflict (id) do update set kind = excluded.kind, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// The searcher the app builds, with TeamCity's declared payload paths --
/// written to `sync.parent_declaration` too, which `under:` reads through
/// `sync.item_parent` since #627. This file has its own database (#572), so
/// declaring TeamCity alone removes nobody else's adapter kind.
async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [knobas_source_teamcity::descriptor_template()],
    )
    .await
    .unwrap()
}

async fn item(
    pool: &sqlx::PgPool,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    let id = format!("teamcity:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'teamcity',$2,$3,$3,$4, now())",
    )
    .bind(&id)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One build configuration and two finished builds of it, stored the way the
/// adapter stores them, titled the way `map` titles them. Answers with the
/// three entity ids, configuration first. `t` keeps the builds' keys apart
/// from every other test's.
async fn configuration(
    pool: &sqlx::PgPool,
    t: &str,
    project: &str,
    id: &str,
    name: &str,
    builds: [i64; 2],
) -> Vec<String> {
    let build_type = serde_json::json!({
        "id": id, "name": name, "projectId": project, "projectName": project
    });
    let mut out = vec![
        item(
            pool,
            "build_config",
            &format!("buildType:{id}"),
            &format!("{project} / {name}"),
            build_type.clone(),
        )
        .await,
    ];
    for build in builds {
        out.push(
            item(
                pool,
                "build",
                &format!("build:{t}-{build}"),
                &format!("{project} / {name} #{build}"),
                serde_json::json!({
                    "id": build, "number": build.to_string(), "buildTypeId": id,
                    "state": "finished", "status": "SUCCESS",
                    "buildType": build_type,
                }),
            )
            .await,
        );
    }
    out
}

fn ids(response: &SearchResponse) -> Vec<String> {
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    out.sort();
    out
}

/// The configuration's name (the title TeamCity gives it) and its id, matched
/// ignoring case, both return the configuration itself and none of its builds
/// (#2422), and nothing of the configuration beside it, though the two share
/// a project.
#[tokio::test]
async fn under_a_configuration_by_name_or_id_returns_itself_and_none_of_its_builds() {
    let pool = pool().await;
    let t = token();
    let project = format!("Payments{t}");
    let id = format!("Payments{t}_Deploy");
    let name = format!("Deploy{t}");
    let ours = configuration(&pool, &t, &project, &id, &name, [41, 42]).await;
    configuration(
        &pool,
        &t,
        &project,
        &format!("Payments{t}_Build"),
        &format!("Build{t}"),
        [43, 44],
    )
    .await;

    for raw in [
        format!("under:{name}"),
        format!("under:{}", name.to_uppercase()),
        format!("under:{id}"),
        format!("under:{}", id.to_lowercase()),
    ] {
        let response = searcher(&pool).await.search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), vec![ours[0].clone()], "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names a configuration and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }
}

/// The configuration's hit is the pipeline (#2422): kind `pipeline`, its own
/// title, no path line where TeamCity gave it no chain of projects, and its
/// newest build's state; the builds themselves are not hits.
#[tokio::test]
async fn a_configuration_is_answered_as_a_pipeline_in_its_newest_builds_state() {
    let pool = pool().await;
    let t = token();
    let project = format!("Payments{t}");
    let name = format!("Deploy{t}");
    let ours = configuration(
        &pool,
        &t,
        &project,
        &format!("Payments{t}_Deploy"),
        &name,
        [41, 42],
    )
    .await;

    let response = searcher(&pool)
        .await
        .search(q(&format!("under:{name}")))
        .await
        .unwrap();
    let rows: Vec<_> = response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits.iter().map(|h| {
                (
                    h.row.kind.clone(),
                    h.row.entity_id.clone(),
                    h.row.title.clone(),
                    h.row.path.clone(),
                    h.row.state.as_ref().map(|s| s.text.clone()),
                )
            })
        })
        .collect();
    assert_eq!(
        rows,
        vec![(
            "pipeline".to_owned(),
            ours[0].clone(),
            format!("{project} / {name}"),
            None,
            Some("SUCCESS".to_owned())
        )]
    );
}
