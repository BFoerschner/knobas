//! `under:<name>`, end to end against a real PostgreSQL (ADR-0018, #595).
//!
//! The filter scopes a search to everything beneath a **parent**, and it reads
//! the parent each child *declares* -- through the real Gitea adapter's
//! descriptor, not a copy of it -- so these items are shaped the way that
//! adapter stores them: a commit and a branch carry the `repository` object
//! `map` writes in, a pull request carries Gitea's own `base.repo`, a repo
//! carries `full_name` and `name` at its root.
//!
//! One database for the file, a unique token per test, nothing truncated: the
//! house rule of `search.rs`. Every repo name below carries the test's token,
//! so no test can match another test's rows, and the tests that share a bare
//! name share it only with themselves.

use knobas_search::{
    FilterAnswer, FilterDimension, SearchFilters, SearchQuery, SearchResponse, Searcher,
};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token() -> String {
    format!("u{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

/// A migrated database, with the three sources these tests read: a Gitea, an
/// Uptime Kuma whose adapter declares no parent, and nothing else.
async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind, name) in [("gitea", "gitea", "Gitea"), ("kuma", "kuma", "Uptime Kuma")] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $3, 'http://x', 'Pat')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// The searcher the app builds: the adapters' declared payload paths, Gitea's
/// among them. Uptime Kuma's template is not passed, and that is the point of
/// it here -- a source whose adapter declares nothing declares no parent.
fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::new(pool.clone()).with_payload_paths([knobas_source_gitea::descriptor_template()])
}

