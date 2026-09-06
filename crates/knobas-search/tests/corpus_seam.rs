//! The M4 seam, run rather than asserted about.
//!
//! Spec §4 marks *"asset search matches ancestor path names (searching
//! 'pve-02' finds the containers under it)"* as **Decided**, with the round-3
//! gap called out: the corpus indexed only the asset's own fields, and the
//! implementation must index the path. M1 has no assets, so what is
//! deliverable here is that adding them in M4 is a [`Corpus`] entry and a
//! migration -- not a rewrite of the search stack.
//!
//! An assertion to that effect is worth nothing. A **second corpus that
//! actually runs through the same builder** is worth the hour, so
//! `corpus::NOTE` (`knobas.note`, a different relation with a different id
//! space, constant `kind`/`source_id` and a composed `headline_text`) goes
//! through `sql::search_sql`, `sql::query_as_with`, `group::group` and
//! `snippet::segments` untouched.
//!
//! `corpus::NOTE` was test-only when this was written and ships in the
//! launcher's corpus list since #46. That makes the file *more* load-bearing
//! rather than redundant: what it still proves is the property the product
//! now depends on -- two corpora, one statement, one bind list, ranking
//! comparable across both -- and it drives the builder directly, so a
//! regression shows here before it shows in the launcher.
//!
//! Every test in this binary shares one database -- fresh per run, shared
//! across the tests in it -- so each seeds a token unique to itself.
//!
//! [`Corpus`]: knobas_search::corpus::Corpus

use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{query_as_with, search_sql};
use knobas_search::{KindCatalog, RawHit, corpus, group};

/// **Every corpus the launcher unions, and that is the point of the list.**
///
/// It was `[&LIVE_ITEM, &NOTE]` until #436, which is how `corpus::ASSET`
/// shipped in #428 and `corpus::ROUTE` in #432 without one of their fragments
/// ever passing through `search_sql` in this file -- the merge review of #458
/// caught it and named this line. `corpus::ALL` is deliberately *not* spelled
/// here: this file drives the builder directly, and a list that follows the
/// product's would stop being a statement about what has been run.
const CORPORA: &[&corpus::Corpus] = &[
    &corpus::LIVE_ITEM,
    &corpus::NOTE,
    &corpus::ASSET,
    &corpus::ROUTE,
];

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn token(tag: &str) -> String {
    format!("zc{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn seed_item(pool: &sqlx::PgPool, id: &str, kind: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, payload)
         values ($1,'jira',$2,$3,$4, now(), '{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

/// A note, seeded as its two rows rather than through `knobas_core::note`.
///
/// The entity row is not optional: `0006`'s `note_entity_fk` says a note that
/// is not an entity is a note nothing can link to. Written by hand here on
/// purpose -- this file is about the *corpus*, and going through the store
/// would make the seam test depend on the store's reconciliation.
async fn seed_note(pool: &sqlx::PgPool, id: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'note',$2)")
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.note (id, title, body_md) values ($1,$2,$3)")
        .bind(id)
        .bind(title)
        .bind(body)
        .execute(pool)
        .await
        .unwrap();
}

/// An asset and its entity row, seeded by hand for [`seed_note`]'s reason.
///
/// `path_text` is passed in rather than derived, because deriving it is
/// `knobas_app::assets`' job and this file is about the *corpus*: the store
/// lives in a crate that depends on this one, and reaching for it here would
/// make the seam test depend on the thing the seam exists to keep separate.
/// The store's own maintenance of the column is witnessed at the wire, in
/// `knobas-app`'s `search_ipc.rs`.
async fn seed_asset(
    pool: &sqlx::PgPool,
    id: &str,
    name: &str,
    parent: Option<&str>,
    path_text: &str,
    properties: serde_json::Value,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, parent_id, type_id, name, properties, path_text)
         values ($1,$2,'vm',$3,$4,$5)",
    )
    .bind(id)
    .bind(parent)
    .bind(name)
    .bind(properties)
    .bind(path_text)
    .execute(pool)
    .await
    .unwrap();
}

/// A route on an asset, seeded by hand for [`seed_note`]'s reason.
async fn seed_route(pool: &sqlx::PgPool, id: &str, asset_id: &str, name: &str, url: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'route',$2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.route (id, asset_id, name, url) values ($1,$2,$3,$4)")
        .bind(id)
        .bind(asset_id)
        .bind(name)
        .bind(url)
        .execute(pool)
        .await
        .unwrap();
}

