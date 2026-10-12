//! The values the query box suggests for `kind:`, `source:` and `under:`
//! (#601, spec #591 stories 27-30), end to end against a real PostgreSQL.
//!
//! **Only values that exist right now** is the rule every test here holds the
//! read to: a kind with live rows, an enabled source with live rows (#614), a
//! parent some live item declares or a mirrored record answers to as one, or
//! an asset, which answers to its name (#600). A suggestion that led to an
//! empty result, or to an unknown token, would be a lie the box told.
//!
//! One database for the file, a unique token per test, nothing truncated: the
//! house rule of `search.rs`. The read answers over the whole database, so
//! each test asserts on the values that carry its own token and on the
//! absence of the values it seeded to be left out.

use knobas_search::{FilterSuggestions, ParentSuggestion, SearchFilters, SearchQuery, Searcher};

fn token() -> String {
    format!("s{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

/// The four sources [`pool`] configures: an enabled Gitea and Uptime Kuma,
/// and a disabled Gitea and TeamCity, whose rows `sync.live_item` hides.
const POOL_SOURCES: [(&str, &str, &str, bool); 4] = [
    ("gitea", "gitea", "Gitea", true),
    ("kuma", "kuma", "Uptime Kuma", true),
    ("gitea-off", "gitea", "Old Gitea", false),
    ("teamcity-off", "teamcity", "Old TeamCity", false),
];

/// A migrated database with the [`POOL_SOURCES`].
async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind, name, enabled) in POOL_SOURCES {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
             values ($1, $2, $3, 'http://x', 'Pat', $4)
             on conflict (id) do update set kind = excluded.kind, enabled = excluded.enabled",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .bind(enabled)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// The searcher the app builds: the real Gitea adapter's declarations.
fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::new(pool.clone()).with_payload_paths([knobas_source_gitea::descriptor_template()])
}

async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$4,'',$5, now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A commit in `owner/name`, stored the way the Gitea adapter stores one.
async fn commit(pool: &sqlx::PgPool, source: &str, owner: &str, name: &str) -> String {
    let sha = uuid::Uuid::new_v4().simple().to_string();
    item(
        pool,
        source,
        "commit",
        &format!("{owner}/{name}@{sha}"),
        serde_json::json!({
            "sha": sha,
            "repository": { "full_name": format!("{owner}/{name}"), "name": name },
        }),
    )
    .await
}

async fn asset(pool: &sqlx::PgPool, id: &str, parent: Option<&str>, name: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'asset', $2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, parent_id, type_id, name) values ($1, $2, 'vm', $3)",
    )
    .bind(id)
    .bind(parent)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

fn parent(name: &str, kind: &str) -> ParentSuggestion {
    ParentSuggestion {
        name: name.to_owned(),
        kind: kind.to_owned(),
    }
}

/// Search for `raw` and assert it is understood and finds something: what a
/// picked suggestion has to do, or the suggestion was not a value that exists.
async fn leads_somewhere(searcher: &Searcher, raw: &str) {
    let response = searcher
        .search(SearchQuery {
            raw: raw.to_owned(),
            limit: 30,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw} was suggested and is unknown: {:?}",
        response.interpreted.unknown_tokens
    );
    assert!(response.total > 0, "{raw} was suggested and finds nothing");
}

/// `under:` and `name` as the query box writes them when a suggestion is
/// picked: quoted when the name has whitespace in it, bare otherwise -- the
/// rule `complete()` in `app/src/lib/launcher/suggest.ts` follows (the
/// deputy's ruling of 2026-10-12 on #597). Probing this spelling, and not the
/// bare name, is what makes the probe the backend half of the box's round trip.
fn typed_under(name: &str) -> String {
    if name.chars().any(char::is_whitespace) {
        format!("under:\"{name}\"")
    } else {
        format!("under:{name}")
    }
}