async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
) -> String {
    let id = format!("{source}:{key}");
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
         values ($1,$2,$3,$4,$5,$6, now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One Gitea repo and a branch, a pull request and a commit in it, stored the
/// way the adapter stores them. Answers with the four entity ids, repo first.
async fn repo(pool: &sqlx::PgPool, owner: &str, name: &str, commit_message: &str) -> Vec<String> {
    let full = format!("{owner}/{name}");
    let repository = serde_json::json!({ "full_name": full, "name": name });
    let sha = uuid::Uuid::new_v4().simple().to_string();
    vec![
        item(
            pool,
            "gitea",
            "repo",
            &full,
            &full,
            &format!("{full}\n\nPayout processing service"),
            serde_json::json!({ "full_name": full, "name": name, "owner": { "login": owner } }),
        )
        .await,
        item(
            pool,
            "gitea",
            "branch",
            &format!("{full}@refs/heads/main"),
            "main",
            "main\n\nInitial import",
            serde_json::json!({ "name": "main", "repository": repository }),
        )
        .await,
        item(
            pool,
            "gitea",
            "pr",
            &format!("{full}#7"),
            "SEPA retry with exponential backoff",
            "SEPA retry with exponential backoff",
            serde_json::json!({ "number": 7, "base": { "ref": "main", "repo": repository } }),
        )
        .await,
        item(
            pool,
            "gitea",
            "commit",
            &format!("{full}@{sha}"),
            commit_message,
            commit_message,
            serde_json::json!({ "sha": sha, "repository": repository }),
        )
        .await,
    ]
}

/// Every entity id the response carries, sorted.
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

/// The tracer bullet: the full name and the bare name, in any case, return the
/// repo's branch, pull request and commit **and** the repo itself -- and
/// nothing of the repo beside it.
#[tokio::test]
async fn under_a_repo_returns_its_branches_prs_and_commits_and_the_repo() {
    let pool = pool().await;
    let t = token();
    // Stored in mixed case, as a Gitea repo may be named: a fold on only one
    // side of the comparison -- the typed value's -- would still match a name
    // stored in lowercase, and nothing would see the other side go.
    let owner = format!("Tidewater{t}");
    let name = format!("Payout-Service-{t}");
    let ours = repo(&pool, &owner, &name, "PAY-231: jitter in backoff").await;
    repo(
        &pool,
        &format!("elsewhere{t}"),
        &format!("ledger-{t}"),
        "unrelated",
    )
    .await;

    for raw in [
        format!("under:{owner}/{name}"),
        format!("under:{}", name.to_lowercase()),
        format!("under:{}", name.to_uppercase()),
        format!("UNDER:{}/{}", owner.to_lowercase(), name.to_uppercase()),
    ] {
        let response = searcher(&pool).search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), sorted(ours.clone()), "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names a parent and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }
}

/// A bare name two owners share scopes to both repos -- the filter never picks
/// one for the reader -- and the full name still picks one.
#[tokio::test]
async fn a_bare_name_two_owners_share_scopes_to_both() {
    let pool = pool().await;
    let t = token();
    let name = format!("shared-{t}");
    let first = repo(&pool, &format!("alpha{t}"), &name, "one").await;
    let second = repo(&pool, &format!("beta{t}"), &name, "two").await;

    let both = searcher(&pool)
        .search(q(&format!("under:{name}")))
        .await
        .unwrap();
    assert_eq!(ids(&both), sorted([first.clone(), second].concat()));

    let one = searcher(&pool)
        .search(q(&format!("under:alpha{t}/{name}")))
        .await
        .unwrap();
    assert_eq!(ids(&one), sorted(first));
}

/// Two `under:` values are "in a or in b", the OR every repeated filter means;
/// a value no declared name matches is reported as unknown rather than read as
/// an empty repo.
#[tokio::test]
async fn repeated_values_are_a_union_and_an_unmatched_one_is_unknown() {
    let pool = pool().await;
    let t = token();
    let a = repo(&pool, &format!("o{t}"), &format!("a-{t}"), "first").await;
    let b = repo(&pool, &format!("o{t}"), &format!("b-{t}"), "second").await;
    repo(&pool, &format!("o{t}"), &format!("c-{t}"), "third").await;

    let union = searcher(&pool)
        .search(q(&format!("under:a-{t} under:b-{t}")))
        .await
        .unwrap();
    assert_eq!(ids(&union), sorted([a.clone(), b].concat()));
    assert!(union.interpreted.unknown_tokens.is_empty());

    let nope = format!("under:nope-{t}");
    let unknown = searcher(&pool).search(q(&nope)).await.unwrap();
    assert_eq!(
        unknown.interpreted.unknown_tokens,
        std::slice::from_ref(&nope)
    );
    assert!(ids(&unknown).is_empty());

    // One known and one unknown: the known one still scopes, the unknown one
    // is still reported.
    let mixed = searcher(&pool)
        .search(q(&format!("under:a-{t} {nope}")))
        .await
        .unwrap();
    assert_eq!(ids(&mixed), sorted(a));
    assert_eq!(mixed.interpreted.unknown_tokens, [nope]);
}

/// The repo's name left the commit's text, and `under:` is what still finds
/// the commit about jitter in that repo -- words after the filter search
/// inside it.
#[tokio::test]
async fn under_a_repo_with_words_finds_the_commit_that_mentions_them() {
    let pool = pool().await;
    let t = token();
    let name = format!("payout-{t}");
    let ours = repo(
        &pool,
        &format!("tidewater{t}"),
        &name,
        &format!("PAY-231: jitter{t} in backoff"),
    )
    .await;
    // The same word in another repo is outside the scope.
    repo(
        &pool,
        &format!("tidewater{t}"),
        &format!("ledger-{t}"),
        &format!("jitter{t} too"),
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("under:{name} jitter{t}")))
        .await
        .unwrap();
    assert_eq!(ids(&response), vec![ours[3].clone()]);
}

/// A plain search for a repo's name finds the repo, and not its commits and
/// branches merely for living in it.
///
/// The full name, because that is the one lexeme the name ever was in a
/// child's text: PostgreSQL's parser reads `owner/name` as a single file-path
/// token, so `tidewater/payout-service` in a commit's body matched exactly
/// this query and a bare `payout-service` never did. The children here carry
/// the text `map` now writes for them -- message and branch name, no repo --
/// and `knobas-source-gitea`'s mapping golden tests are what pin that it is
/// what `map` writes. This is what a reader sees as a result.
#[tokio::test]
async fn a_plain_repo_name_finds_the_repo_and_not_its_contents() {
    let pool = pool().await;
    let t = token();
    let full = format!("tidewater{t}/payout{t}");
    let ours = repo(
        &pool,
        &format!("tidewater{t}"),
        &format!("payout{t}"),
        "PAY-231: jitter in backoff",
    )
    .await;

    let response = searcher(&pool).search(q(&full)).await.unwrap();
    assert_eq!(ids(&response), vec![ours[0].clone()]);
}

/// `under:` takes part in coverage the way `author:` does: a source with no
/// kind declaring a parent is reported as having nothing the filter can match,
/// so a missing monitor reads as "cannot answer" and not as "nothing there".
#[tokio::test]
async fn coverage_names_a_source_that_declares_no_parent() {
    let pool = pool().await;
    let t = token();
    let name = format!("cov-{t}");
    repo(&pool, &format!("o{t}"), &name, "first").await;
    item(
        &pool,
        "kuma",
        "monitor",
        &format!("m-{t}"),
        "payments-api",
        "payments-api",
        serde_json::json!({ "name": "payments-api" }),
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("under:{name}")))
        .await
        .unwrap();
    let under = response
        .coverage
        .iter()
        .find(|c| c.dimension == FilterDimension::Under)
        .expect("an under: query reports its coverage");
    let answer = |id: &str| {
        under
            .sources
            .iter()
            .find(|s| s.source_id == id)
            .map(|s| s.answer)
    };
    assert_eq!(answer("gitea"), Some(FilterAnswer::Answered));
    assert_eq!(answer("kuma"), Some(FilterAnswer::NoValues));
    assert!(
        !response
            .coverage
            .iter()
            .any(|c| c.dimension == FilterDimension::Author),
        "no author was asked about"
    );

    // And a query that asked no under: question reports nothing about it.
    let plain = searcher(&pool).search(q(&name)).await.unwrap();
    assert!(plain.coverage.is_empty());
}

