//! Whether the launcher admits a row beneath an `under:` name is answered
//! twice (#2495): by the launcher's own statement, and by
//! `sql::parent_positions`, the bounded read off `sync.item_parent`
//! and the estate that `Searcher::search` asks first -- to skip the statement
//! for a name with nothing admitted beneath it, and to name that token as
//! left out (#2493). The two must agree for every shape a name can have, or
//! `under:` admits what the probe says is left out. Each shape is one row
//! here, under the real Gitea descriptor, and the oracle is the statement
//! itself, as `under::left_out_tokens` ran it before #2495.

use knobas_search::query::EffectiveFilters;
use knobas_search::{RawHit, Searcher, corpus, sql};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // A Gitea, a Gitea turned off, and a source whose rows were synced by the
    // Gitea adapter but which is configured as something else now (the
    // read-time adapter match, ADR-0019).
    for (id, kind, enabled) in [
        ("gitea", "gitea", true),
        ("gitea-off", "gitea", false),
        ("gitea-as-jira", "jira", true),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
             values ($1, $2, $2, 'http://x', 'Pat', $3)
             on conflict (id) do update set kind = excluded.kind, enabled = excluded.enabled",
        )
        .bind(id)
        .bind(kind)
        .bind(enabled)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

fn token() -> String {
    format!("ua{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

/// A Gitea row of `kind` in `source`, naming the repository `repo` the way
/// the adapter stores it: a pull request through `base.repo`, a commit
/// through `repository`.
async fn gitea(pool: &sqlx::PgPool, source: &str, kind: &str, repo: &str) -> String {
    let id = format!("{source}:{repo}#{kind}");
    let full = format!("o/{repo}");
    let payload = match kind {
        "commit" => serde_json::json!({ "repository": { "full_name": full, "name": repo } }),
        _ => serde_json::json!({ "base": { "repo": { "full_name": full, "name": repo } } }),
    };
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(&id)
        .bind(kind)
        .bind(repo)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1, $2, $3, $4, '', $5, now(), now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(repo)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn asset(pool: &sqlx::PgPool, name: &str, parent: Option<&str>) -> String {
    let id = format!("asset:{name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'asset', $2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, path_text, parent_id)
         values ($1, 'vm', $2, '', $3)",
    )
    .bind(&id)
    .bind(name)
    .bind(parent)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// What the launcher's statement, asked with nothing beside the name and the
/// hidden kinds, admits: the oracle.
async fn statement_admits(
    pool: &sqlx::PgPool,
    declarations: &knobas_core::payload::Declarations,
    name: &str,
    hidden: &[&str],
) -> bool {
    let filters = EffectiveFilters {
        under: vec![name.to_owned()],
        declarations: declarations.clone(),
        hidden_kinds: hidden.iter().map(|k| (*k).to_owned()).collect(),
        ..EffectiveFilters::default()
    };
    let corpora = corpus::searched(corpus::ALL, false);
    let built = sql::search_sql(&corpora, None, false, &filters, 1, 1);
    let rows: Vec<RawHit> = sql::query_as_with(built).fetch_all(pool).await.unwrap();
    !rows.is_empty()
}

#[tokio::test]
async fn the_bounded_read_admits_exactly_what_the_launchers_statement_admits() {
    let pool = pool().await;
    let searcher = Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap();
    let declarations = searcher.vocabulary().await.unwrap().declarations;
    let t = token();

    let kept = format!("kept-{t}");
    gitea(&pool, "gitea", "pr", &kept).await;
    let commits_only = format!("commits-{t}");
    gitea(&pool, "gitea", "commit", &commits_only).await;
    let withdrawn = format!("gone-{t}");
    let gone = gitea(&pool, "gitea", "pr", &withdrawn).await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();
    let off = format!("off-{t}");
    gitea(&pool, "gitea-off", "pr", &off).await;
    let mismatch = format!("mismatch-{t}");
    gitea(&pool, "gitea-as-jira", "pr", &mismatch).await;
    let host = format!("host-{t}");
    let host_id = asset(&pool, &host, None).await;
    let routed = format!("routed-{t}");
    let routed_id = asset(&pool, &routed, Some(&host_id)).await;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'route', $2)")
        .bind(format!("route:{t}"))
        .bind(&routed)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.route (id, asset_id, name, url) values ($1, $2, $3, 'https://x')",
    )
    .bind(format!("route:{t}"))
    .bind(&routed_id)
    .bind(&routed)
    .execute(&pool)
    .await
    .unwrap();
    let bare = format!("bare-{t}");
    asset(&pool, &bare, None).await;
    let unknown = format!("nothing-{t}");

    // The hand-written truth, so the two cannot agree by both being wrong.
    // The launcher hides commits (`frames::ALWAYS_LEFT_OUT`); the other
    // hidden sets are the extremes the predicate has to survive.
    let shapes: &[(&str, &str, bool)] = &[
        ("a pull request the launcher keeps names it", &kept, true),
        ("only a commit names it", &commits_only, false),
        ("a withdrawn pull request names it", &withdrawn, false),
        ("a pull request in a disabled source names it", &off, false),
        ("a row synced by Gitea in a source configured as Jira", &mismatch, false),
        ("an asset is called so", &host, true),
        ("an asset with a route beneath is called so", &routed, true),
        ("an asset with nothing beneath is called so", &bare, true),
        ("nothing answers to it", &unknown, false),
        // Upper case: compared lower-cased on both sides, as `under:` is.
        ("the kept name, upper-cased", &kept.to_uppercase(), true),
    ];
    let names: Vec<String> = shapes.iter().map(|(_, name, _)| (*name).to_owned()).collect();

    for hidden in [
        &["commit"][..],
        &[][..],
        &["commit", "pr"][..],
        &["commit", "asset"][..],
        &["commit", "asset", "route"][..],
    ] {
        let hidden_kinds: Vec<String> = hidden.iter().map(|k| (*k).to_owned()).collect();
        let admitting = sql::parent_positions(&pool, &names, &hidden_kinds)
            .await
            .unwrap()
            .admitting;
        for ((shape, name, kept_beneath), position) in shapes.iter().zip(1_i64..) {
            let oracle = statement_admits(&pool, &declarations, name, hidden).await;
            let bounded = admitting.contains(&position);
            assert_eq!(
                bounded, oracle,
                "{shape} (under:{name}, hidden {hidden:?}): the bounded read says {bounded}, \
                 the launcher's statement {oracle}"
            );
            // What the truth is, with the launcher's own hidden kinds.
            if hidden == ["commit"] {
                assert_eq!(oracle, *kept_beneath, "{shape} (under:{name})");
            }
        }
    }
    // With commits kept, the commits-only name admits its commit; with pull
    // requests hidden too, the kept name admits nothing; with assets hidden,
    // the routed asset is still admitted through its route (and so is the
    // host above it, through the walk) while the bare one is not. Each is a
    // row above under one of the hidden sets, and the oracle decided it;
    // this pins that the fixture exercised every arm.
    let under = |hidden: &[&str]| {
        let hidden: Vec<String> = hidden.iter().map(|k| (*k).to_owned()).collect();
        let names = names.clone();
        let pool = pool.clone();
        async move {
            sql::parent_positions(&pool, &names, &hidden)
                .await
                .unwrap()
                .admitting
        }
    };
    assert!(under(&[]).await.contains(&2), "commits kept: the commit is admitted");
    assert!(!under(&["commit", "pr"]).await.contains(&1), "pull requests hidden");
    let assets_hidden = under(&["commit", "asset"]).await;
    assert!(assets_hidden.contains(&7), "the route beneath the hidden asset");
    assert!(assets_hidden.contains(&6), "the route beneath the hidden host's child");
    assert!(!assets_hidden.contains(&8), "the bare hidden asset");
    assert!(!under(&["commit", "asset", "route"]).await.contains(&7));
}

#[tokio::test]
async fn no_names_ask_nothing() {
    let pool = pool().await;
    let admitting = sql::parent_positions(&pool, &[], &["commit".to_owned()])
        .await
        .unwrap();
    assert_eq!(admitting, sql::ParentPositions::default());
}