/// The parents whose name carries this test's token, sorted.
fn parents_of(suggestions: &FilterSuggestions, t: &str) -> Vec<ParentSuggestion> {
    let mut out: Vec<ParentSuggestion> = suggestions
        .parents
        .iter()
        .filter(|p| p.name.contains(t))
        .cloned()
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.kind.cmp(&b.kind)));
    out
}

/// The tracer bullet: the kinds that have live rows, the enabled sources that
/// have them, and the parents the mirror declares, each with its kind -- and
/// nothing a disabled source holds.
#[tokio::test]
async fn the_read_offers_the_kinds_the_sources_and_the_parents_that_exist() {
    let pool = pool().await;
    let t = token();
    let owner = format!("Tidewater{t}");
    let name = format!("payout-{t}");
    item(
        &pool,
        "gitea",
        "repo",
        &format!("{owner}/{name}"),
        serde_json::json!({ "full_name": format!("{owner}/{name}"), "name": name }),
    )
    .await;
    commit(&pool, "gitea", &owner, &name).await;
    // A parent whose names have a space in them, so the probe below runs the
    // quoted spelling the box completes such a name with.
    let spaced = format!("Payout Desk {t}");
    commit(&pool, "gitea", &owner, &spaced).await;
    // A mirrored repo nothing sits in yet: no child declares it, and it is
    // still a parent `under:` reaches -- it answers to its own names.
    let lonely = format!("lonely-{t}");
    item(
        &pool,
        "gitea",
        "repo",
        &format!("{owner}/{lonely}"),
        serde_json::json!({ "full_name": format!("{owner}/{lonely}"), "name": lonely }),
    )
    .await;
    item(
        &pool,
        "kuma",
        "monitor",
        &format!("m-{t}"),
        serde_json::json!({ "name": "payments-api" }),
    )
    .await;
    // A disabled source's rows are not in the corpus: neither the kind only it
    // holds nor the parent only it names may be offered.
    item(
        &pool,
        "teamcity-off",
        "build",
        &format!("b-{t}"),
        serde_json::json!({}),
    )
    .await;
    commit(
        &pool,
        "gitea-off",
        &format!("ghost{t}"),
        &format!("gone-{t}"),
    )
    .await;

    let suggestions = searcher(&pool).filter_suggestions().await.unwrap();

    let kinds: Vec<&str> = suggestions.kinds.iter().map(|k| k.kind.as_str()).collect();
    for kind in ["commit", "monitor", "repo"] {
        assert!(kinds.contains(&kind), "{kind} has live rows: {kinds:?}");
    }
    assert!(
        !kinds.contains(&"build"),
        "only a disabled source holds a build: {kinds:?}"
    );

    // Other tests in this file configure sources of their own; this one
    // answers for the ones `pool()` configures.
    let sources: Vec<(&str, &str)> = suggestions
        .sources
        .iter()
        .filter(|s| POOL_SOURCES.iter().any(|(id, ..)| *id == s.id))
        .map(|s| (s.id.as_str(), s.display_name.as_str()))
        .collect();
    assert_eq!(sources, [("gitea", "Gitea"), ("kuma", "Uptime Kuma")]);

    assert_eq!(
        parents_of(&suggestions, &t),
        [
            parent(&spaced, "repo"),
            parent(&format!("{owner}/{spaced}"), "repo"),
            parent(&format!("{owner}/{lonely}"), "repo"),
            parent(&format!("{owner}/{name}"), "repo"),
            parent(&lonely, "repo"),
            parent(&name, "repo"),
        ],
    );
    assert!(
        parents_of(&suggestions, &t)
            .iter()
            .any(|p| typed_under(&p.name).contains('"')),
        "no offered parent has a space in its name, so the quoted spelling goes unprobed"
    );
    for p in parents_of(&suggestions, &t) {
        leads_somewhere(&searcher(&pool), &typed_under(&p.name)).await;
    }
    for k in &suggestions.kinds {
        leads_somewhere(&searcher(&pool), &format!("kind:{}", k.kind)).await;
    }
}