async fn run(pool: &sqlx::PgPool, text: &str, limit: u32) -> Vec<knobas_search::ResultGroup> {
    let built = search_sql(
        CORPORA,
        Some(text),
        false,
        &EffectiveFilters::default(),
        10,
        limit,
    );
    let rows: Vec<RawHit> = query_as_with(built).fetch_all(pool).await.unwrap();
    group::group(rows, &KindCatalog::default())
}

#[tokio::test]
async fn a_second_corpus_joins_the_same_pipeline_untouched() {
    let pool = pool().await;
    let t = token("join");
    seed_item(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "SEPA retry",
        "nothing to do with the estate",
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-1"),
        &format!("{t} rebuild"),
        "Rebuild the hypervisor from the golden image.",
    )
    .await;

    // Grouping, ordering, totals and snippets are the same code as for
    // live_item: the second corpus needed no branch anywhere above sql.rs.
    let groups = run(&pool, &t, 20).await;
    assert_eq!(
        groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
        ["note"]
    );
    assert_eq!(groups[0].total, 1);
    assert!(groups[0].hits[0].snippet.iter().any(|s| s.hit));
    // The constant columns arrive as columns: a corpus whose `kind` and
    // `source_id` are literals is indistinguishable downstream from one whose
    // are real columns, which is the whole claim.
    assert_eq!(groups[0].hits[0].row.entity_id, format!("note:{t}-1"));
    assert_eq!(groups[0].hits[0].row.source_id, "note");
    assert_eq!(groups[0].hits[0].row.kind, "note");
    assert_eq!(groups[0].plural, "Notes");
    // `headline_text` composes two columns that are not the title, and the
    // excerpt is quoted from the composition.
    let joined: String = groups[0].hits[0]
        .snippet
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert!(joined.contains("hypervisor"), "{joined:?}");
}

/// Two corpora, one query: the union must interleave by rank rather than put
/// one relation's rows first -- otherwise M4's assets would always sort above
/// or below the work items regardless of how well they matched.
///
/// The page is the observable: the final result is ordered by *kind* (the
/// launcher's fixed groups), so which rows survive `limit` is the only place a
/// corpus-first ordering shows. Two strong matches, one per corpus, and two
/// weak ones; a page of two must be the two strong ones.
#[tokio::test]
async fn ranking_is_comparable_across_corpora() {
    let pool = pool().await;
    let t = token("rank");
    let filler = "ledger reconciliation window batch payout backlog";

    seed_item(
        &pool,
        &format!("jira:{t}-strong"),
        "ticket",
        &format!("{t} {t} {t}"),
        filler,
    )
    .await;
    seed_item(
        &pool,
        &format!("jira:{t}-weak"),
        "ticket",
        "Reconcile the ledger",
        &format!("{filler} {filler} {t}"),
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-strong"),
        &format!("{t} {t} {t}"),
        filler,
    )
    .await;
    seed_note(
        &pool,
        &format!("note:{t}-weak"),
        "Runbook",
        &format!("{filler} {filler} {t}"),
    )
    .await;

    // All four match, so the page really is a choice between them.
    let all = run(&pool, &t, 20).await;
    assert_eq!(all.iter().map(|g| g.total).sum::<u32>(), 4);

    let page = run(&pool, &t, 2).await;
    let mut ids: Vec<&str> = page
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [format!("jira:{t}-strong"), format!("note:{t}-strong")],
        "a page of two must be the two best matches, one from each corpus"
    );
    // And both corpora are still *counted* in full: the limit cut the page,
    // not the totals.
    assert_eq!(page.iter().map(|g| g.total).sum::<u32>(), 4);
}

/// The bind list is per *query*, not per branch.
///
/// `Builder::param` exists for exactly this: each branch repeats the text and
/// filter placeholders and must reuse the same `$n`. A builder that pushed a
/// duplicate per branch would still produce runnable SQL for one corpus and
/// would silently read the wrong argument for two.
#[tokio::test]
async fn a_second_corpus_adds_no_binds_and_still_executes() {
    let pool = pool().await;
    let filters = EffectiveFilters {
        sources: vec!["jira".to_owned()],
        kinds: vec!["ticket".to_owned(), "note".to_owned()],
        updated_within_days: Some(30),
        mine: false,
        named_authors: Vec::new(),
        identity_authors: Vec::new(),
    };
    let one = search_sql(&[&corpus::LIVE_ITEM], Some("sepa"), true, &filters, 10, 20);
    let two = search_sql(CORPORA, Some("sepa"), true, &filters, 10, 20);
    assert_eq!(one.bind_count(), two.bind_count());
    assert!(two.sql().contains("union all"));
    // Executing it is the half a shape assertion cannot do: a placeholder that
    // drifted from its argument is a type error the *server* raises. The row
    // set is beside the point here -- that the statement parsed, bound and
    // decoded into `RawHit` at all is the assertion.
    let _rows: Vec<RawHit> = query_as_with(two).fetch_all(&pool).await.unwrap();
}

