//! Whether the launcher admits a row beneath an `under:` name is answered
//! twice (#2495): by the launcher's own statement, and by
//! `sql::parent_positions`, the bounded read off `sync.item_parent`
//! and the estate that `Searcher::search` asks first -- to skip the statement
//! for a name with nothing admitted beneath it, and to name that token as
//! left out (#2493). The two must agree for every shape a name can have, or
//! `under:` admits what the probe says is left out. Each shape is one row
//! here, under the real Gitea descriptor, and the oracle is the statement
//! itself, as `under::left_out_tokens` ran it before #2495. The second test
//! holds `Searcher::search` to the same table, two names at a time, because
//! the skip is decided over every name typed.

use knobas_search::query::EffectiveFilters;
use knobas_search::{RawHit, SearchFilters, SearchQuery, SearchResponse, Searcher, corpus, sql};

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
/// through `repository`, the repository itself by its own names (the parent
/// arm of `sync.item_parent`, where the other two are the child arm).
async fn gitea(pool: &sqlx::PgPool, source: &str, kind: &str, repo: &str) -> String {
    let id = format!("{source}:{repo}#{kind}");
    let full = format!("o/{repo}");
    let payload = match kind {
        "commit" => serde_json::json!({ "repository": { "full_name": full, "name": repo } }),
        "repo" => {
            serde_json::json!({ "full_name": full, "name": repo, "owner": { "login": "o" } })
        }
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

/// What the launcher's statement, asked with nothing beside the names and the
/// hidden kinds, admits: the oracle.
async fn statement_admits(
    pool: &sqlx::PgPool,
    declarations: &knobas_core::payload::Declarations,
    names: &[&str],
    hidden: &[&str],
) -> bool {
    let filters = EffectiveFilters {
        under: names.iter().map(|name| (*name).to_owned()).collect(),
        declarations: declarations.clone(),
        hidden_kinds: hidden.iter().map(|k| (*k).to_owned()).collect(),
        ..EffectiveFilters::default()
    };
    let corpora = corpus::searched(corpus::ALL, false);
    let built = sql::search_sql(&corpora, None, false, &filters, 1, 1);
    let rows: Vec<RawHit> = sql::query_as_with(built).fetch_all(pool).await.unwrap();
    !rows.is_empty()
}

/// One shape a name can have, with the hand-written truth of it under what the
/// launcher hides (`frames::ALWAYS_LEFT_OUT`, commits), so the two reads and
/// the search cannot agree by all being wrong.
struct Shape {
    what: &'static str,
    name: String,
    /// A parent answers to it, so it is no unknown token.
    known: bool,
    /// The launcher admits a row beneath it.
    admits: bool,
}

/// Seeds every shape, each name carrying a fresh token. The positions the
/// first test reads its arms by are the order here.
async fn shapes(pool: &sqlx::PgPool) -> Vec<Shape> {
    let t = token();

    let kept = format!("kept-{t}");
    gitea(pool, "gitea", "pr", &kept).await;
    let commits_only = format!("commits-{t}");
    gitea(pool, "gitea", "commit", &commits_only).await;
    let withdrawn = format!("gone-{t}");
    let gone = gitea(pool, "gitea", "pr", &withdrawn).await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(pool)
        .await
        .unwrap();
    let off = format!("off-{t}");
    gitea(pool, "gitea-off", "pr", &off).await;
    let mismatch = format!("mismatch-{t}");
    gitea(pool, "gitea-as-jira", "pr", &mismatch).await;
    let host = format!("host-{t}");
    let host_id = asset(pool, &host, None).await;
    let routed = format!("routed-{t}");
    let routed_id = asset(pool, &routed, Some(&host_id)).await;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'route', $2)")
        .bind(format!("route:{t}"))
        .bind(&routed)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.route (id, asset_id, name, url) values ($1, $2, $3, 'https://x')",
    )
    .bind(format!("route:{t}"))
    .bind(&routed_id)
    .bind(&routed)
    .execute(pool)
    .await
    .unwrap();
    let bare = format!("bare-{t}");
    asset(pool, &bare, None).await;
    let repository = format!("repo-{t}");
    gitea(pool, "gitea", "repo", &repository).await;

    let shape = |what, name: &str, known, admits| Shape {
        what,
        name: name.to_owned(),
        known,
        admits,
    };
    vec![
        shape(
            "a pull request the launcher keeps names it",
            &kept,
            true,
            true,
        ),
        shape("only a commit names it", &commits_only, true, false),
        shape(
            "a withdrawn pull request names it",
            &withdrawn,
            false,
            false,
        ),
        shape(
            "a pull request in a disabled source names it",
            &off,
            false,
            false,
        ),
        shape(
            "a row synced by Gitea in a source configured as Jira",
            &mismatch,
            false,
            false,
        ),
        shape("an asset is called so", &host, true, true),
        shape(
            "an asset with a route beneath is called so",
            &routed,
            true,
            true,
        ),
        shape(
            "an asset with nothing beneath is called so",
            &bare,
            true,
            true,
        ),
        shape(
            "nothing answers to it",
            &format!("nothing-{t}"),
            false,
            false,
        ),
        // Upper case: compared lower-cased on both sides, as `under:` is.
        shape(
            "the kept name, upper-cased",
            &kept.to_uppercase(),
            true,
            true,
        ),
        shape(
            "a repository answers to its own name",
            &repository,
            true,
            true,
        ),
    ]
}