/// A saved list holding `under:` stores it as plain text and runs it like any
/// query: the rail's count and the list's rows both come from the filter.
#[tokio::test]
async fn a_saved_list_holding_under_runs_it() {
    let pool = pool().await;
    let t = token();
    let name = format!("saved-{t}");
    let ours = repo(&pool, &format!("o{t}"), &name, "first").await;
    repo(&pool, &format!("o{t}"), &format!("other-{t}"), "second").await;

    let saved = searcher(&pool)
        .save_list(&format!("Everything in {name}"), &format!("under:{name}"))
        .await
        .unwrap();
    assert_eq!(
        saved.count, 4,
        "the rail counts the repo and its three children"
    );

    let opened = searcher(&pool)
        .smart_list_items(&saved.id, 30)
        .await
        .unwrap();
    assert_eq!(ids(&opened), sorted(ours));
}

// ---------------------------------------------------------------------------
// The estate (#600)
// ---------------------------------------------------------------------------
//
// An asset's parent is its `parent_id` column and a route's is the asset
// exposing it (ADR-0014): fields knobas keeps, not declared paths, so these are
// seeded straight into `knobas.asset` and `knobas.route` -- with the
// `path_text` the store would have written, since keeping it is
// `knobas_app::assets`' job and that crate depends on this one. The store's
// own move is witnessed through it, in `knobas-app`'s `search_ipc.rs`.