/// A parent named by children but not mirrored itself is still offered, with
/// the kind its children declare -- the reason the declaration carries a kind
/// at all (the deputy's ruling of 2026-10-11 on #594).
#[tokio::test]
async fn a_parent_only_its_children_name_is_still_offered() {
    let pool = pool().await;
    let t = token();
    let owner = format!("elsewhere{t}");
    let name = format!("unmirrored-{t}");
    commit(&pool, "gitea", &owner, &name).await;

    let suggestions = searcher(&pool).filter_suggestions().await.unwrap();

    assert_eq!(
        parents_of(&suggestions, &t),
        [
            parent(&format!("{owner}/{name}"), "repo"),
            parent(&name, "repo"),
        ],
    );
    for p in parents_of(&suggestions, &t) {
        leads_somewhere(&searcher(&pool), &typed_under(&p.name)).await;
    }
}

/// Every asset is offered by its name, as `asset`: one that holds another
/// asset, one that exposes a route, and a leaf that holds nothing -- `under:`
/// knows every asset's name and returns the asset itself (#600), so each one
/// leads somewhere, a spaced name typed quoted as the box writes it.
#[tokio::test]
async fn every_asset_is_offered_by_its_name_and_leads_somewhere() {
    let pool = pool().await;
    let t = token();
    asset(
        &pool,
        &format!("asset:site-{t}"),
        None,
        &format!("hel1-{t}"),
    )
    .await;
    asset(
        &pool,
        &format!("asset:vm-{t}"),
        Some(&format!("asset:site-{t}")),
        &format!("vm-{t}"),
    )
    .await;
    asset(
        &pool,
        &format!("asset:lone-{t}"),
        None,
        &format!("Lone Rack {t}"),
    )
    .await;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'route', 'web')")
        .bind(format!("route:{t}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.route (id, asset_id, name, url) values ($1, $2, 'web', 'http://x/')",
    )
    .bind(format!("route:{t}"))
    .bind(format!("asset:vm-{t}"))
    .execute(&pool)
    .await
    .unwrap();

    let suggestions = searcher(&pool).filter_suggestions().await.unwrap();

    assert_eq!(
        parents_of(&suggestions, &t),
        [
            parent(&format!("Lone Rack {t}"), "asset"),
            parent(&format!("hel1-{t}"), "asset"),
            parent(&format!("vm-{t}"), "asset"),
        ],
    );
    for p in parents_of(&suggestions, &t) {
        leads_somewhere(&searcher(&pool), &typed_under(&p.name)).await;
    }
    let kinds: Vec<&str> = suggestions.kinds.iter().map(|k| k.kind.as_str()).collect();
    assert!(
        kinds.contains(&"asset") && kinds.contains(&"route"),
        "{kinds:?}"
    );
}

/// Configure a Gitea source `id`, enabled or not.
async fn source(pool: &sqlx::PgPool, id: &str, enabled: bool) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'gitea', $1, 'http://x', 'Pat', $2)",
    )
    .bind(id)
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
}

/// The offered source ids that carry this test's token, in the order offered.
fn sources_of<'a>(suggestions: &'a FilterSuggestions, t: &str) -> Vec<&'a str> {
    suggestions
        .sources
        .iter()
        .filter(|s| s.id.contains(t))
        .map(|s| s.id.as_str())
        .collect()
}

