//! What a repository's detail carries for the repository page (#2298, #2307),
//! on a real PostgreSQL: the default branch through the kind's declared
//! path, and how its tree names its sections, from the adapter registry.

use knobas_app::commands::entity::get_entity_inner;
use knobas_app::sources::Registry;
use knobas_source::view::TreeSections;

/// A Gitea source of its own, and one row of `kind` in it with `payload`.
async fn gitea_row(kind: &str, payload: serde_json::Value) -> (sqlx::PgPool, String) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let source = format!("gitea-{}", &unique[..12]);
    crate::support::source_config(&pool, &source, "gitea", "gitea", &serde_json::json!({})).await;
    let id = format!("{source}:tidewater/payout-service-{}", &unique[..6]);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$1)")
        .bind(&id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$1,'',$4)",
    )
    .bind(&id)
    .bind(&source)
    .bind(kind)
    .bind(payload)
    .execute(&pool)
    .await
    .unwrap();
    (pool, id)
}

#[tokio::test]
async fn a_gitea_repositorys_detail_carries_its_default_branch_and_its_trees_sections() {
    let (pool, id) = gitea_row(
        "repo",
        serde_json::json!({ "full_name": "tidewater/payout-service", "default_branch": "trunk" }),
    )
    .await;

    let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(detail.default_branch.as_deref(), Some("trunk"));
    assert_eq!(
        detail.tree,
        Some(TreeSections {
            dir: "files:".to_owned(),
            file: "file:".to_owned(),
        })
    );
}

#[tokio::test]
async fn a_repository_whose_record_names_no_default_branch_has_none() {
    for payload in [
        serde_json::json!({ "full_name": "tidewater/payout-service" }),
        serde_json::json!({ "default_branch": 7 }),
    ] {
        let (pool, id) = gitea_row("repo", payload.clone()).await;
        let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
            .await
            .unwrap();
        assert_eq!(detail.default_branch, None, "{payload}");
    }
}

#[tokio::test]
async fn a_kind_that_draws_no_tree_carries_no_sections() {
    let (pool, id) = gitea_row("pr", serde_json::json!({ "number": 4 })).await;
    let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(detail.tree, None);
}
