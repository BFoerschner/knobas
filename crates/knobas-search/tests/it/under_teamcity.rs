//! `under:<build configuration>` returns its builds, against a real PostgreSQL
//! (ADR-0018, #597).
//!
//! TeamCity declares a build's configuration as its parent, read off the
//! `buildType` object every build record nests, answered to by its `name` and
//! its `id`. These items are shaped the way the real adapter stores them -- a
//! build's raw record with its nested `buildType`, a configuration's raw
//! `buildType` record -- and the declaration is that adapter's own
//! descriptor, not a copy of it. One database for the file, a unique token per
//! test, nothing truncated: the house rule of `under.rs`.

use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
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

/// The searcher the app builds, with TeamCity's declared payload paths.
fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::new(pool.clone()).with_payload_paths([knobas_source_teamcity::descriptor_template()])
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

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// The configuration's name -- the title TeamCity gives it -- and its id, in
/// any case, both return its builds
/// **and** the configuration itself -- and nothing of the configuration
/// beside it, though the two share a project.
#[tokio::test]
async fn under_a_configuration_by_name_or_id_returns_its_builds_and_itself() {
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
        let response = searcher(&pool).search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), sorted(ours.clone()), "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names a configuration and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }
}

/// A build hit carries its configuration on its path line -- the
/// configuration's name, the first name TeamCity declares -- and keeps its
/// title, which already names the configuration (ADR-0018: a title may name
/// its parent where it means nothing without it). The configuration declares
/// no parent and carries no path.
#[tokio::test]
async fn a_build_carries_its_configuration_as_path_and_keeps_its_title() {
    let pool = pool().await;
    let t = token();
    let project = format!("Payments{t}");
    let name = format!("Deploy{t}");
    let builds = [41, 42];
    let ours = configuration(
        &pool,
        &t,
        &project,
        &format!("Payments{t}_Deploy"),
        &name,
        builds,
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("under:{name}")))
        .await
        .unwrap();
    let mut rows: Vec<(String, String, Option<String>)> = response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits.iter().map(|h| {
                (
                    h.row.entity_id.clone(),
                    h.row.title.clone(),
                    h.row.path.clone(),
                )
            })
        })
        .collect();
    rows.sort();
    let mut expected = vec![
        (ours[0].clone(), format!("{project} / {name}"), None),
        (
            ours[1].clone(),
            format!("{project} / {name} #{}", builds[0]),
            Some(name.clone()),
        ),
        (
            ours[2].clone(),
            format!("{project} / {name} #{}", builds[1]),
            Some(name.clone()),
        ),
    ];
    expected.sort();
    assert_eq!(rows, expected);
}