/// A source is offered only once it has a live row (the deputy's ruling of
/// 2026-10-12 on #614): one configured but never synced would lead to an
/// empty result, which story 30 rules out. Every offered source is probed the
/// way the kinds and parents are, and the unsynced one is offered at the next
/// read once its first row lands.
#[tokio::test]
async fn a_source_is_offered_only_once_it_has_a_live_row() {
    let pool = pool().await;
    let t = token();
    let synced = format!("a-synced-{t}");
    let unsynced = format!("b-unsynced-{t}");
    let disabled = format!("c-disabled-{t}");
    source(&pool, &synced, true).await;
    source(&pool, &unsynced, true).await;
    source(&pool, &disabled, false).await;
    item(
        &pool,
        &synced,
        "repo",
        &format!("r-{t}"),
        serde_json::json!({}),
    )
    .await;
    item(
        &pool,
        &disabled,
        "repo",
        &format!("d-{t}"),
        serde_json::json!({}),
    )
    .await;

    let suggestions = searcher(&pool).filter_suggestions().await.unwrap();
    assert_eq!(sources_of(&suggestions, &t), [synced.as_str()]);
    for s in &suggestions.sources {
        leads_somewhere(&searcher(&pool), &format!("source:{}", s.id)).await;
    }

    item(
        &pool,
        &unsynced,
        "repo",
        &format!("u-{t}"),
        serde_json::json!({}),
    )
    .await;

    let suggestions = searcher(&pool).filter_suggestions().await.unwrap();
    assert_eq!(
        sources_of(&suggestions, &t),
        [synced.as_str(), unsynced.as_str()]
    );
    for s in &suggestions.sources {
        leads_somewhere(&searcher(&pool), &format!("source:{}", s.id)).await;
    }
}

/// Whether `under:` knows `name`, asked the way the box asks: the spelling a
/// picked suggestion is typed with, and no unknown token in the answer.
async fn under_knows(searcher: &Searcher, name: &str) -> bool {
    searcher
        .search(SearchQuery {
            raw: typed_under(name),
            limit: 1,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
        .interpreted
        .unknown_tokens
        .is_empty()
}

/// What `under:` knows and what the box offers for it are one set (#615).
///
/// They are read by two statements, `under`'s and `suggest`'s, over the same
/// three parent shapes: a parent only its children name, a mirrored parent
/// nothing sits in, and an asset. A shape only `under:` reads is a name the
/// box never offers; a shape only the box reads is an offer that greys out.
/// So every name the seed could answer to -- the parents' names and some that
/// are no parent's -- is asked of both, and the two answers must agree, in
/// both directions, for each shape.
#[tokio::test]
async fn under_knows_exactly_the_names_the_box_offers() {
    let pool = pool().await;
    let t = token();
    let owner = format!("Harbour{t}");
    // A parent only its children name: a commit in a repo the mirror lacks.
    let unmirrored = format!("Unmirrored Ledger {t}");
    commit(&pool, "gitea", &owner, &unmirrored).await;
    // A mirrored parent nothing sits in.
    let lonely = format!("lonely-{t}");
    item(
        &pool,
        "gitea",
        "repo",
        &format!("{owner}/{lonely}"),
        serde_json::json!({ "full_name": format!("{owner}/{lonely}"), "name": lonely }),
    )
    .await;
    // An asset.
    let rack = format!("Rack Four {t}");
    asset(&pool, &format!("asset:rack-{t}"), None, &rack).await;

    let searcher = searcher(&pool);
    let offered: Vec<String> = parents_of(&searcher.filter_suggestions().await.unwrap(), &t)
        .into_iter()
        .map(|p| p.name)
        .collect();

    let candidates = [
        unmirrored.clone(),
        format!("{owner}/{unmirrored}"),
        lonely.clone(),
        format!("{owner}/{lonely}"),
        rack.clone(),
        // No parent answers to these: the owner alone, and a name next to one.
        owner.clone(),
        format!("{lonely}-not"),
    ];
    let mut known = Vec::new();
    for name in &candidates {
        if under_knows(&searcher, name).await {
            known.push(name.clone());
        }
    }
    known.sort();

    for name in &offered {
        assert!(
            known.contains(name),
            "{name} is offered and under: does not know it"
        );
    }
    for name in &known {
        assert!(
            offered.contains(name),
            "under: knows {name} and the box never offers it"
        );
    }
    // The seed reaches every shape, so neither direction was asked of an
    // empty set, and the decoys were not known.
    let mut expected = vec![
        unmirrored.clone(),
        format!("{owner}/{unmirrored}"),
        lonely.clone(),
        format!("{owner}/{lonely}"),
        rack.clone(),
    ];
    expected.sort();
    assert_eq!(known, expected);
}