/// **The estate goes through the builder too, path and properties and all**
/// (#436).
///
/// The two corpora M4.0 added are the ones this file was written *for*: its
/// module docs open with spec §4's *"asset search matches ancestor path
/// names"*, and until now the claim rested on [`corpus::NOTE`] standing in for
/// them. Now they are here, and each carries something no corpus above it has:
///
/// * [`corpus::ASSET`]'s `path` is a **column** rather than
///   `ancestor_path_read!` over a payload -- the first corpus for which
///   "where is this" is knobas' own answer;
/// * [`corpus::ROUTE`]'s `relation` is a **join**, so its `path` is read off a
///   *second* table's row, and its own `fts` deliberately does not carry it.
///
/// One query over four corpora, and every one of the four is asked. The
/// mirror's ticket carries the token for the reason the one in
/// `search_ipc.rs` does: without a row the union has to rank *against*, an
/// estate-only answer is not evidence that the estate was searched.
#[tokio::test]
async fn the_estates_two_corpora_go_through_the_same_pipeline() {
    let pool = pool().await;
    let t = token("estate");

    seed_item(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        &format!("{t} certificate renewal"),
        "nothing to do with the estate",
    )
    .await;
    let site = format!("asset:{t}-site");
    let vm = format!("asset:{t}-vm");
    seed_asset(
        &pool,
        &site,
        &format!("{t} hel1"),
        None,
        "",
        serde_json::json!({}),
    )
    .await;
    seed_asset(
        &pool,
        &vm,
        &format!("{t} db"),
        Some(&site),
        &format!("{t} hel1"),
        serde_json::json!({ "hostname": format!("{t}-db-01"), "os": "Debian 13" }),
    )
    .await;
    let route = format!("route:{t}-kuma");
    // The shape every route in `testenv/hetzner/estate.json` has -- a loopback
    // host and the port that is the only thing telling the nine of them apart
    // (ADR-0013: the real container is the witness). The port is unique to this
    // file, so no other test in this binary can match it.
    seed_route(&pool, &route, &vm, &format!("{t} kuma"), "http://127.0.0.1:53001/").await;

    // The token is on every seeded row, so one query asks all four corpora and
    // the answer is the union's.
    let groups = run(&pool, &t, 20).await;
    let kinds: Vec<&str> = groups.iter().map(|g| g.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["ticket", "asset", "route"],
        "one query, four corpora, and `group::group`'s fixed order over all of them"
    );

    let assets = &groups[1];
    assert_eq!(assets.plural, "Assets");
    let vm_hit = assets
        .hits
        .iter()
        .find(|hit| hit.row.entity_id == vm)
        .expect("the VM under the site");
    // The path is the estate's own column, not the mirror's `null`, and a root
    // asset's is `null` rather than an empty line under every site.
    assert_eq!(vm_hit.row.path.as_deref(), Some(format!("{t} hel1").as_str()));
    assert_eq!(
        assets
            .hits
            .iter()
            .find(|hit| hit.row.entity_id == site)
            .expect("the site")
            .row
            .path,
        None
    );
    assert_eq!(vm_hit.row.kind, "asset");
    assert_eq!(vm_hit.row.source_id, "asset");

    let routes = &groups[2];
    assert_eq!(routes.plural, "Routes");
    let route_hit = &routes.hits[0];
    assert_eq!(route_hit.row.entity_id, route);
    // A route sits **on** the asset exposing it, which is that asset's own path
    // plus its name -- one level deeper than the asset's own answer.
    assert_eq!(
        route_hit.row.path.as_deref(),
        Some(format!("{t} hel1 / {t} db").as_str())
    );

    // A property **value** finds the asset that carries it; the property
    // **key** does not, which is `0019`'s decision run rather than asserted.
    let by_hostname = run(&pool, &format!("{t}-db-01"), 20).await;
    let found: Vec<&str> = by_hostname
        .iter()
        .flat_map(|g| &g.hits)
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(found, [vm.as_str()], "the hostname finds the VM");
    // And the excerpt can quote the reason: `headline_text` carries the
    // property text, so the hit is not a row with no visible cause.
    let quoted: String = by_hostname[0].hits[0]
        .snippet
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert!(quoted.contains(&format!("{t}-db-01")), "{quoted:?}");
    assert!(
        by_hostname[0].hits[0].snippet.iter().any(|s| s.hit),
        "a property match is highlighted like any other"
    );

    // **A route is found by its URL** -- by the port, which on the estate that
    // exists is the only thing telling nine otherwise identical loopback URLs
    // apart. `to_tsvector` gives a port a lexeme of its own when the URL ends
    // at `/`, so this is an exact match and not a prefix one: `run` asks with
    // `prefix_last_term` false, so nothing here rests on the reader still
    // typing.
    let by_port = run(&pool, "53001", 20).await;
    let ports: Vec<&str> = by_port
        .iter()
        .flat_map(|g| &g.hits)
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(ports, [route.as_str()], "the port finds the route");
}