#[tokio::test]
async fn the_bounded_read_admits_exactly_what_the_launchers_statement_admits() {
    let pool = pool().await;
    let searcher = Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap();
    let declarations = searcher.vocabulary().await.unwrap().declarations;
    let shapes = shapes(&pool).await;
    let names: Vec<String> = shapes.iter().map(|shape| shape.name.clone()).collect();

    // The launcher hides commits; the other hidden sets are the extremes the
    // predicate has to survive.
    for hidden in [
        &["commit"][..],
        &[][..],
        &["commit", "pr"][..],
        &["commit", "repo"][..],
        &["commit", "asset"][..],
        &["commit", "asset", "route"][..],
    ] {
        let hidden_kinds: Vec<String> = hidden.iter().map(|k| (*k).to_owned()).collect();
        let positions = sql::parent_positions(&pool, &names, &hidden_kinds)
            .await
            .unwrap();
        for (shape, position) in shapes.iter().zip(1_i64..) {
            let (what, name) = (shape.what, &shape.name);
            let oracle = statement_admits(&pool, &declarations, &[name], hidden).await;
            let bounded = positions.admitting.contains(&position);
            assert_eq!(
                bounded, oracle,
                "{what} (under:{name}, hidden {hidden:?}): the bounded read says {bounded}, \
                 the launcher's statement {oracle}"
            );
            // Whether a name is known does not depend on what is hidden, and
            // what is admitted is a subset of it: `admits_nothing` reads an
            // unknown name as having nothing beneath it.
            let known = positions.known.contains(&position);
            assert_eq!(known, shape.known, "{what} (under:{name})");
            assert!(known || !bounded, "{what} (under:{name}) admits, unknown");
            // What the truth is, with the launcher's own hidden kinds.
            if hidden == ["commit"] {
                assert_eq!(oracle, shape.admits, "{what} (under:{name})");
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
    assert!(
        under(&[]).await.contains(&2),
        "commits kept: the commit is admitted"
    );
    assert!(
        !under(&["commit", "pr"]).await.contains(&1),
        "pull requests hidden"
    );
    let assets_hidden = under(&["commit", "asset"]).await;
    assert!(
        assets_hidden.contains(&7),
        "the route beneath the hidden asset"
    );
    assert!(
        assets_hidden.contains(&6),
        "the route beneath the hidden host's child"
    );
    assert!(!assets_hidden.contains(&8), "the bare hidden asset");
    assert!(!under(&["commit", "asset", "route"]).await.contains(&7));
    assert!(
        !under(&["commit", "repo"]).await.contains(&11),
        "the repository hidden, and nothing else beneath its name"
    );
}

async fn search(searcher: &Searcher, raw: &str) -> SearchResponse {
    searcher
        .search(SearchQuery {
            titles: false,
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
}

/// `Searcher::search` skips the statement when no name typed admits a row. So
/// every two shapes typed together, in both orders, at the seam the launcher
/// calls: rows when either name admits one, which the statement confirms; each
/// unknown name reported; and on an empty answer, exactly the known names that
/// admit nothing named as left out, in the order typed. A name left out beside
/// a name that admits a row must not cost that row: that is `admits_nothing`
/// reading every name rather than any of them.
#[tokio::test]
async fn a_query_skips_the_statement_only_when_no_name_typed_admits_a_row() {
    let pool = pool().await;
    let searcher = Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap();
    let declarations = searcher.vocabulary().await.unwrap().declarations;
    let shapes = shapes(&pool).await;
    let word = format!("nothing{}", token());
    let token_of = |shape: &&Shape| format!("under:{}", shape.name);

    for first in &shapes {
        for second in &shapes {
            // One value typed twice is one name (`query::same_value`).
            if first.name.to_lowercase() == second.name.to_lowercase() {
                continue;
            }
            let pair = [first, second];
            let typed = format!("{} {}", token_of(&first), token_of(&second));
            let unknown: Vec<String> = pair
                .iter()
                .filter(|shape| !shape.known)
                .map(token_of)
                .collect();
            let left_out: Vec<String> = pair
                .iter()
                .filter(|shape| shape.known && !shape.admits)
                .map(token_of)
                .collect();
            let admitted = first.admits || second.admits;

            let names = [first.name.as_str(), second.name.as_str()];
            assert_eq!(
                statement_admits(&pool, &declarations, &names, &["commit"]).await,
                admitted,
                "{typed}: the statement"
            );

            let answer = search(&searcher, &typed).await;
            assert_eq!(answer.total > 0, admitted, "{typed}");
            assert_eq!(answer.interpreted.unknown_tokens, unknown, "{typed}");
            let said = if admitted { &[][..] } else { &left_out[..] };
            assert_eq!(answer.interpreted.left_out_tokens, said, "{typed}");

            // A word that misses is empty whatever sits beneath, and the names
            // the launcher left out are named all the same.
            let raw = format!("{typed} {word}");
            let answer = search(&searcher, &raw).await;
            assert_eq!(answer.total, 0, "{raw}");
            assert_eq!(answer.interpreted.unknown_tokens, unknown, "{raw}");
            assert_eq!(answer.interpreted.left_out_tokens, left_out, "{raw}");
        }
    }

    // A left-out `kind:` comes first in the tokens, whatever the typed order,
    // and is the whole of them beside a name that admits a row.
    let kept = &shapes[0].name;
    let commits_only = &shapes[1].name;
    let raw = format!("under:{commits_only} kind:commit");
    let answer = search(&searcher, &raw).await;
    assert_eq!(answer.total, 0, "{raw}");
    assert_eq!(
        answer.interpreted.left_out_tokens,
        ["kind:commit".to_owned(), format!("under:{commits_only}")],
        "{raw}"
    );
    let raw = format!("under:{kept} kind:commit");
    let answer = search(&searcher, &raw).await;
    assert_eq!(answer.total, 0, "{raw}");
    assert_eq!(answer.interpreted.left_out_tokens, ["kind:commit"], "{raw}");
}

#[tokio::test]
async fn no_names_ask_nothing() {
    let pool = pool().await;
    let admitting = sql::parent_positions(&pool, &[], &["commit".to_owned()])
        .await
        .unwrap();
    assert_eq!(admitting, sql::ParentPositions::default());
}