/// An asset and its entity row, under `parent` and at `path_text`.
async fn asset(pool: &sqlx::PgPool, name: &str, parent: Option<&str>, path_text: &str) -> String {
    let id = format!("asset:{}", uuid::Uuid::new_v4().simple());
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
    .bind(path_text)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A route `asset_id` exposes.
async fn route(pool: &sqlx::PgPool, asset_id: &str, name: &str) -> String {
    let id = format!("route:{}", uuid::Uuid::new_v4().simple());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'route',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.route (id, asset_id, name, url) values ($1,$2,$3,'http://x/')")
        .bind(&id)
        .bind(asset_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// The path line each hit carries, by entity id.
fn paths(response: &SearchResponse) -> std::collections::BTreeMap<String, Option<String>> {
    response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter())
        .map(|h| (h.row.entity_id.clone(), h.row.path.clone()))
        .collect()
}

/// `under:hetzner-1` is the whole subtree at any depth -- children,
/// grandchildren, the routes any of them expose -- and the asset itself, by
/// its name in any case. A sibling subtree and its routes stay out, and so do
/// the asset's ancestors.
#[tokio::test]
async fn under_an_asset_returns_its_whole_subtree_and_the_asset() {
    let pool = pool().await;
    let t = token();
    let site = format!("Site-{t}");
    let host = format!("Hetzner-1-{t}");
    let root = asset(&pool, &site, None, "").await;
    let hetzner = asset(&pool, &host, Some(&root), &site).await;
    let vm = asset(
        &pool,
        &format!("vm-{t}"),
        Some(&hetzner),
        &format!("{site} / {host}"),
    )
    .await;
    let pg = asset(
        &pool,
        &format!("pg-{t}"),
        Some(&vm),
        &format!("{site} / {host} / vm-{t}"),
    )
    .await;
    let on_host = route(&pool, &hetzner, &format!("console-{t}")).await;
    let on_pg = route(&pool, &pg, &format!("postgres-{t}")).await;
    // The sibling: beside hetzner-1 under the same site, with a route of its own.
    let other = asset(&pool, &format!("hetzner-2-{t}"), Some(&root), &site).await;
    route(&pool, &other, &format!("elsewhere-{t}")).await;

    let ours = sorted(vec![
        hetzner.clone(),
        vm.clone(),
        pg.clone(),
        on_host.clone(),
        on_pg.clone(),
    ]);
    for raw in [
        format!("under:{host}"),
        format!("under:{}", host.to_lowercase()),
        format!("UNDER:{}", host.to_uppercase()),
    ] {
        let response = searcher(&pool).search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), ours, "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names an asset and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }

    // Each row carries its ancestor chain; a route carries its asset's chain
    // and the asset itself, which is where it sits.
    let response = searcher(&pool)
        .search(q(&format!("under:{host}")))
        .await
        .unwrap();
    let lines = paths(&response);
    assert_eq!(lines[&hetzner].as_deref(), Some(site.as_str()));
    assert_eq!(
        lines[&vm].as_deref(),
        Some(format!("{site} / {host}").as_str())
    );
    assert_eq!(
        lines[&pg].as_deref(),
        Some(format!("{site} / {host} / vm-{t}").as_str())
    );
    assert_eq!(
        lines[&on_host].as_deref(),
        Some(format!("{site} / {host}").as_str())
    );
    assert_eq!(
        lines[&on_pg].as_deref(),
        Some(format!("{site} / {host} / vm-{t} / pg-{t}").as_str())
    );
}

/// Coverage does not name the estate as unable to answer `under:`: it answers,
/// so there is no gap to explain. Coverage lists configured sources, and the
/// estate is knobas' own and is none of them -- so an `asset:` scope reports
/// nothing, and a whole-corpus scope names only the source that really cannot
/// answer (a Kuma, which declares no parent) while the estate's rows come back.
#[tokio::test]
async fn coverage_does_not_name_the_estate_as_unable_to_answer() {
    let pool = pool().await;
    let t = token();
    let host = format!("cov-host-{t}");
    let root = asset(&pool, &host, None, "").await;
    let vm = asset(&pool, &format!("cov-vm-{t}"), Some(&root), &host).await;
    item(
        &pool,
        "kuma",
        "monitor",
        &format!("m-{t}"),
        "payments-api",
        "payments-api",
        serde_json::json!({ "name": "payments-api" }),
    )
    .await;

    let estate_only = searcher(&pool)
        .search(q(&format!("asset: under:{host}")))
        .await
        .unwrap();
    assert_eq!(ids(&estate_only), sorted(vec![root.clone(), vm.clone()]));
    assert!(
        estate_only.coverage.is_empty(),
        "nothing in an estate scope is unable to answer: {:?}",
        estate_only.coverage
    );

    let everywhere = searcher(&pool)
        .search(q(&format!("under:{host}")))
        .await
        .unwrap();
    assert_eq!(ids(&everywhere), sorted(vec![root, vm]));
    let under = everywhere
        .coverage
        .iter()
        .find(|c| c.dimension == FilterDimension::Under)
        .expect("the Kuma is in scope and cannot answer");
    for source in &under.sources {
        assert!(
            !["asset", "route"].contains(&source.source_id.as_str()),
            "the estate is reported on: {under:?}"
        );
    }
    assert!(
        under
            .sources
            .iter()
            .any(|s| s.source_id == "kuma" && s.answer == FilterAnswer::NoValues)
    );
}

/// A parent whose name has a space in it is reached by quoting the name, which
/// is how the query box completes one it suggests (#601): the tokenizer keeps a
/// quoted run together with its quotes, and `under:` reads the name inside
/// them.
#[tokio::test]
async fn a_quoted_name_with_a_space_scopes_like_any_other() {
    let pool = pool().await;
    let t = token();
    let name = format!("Payout Service {t}");
    let ours = repo(&pool, &format!("o{t}"), &name, "first").await;

    let raw = format!("under:\"{name}\"");
    let response = searcher(&pool).search(q(&raw)).await.unwrap();
    assert_eq!(ids(&response), sorted(ours));
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw} names a parent: {:?}",
        response.interpreted.unknown_tokens
    );
}