/// **Where a URL stops being reachable**, measured rather than assumed (#436).
///
/// `corpus::ROUTE`'s docs carry the table this asserts. It is a test and not
/// only prose because the boundary is not where anybody would guess -- it is
/// the **path** that fuses a URL's parts together, not the port -- and because
/// the next reader to widen `0018`'s `fts` needs a red test to work against
/// rather than a paragraph to measure again.
///
/// Both negatives are of the *index*, asked with `prefix_last_term` false.
/// While the last word is still being typed the first is reachable as a
/// prefix, which is a hit that vanishes when the reader presses space; the
/// second is out of reach at every stage.
#[tokio::test]
async fn a_path_in_a_url_takes_its_port_and_its_segments_out_of_reach() {
    let pool = pool().await;
    let t = token("url");

    let vm = format!("asset:{t}-vm");
    seed_asset(&pool, &vm, &format!("{t} vm"), None, "", serde_json::json!({})).await;
    let route = format!("route:{t}-r");
    seed_route(
        &pool,
        &route,
        &vm,
        &format!("{t} kuma"),
        "http://127.0.0.1:54002/dashboard",
    )
    .await;

    // The route exists and is findable -- by its name -- so neither assertion
    // below is green because the fixture is missing.
    let by_name = run(&pool, &format!("{t} kuma"), 20).await;
    assert!(
        by_name
            .iter()
            .flat_map(|g| &g.hits)
            .any(|hit| hit.row.entity_id == route),
        "the route is in the corpus"
    );

    // The port, fused to the path: the lexeme is `54002/dashboard`.
    assert!(run(&pool, "54002", 20).await.is_empty());
    // The path segment, which heads no lexeme and so is out of reach even
    // mid-typing.
    assert!(run(&pool, "dashboard", 20).await.is_empty());
}

/// **A name outranks an ancestor's name outranks a property**, which is the
/// whole of what `0017`'s weights and `0019`'s last rung are for.
///
/// Three assets, one query, and the order is the assertion: the machine
/// *called* `<token>` first, the machine *under* it second, the machine merely
/// *mentioning* it in a property third. Without the ranking a search for a
/// hostname would put every container beneath that host above the host itself.
#[tokio::test]
async fn a_name_outranks_a_path_outranks_a_property() {
    let pool = pool().await;
    let t = token("weight");

    let named = format!("asset:{t}-named");
    let under = format!("asset:{t}-under");
    let mentions = format!("asset:{t}-mentions");
    seed_asset(&pool, &named, t.as_str(), None, "", serde_json::json!({})).await;
    seed_asset(
        &pool,
        &under,
        &format!("child of {}", &t[..6]),
        Some(&named),
        t.as_str(),
        serde_json::json!({}),
    )
    .await;
    seed_asset(
        &pool,
        &mentions,
        &format!("mentions {}", &t[..6]),
        None,
        "",
        serde_json::json!({ "image": t.as_str() }),
    )
    .await;

    let groups = run(&pool, &t, 20).await;
    let order: Vec<&str> = groups[0]
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(
        order,
        [named.as_str(), under.as_str(), mentions.as_str()],
        "name (A) then ancestor path (B) then property (C)"
    );
}
