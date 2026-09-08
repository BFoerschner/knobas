//! The four §2.4 search commands, exercised through their bodies.
//!
//! A `#[tauri::command]` cannot be called without a window, so what is tested
//! here is the `*_inner` function each command is a two-line shim over -- the
//! pattern `tests/demo.rs` already uses for `demo_load_inner`. `tests/ipc.rs`
//! covers the other half (that the argument shapes decode and the handler list
//! dispatches); this file covers what the shims are *for*: the wire shape the
//! hand-written TypeScript mirrors, the error codes the frontend branches on,
//! and the one DTO that is a composition of two streams' data.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`), and
//! libtest does not order them. So: nothing truncates, every corpus assertion
//! is scoped to a token this test seeded, and no assertion is made about an
//! absolute row count of anything global.

use knobas_app::IpcErrorCode;
use knobas_app::commands::search::{
    create_smart_list_inner, delete_smart_list_inner, launcher_home_inner, rename_smart_list_inner,
    search_inner, smart_list_items_inner, smart_lists_inner,
};
use knobas_search::{SearchFilters, SearchQuery};

/// A pool with the schema on it.
async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A token nothing else in this binary can match.
fn token(tag: &str) -> String {
    format!("zi{tag}{}", uuid::Uuid::new_v4().simple())
}

/// One ticket and one PR carrying `tag`, plus a source to attribute them to.
///
/// The source row is what `launcher_home` reads its health out of, so it has
/// to exist before that assertion -- and it is inserted with `on conflict do
/// nothing` because another test in this binary may have got there first.
async fn seed(pool: &sqlx::PgPool, tag: &str) -> String {
    let t = token(tag);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(pool)
    .await
    .unwrap();

    for (suffix, kind, title, body) in [
        (
            "PAY-231",
            "ticket",
            format!("Retry failed {t} payouts"),
            format!("The {t} batch gives up after one retry instead of backing off."),
        ),
        (
            "142",
            "pr",
            format!("Backoff for the {t} retry loop"),
            format!("Adds jittered backoff to the {t} worker."),
        ),
    ] {
        let id = format!("jira:{t}-{suffix}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
            .bind(&id)
            .bind(kind)
            .bind(&title)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item
               (entity_id, source_id, kind, title, body_text, author, item_updated_at,
                synced_at, payload)
             values ($1,'jira',$2,$3,$4,'mara.lindqvist', now() - interval '1 day',
                     now() - interval '4 minutes', '{}'::jsonb)",
        )
        .bind(&id)
        .bind(kind)
        .bind(&title)
        .bind(&body)
        .execute(pool)
        .await
        .unwrap();
    }
    t
}

fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 20,
        filters: SearchFilters::default(),
    }
}

/// The field names are the contract.
///
/// `app/src/lib/ipc/search.ts` is written by hand, so a rename on this side is
/// a field the frontend reads as `undefined` -- no compile error on either
/// half. The crate's own unit tests pin the shape of a `SearchHit` against the
/// mirror's text; this pins the shape of a **live answer**, which is the only
/// place `interpreted`, `groups` and `took_ms` appear together and the only
/// place the flatten is observed on data that came out of PostgreSQL.
#[tokio::test]
async fn the_wire_shape_is_the_contract() {
    let pool = pool().await;
    let t = seed(&pool, "wire").await;

    let response = search_inner(&pool, query(&t)).await.unwrap();
    let v = serde_json::to_value(&response).unwrap();

    assert!(v["interpreted"]["unknown_tokens"].is_array(), "{v}");
    assert!(v["groups"][0]["monogram"].is_string(), "{v}");
    assert!(v["groups"][0]["plural"].is_string(), "{v}");
    assert!(v["groups"][0]["total"].is_number(), "{v}");
    // `SearchHit` flattens its `EntityRow`: the six identity fields are inline
    // and there is no `row` object. Both halves are asserted, because the
    // attribute can only be lost in the direction that adds the nesting back.
    assert!(
        v["groups"][0]["hits"][0]["entity_id"].is_string(),
        "SearchHit flattens its row: {v}"
    );
    assert!(v["groups"][0]["hits"][0]["row"].is_null(), "{v}");
    assert!(v["groups"][0]["hits"][0]["synced_at"].is_string(), "{v}");
    assert!(
        v["groups"][0]["hits"][0]["snippet"][0]["hit"].is_boolean(),
        "{v}"
    );
    assert!(
        v["groups"][0]["hits"][0]["snippet"][0]["text"].is_string(),
        "{v}"
    );
    assert!(v["took_ms"].is_number(), "{v}");
    assert!(v["total"].is_number(), "{v}");

    // And the answer is about the rows this test seeded, so the assertions
    // above are made against a populated response rather than an empty one.
    assert_eq!(response.groups.len(), 2, "a ticket and a PR: {v}");
    assert_eq!(response.total, 2);
}

/// The excerpt crosses the bridge as **data**, never as markup.
///
/// Roadmap §4 gotcha 7. The frontend renders `segment.text` as text and the
/// `hit` flag is what draws the `<mark>`; if a highlighter sentinel ever
/// reached the wire as a tag, a component author following the doc comment
/// would render it inertly and the highlight would silently disappear -- or,
/// worse, someone would reach for `{@html}` to make it work again.
#[tokio::test]
async fn no_markup_reaches_the_wire_in_a_snippet() {
    let pool = pool().await;
    let t = seed(&pool, "mkup").await;

    let response = search_inner(&pool, query(&t)).await.unwrap();
    let segments: Vec<_> = response
        .groups
        .iter()
        .flat_map(|group| group.hits.iter())
        .flat_map(|hit| hit.snippet.iter())
        .collect();

    assert!(!segments.is_empty(), "the seeded rows have excerpts");
    assert!(
        segments.iter().any(|segment| segment.hit),
        "the match itself is flagged, not marked up: {segments:?}"
    );
    for segment in &segments {
        assert!(
            !segment.text.contains('<') && !segment.text.contains('\u{1}'),
            "a segment carries markup or a raw sentinel: {segment:?}"
        );
    }
}

/// Errors carry a code the frontend can branch on (ruling P1).
///
/// `not_found` is what makes a stale `list:` in the box say *no such list*
/// instead of *something went wrong*; the message is for humans and nobody
/// parses it.
#[tokio::test]
async fn errors_carry_a_code_the_frontend_can_branch_on() {
    let pool = pool().await;

    let unknown = smart_list_items_inner(&pool, "nope", 10).await.unwrap_err();
    assert_eq!(unknown.code, IpcErrorCode::NotFound, "{unknown:?}");
    assert!(unknown.message.contains("nope"), "{unknown:?}");

    // A query outside the engine's bounds is the caller's fault, not the
    // database's: `invalid`, so the box can say so rather than offering
    // *Retry*.
    let huge = search_inner(&pool, query(&"p".repeat(1_000)))
        .await
        .unwrap_err();
    assert_eq!(huge.code, IpcErrorCode::Invalid, "{huge:?}");

    // And a database failure is `internal`. Taken against a closed pool so
    // the mapping is observed rather than argued: `SearchError::Db` is the one
    // arm no bad argument can reach.
    let closed = knobas_db::test_util::test_pool().await;
    closed.close().await;
    let dead = search_inner(&closed, query("sepa")).await.unwrap_err();
    assert_eq!(dead.code, IpcErrorCode::Internal, "{dead:?}");
}

/// The board reports source health beside the lists (interfaces §2.4).
///
/// `LauncherHome` is the one DTO in this stream that composes two streams'
/// data: the lists and the recent rows are `knobas-search`'s, `sources` is
/// stream F's `CredentialHealth`, and `pending_writes` is stream G's write
/// queue. What is asserted here is that all four halves arrive and are shaped
/// as the mirror declares them; the *value* of the count belongs to
/// `launcher_home_counts_the_pending_writes`, which is the only test in this
/// binary that queues anything and therefore the only one that may say a
/// number out loud about a table the whole binary shares.
#[tokio::test]
async fn launcher_home_reports_source_health_beside_the_lists() {
    let pool = pool().await;
    seed(&pool, "home").await;

    let home = launcher_home_inner(&pool).await.unwrap();
    let v = serde_json::to_value(&home).unwrap();

    for field in ["smart_lists", "recent", "sources", "pending_writes"] {
        assert!(!v[field].is_null(), "{field} missing: {v}");
    }
    assert!(
        v["pending_writes"].is_u64(),
        "the footer renders this as a number: {v}"
    );

    // §2.2's `CredentialHealth` shape, taken from stream F's own type rather
    // than from a second copy of it -- and `unknown` because nothing has
    // tested the credential (0002's default).
    let jira = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_id"] == "jira")
        .unwrap_or_else(|| panic!("the seeded source is missing: {v}"));
    assert_eq!(jira["state"], "unknown", "{jira}");
    for field in [
        "source_id",
        "state",
        "checked_at",
        "detail",
        "secret_expires_at",
    ] {
        assert!(jira.get(field).is_some(), "{field} missing: {jira}");
    }

    // The board's own two halves are populated and shaped as the mirror
    // declares them: a smart list with a count and a badge, and recent rows
    // carrying the provenance stamp §4 renders "synced 4 min ago" from.
    assert!(!home.smart_lists.is_empty(), "M1 ships built-in lists");
    let list = serde_json::to_value(&home.smart_lists[0]).unwrap();
    for field in ["id", "label", "count", "changed", "description"] {
        assert!(list.get(field).is_some(), "{field} missing: {list}");
    }
    assert!(!home.recent.is_empty(), "this test seeded two items");
    assert!(v["recent"][0]["synced_at"].is_string(), "{v}");
}

/// The footer's number is the write queue's *pending* rows (issue #212).
///
/// A delta rather than an absolute, for the reason this file's header gives:
/// the binary shares one database and `write_queue::counts` is global, so what
/// can be pinned is the movement. Both directions are asserted, because the
/// direction is the decision: queueing a write raises the count, and *holding*
/// that same write lowers it again -- "N pending writes" says be patient, and a
/// held write is asking for a decision instead (`QueueCounts`' own rule that
/// "3 waiting" may never absorb a held write).
#[tokio::test]
async fn launcher_home_counts_the_pending_writes() {
    let pool = pool().await;
    let entity = knobas_core::entity::EntityRef::new("jira", &token("wq"));

    let before = launcher_home_inner(&pool).await.unwrap().pending_writes;

    let queued = knobas_core::write_queue::queue(
        &pool,
        "jira",
        &entity,
        "comment",
        serde_json::json!({ "text": "queued" }),
        serde_json::json!({}),
    )
    .await
    .unwrap();
    assert_eq!(
        launcher_home_inner(&pool).await.unwrap().pending_writes,
        before + 1,
        "a queued write is one the launcher still owes a source"
    );

    knobas_core::write_queue::hold(&pool, queued.id, serde_json::json!({}))
        .await
        .unwrap()
        .expect("the write was pending, so it can be held");
    assert_eq!(
        launcher_home_inner(&pool).await.unwrap().pending_writes,
        before,
        "a held write waits for the user, not for the network"
    );
}

/// `smart_lists` answers the same summaries the board embeds.
///
/// Two commands, one source of truth: the launcher refreshes the rail without
/// re-reading the whole board, and a divergence between them would show as a
/// count that changes when nothing did.
#[tokio::test]
async fn the_rail_and_the_board_report_the_same_lists() {
    let pool = pool().await;
    seed(&pool, "rail").await;

    let standalone = smart_lists_inner(&pool).await.unwrap();
    let embedded = launcher_home_inner(&pool).await.unwrap().smart_lists;

    let ids = |lists: &[knobas_search::SmartListSummary]| {
        lists.iter().map(|l| l.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(&standalone), ids(&embedded));
    assert!(!standalone.is_empty());
}

/// Opening a list answers with a `SearchResponse`, so the launcher renders it
/// with the code it renders results with.
#[tokio::test]
async fn a_smart_list_answers_in_the_search_shape() {
    let pool = pool().await;
    seed(&pool, "list").await;

    let response = smart_list_items_inner(&pool, "just-synced", 20)
        .await
        .unwrap();
    let v = serde_json::to_value(&response).unwrap();

    assert_eq!(v["interpreted"]["prefix"], "list", "{v}");
    assert!(v["groups"].is_array(), "{v}");
    assert!(v["took_ms"].is_number(), "{v}");
    // A list row has no excerpt and no rank -- it was not matched against
    // anything -- and the *shape* is still a search's, which is the point.
    let hit = &v["groups"][0]["hits"][0];
    assert!(hit["entity_id"].is_string(), "{v}");
    assert_eq!(hit["snippet"], serde_json::json!([]), "{v}");
}

/// **The Tree's search box, at the wire** (#430, story 30).
///
/// The box sends this exact query — the raw text and `kinds = ["asset"]`,
/// which is `app/src/lib/assets/tree.ts`'s `estateQuery` — and reveals the
/// path of the first hit it gets back. Everything above the wire is asserted
/// against a fake in `AssetsView.test.svelte.ts`; what a fake cannot say is
/// whether `corpus::ASSET` answers this filter at all, whether the hit's
/// `path` is the estate's path or the mirror's `null`, and whether the filter
/// keeps the mirror's own rows out. So it is said here, on a real database,
/// through the same function the command is a shim over.
///
/// The seeded ticket and PR carry the same token as the assets on purpose:
/// without a row the filter has to *exclude*, an asset-only answer is not
/// evidence that anything was filtered.
#[tokio::test]
async fn the_trees_search_answers_with_assets_and_their_paths() {
    let pool = pool().await;
    let t = seed(&pool, "estate").await;

    let site = knobas_app::assets::create(&pool, None, "site", &format!("site {t}"), &[])
        .await
        .unwrap()
        .value;
    let vm = knobas_app::assets::create(&pool, Some(&site.id), "vm", &format!("vm {t}"), &[])
        .await
        .unwrap()
        .value;
    let container =
        knobas_app::assets::create(&pool, Some(&vm.id), "container", &format!("pg {t}"), &[])
            .await
            .unwrap()
            .value;

    // What the box sends.
    let mut narrowed = query(&format!("pg {t}"));
    narrowed.filters.kinds = vec!["asset".to_owned()];
    let response = search_inner(&pool, narrowed).await.unwrap();

    let kinds: Vec<_> = response.groups.iter().map(|g| g.kind.as_str()).collect();
    assert_eq!(kinds, ["asset"], "the mirror's rows are out: {kinds:?}");
    let hits = &response.groups[0].hits;
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].row.entity_id, container.id);
    assert_eq!(hits[0].row.title, format!("pg {t}"));
    // The path is what the offer line shows and what makes two containers
    // called `postgres` tellable apart -- the ancestors' names, outermost
    // first, and never the asset's own.
    assert_eq!(
        hits[0].row.path.as_deref(),
        Some(format!("site {t} / vm {t}").as_str())
    );

    // Spec §4's own example, which is why the corpus indexes the path at all:
    // searching for the site finds the machines *under* it, and the site
    // itself ranks first because its name is weighted above its descendants'
    // paths.
    let mut ancestors = query(&format!("site {t}"));
    ancestors.filters.kinds = vec!["asset".to_owned()];
    let found = search_inner(&pool, ancestors).await.unwrap();
    let named: Vec<_> = found.groups[0]
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.clone())
        .collect();
    assert_eq!(
        named.first(),
        Some(&site.id),
        "the site outranks what it holds: {named:?}"
    );
    let mut found_ids = named.clone();
    found_ids.sort();
    let mut want = vec![site.id.clone(), vm.id.clone(), container.id.clone()];
    want.sort();
    // The two machines under it tie on rank -- neither name matched, both
    // paths did -- so what is asserted about them is that they are *there*,
    // which is the half spec §4 is about. Their order between themselves is
    // the engine's and nothing reads it.
    assert_eq!(found_ids, want, "{named:?}");

    // A root asset sits nowhere, and the corpus answers that with `null`
    // rather than an empty path line under every site in the estate.
    assert_eq!(found.groups[0].hits[0].row.path, None);
}

/// **The estate browses, so the timer picker can offer an asset** (#437).
///
/// The picker's second list is one `search` with no text and `kinds: ["asset"]`
/// -- a *browse*, which the engine orders by recency and answers without a
/// tsquery. Every frontend test of that list is made against a fake, so this is
/// the only place the wire itself is witnessed: that a text-free query carrying
/// only a kind filter is not short-circuited as "an empty box", that
/// `corpus::ASSET` is what answers it, and that the hit carries the asset's
/// path -- which is what the picker draws under the name so two containers
/// called `postgres` are told apart.
///
/// Deliberately **not** the `asset:` prefix, which parses to the same filter:
/// the picker sets the filter itself, and that is the mechanism under test. The
/// prefix was short-circuited to nothing when this was written and answers from
/// the estate since #436; `the_asset_prefix_answers_from_the_estate_with_its_path`
/// below is where the two are held to the same answer.
#[tokio::test]
async fn a_text_free_query_filtered_to_assets_browses_the_estate() {
    let pool = pool().await;
    let name = token("estate");
    let site = knobas_app::assets::create(&pool, None, "site", &format!("site {name}"), &[])
        .await
        .expect("a site")
        .value;
    let vm = knobas_app::assets::create(&pool, Some(&site.id), "vm", &format!("vm {name}"), &[])
        .await
        .expect("a VM inside it")
        .value;

    let browse = SearchQuery {
        raw: String::new(),
        limit: 50,
        filters: SearchFilters {
            kinds: vec!["asset".to_owned()],
            ..SearchFilters::default()
        },
    };
    let answer = search_inner(&pool, browse)
        .await
        .expect("the estate browses");

    let assets = answer
        .groups
        .iter()
        .find(|group| group.kind == "asset")
        .unwrap_or_else(|| {
            panic!(
                "a browse filtered to assets answered with no asset group: {:?}",
                answer.groups.iter().map(|g| &g.kind).collect::<Vec<_>>()
            )
        });
    let hit = assets
        .hits
        .iter()
        .find(|hit| hit.row.entity_id == vm.id)
        .expect("the VM this test made is in the estate's browse");
    assert_eq!(hit.row.title, format!("vm {name}"));
    assert_eq!(
        hit.row.path.as_deref(),
        Some(format!("site {name}").as_str()),
        "the hit carries where the asset sits, which is what the picker draws \
         under its name"
    );
    assert!(
        assets.hits.iter().any(|hit| hit.row.entity_id == site.id),
        "the browse is the estate and not one level of it"
    );
}

/// **`asset:` answers, and the launcher's own estate hit** (#436).
///
/// The prefix was **short-circuited** from M1 until this ticket: it parsed, it
/// set `kinds = ["asset"]`, and `Searcher::search` then returned an empty
/// response without reaching a corpus. The rows had existed since #428 and the
/// greyed-out prefix was the honest state while nothing drew an estate row.
/// This is the wire half of turning it on -- that the prefix now reaches
/// `corpus::ASSET`, that what comes back carries the path from the root, and
/// that `coverage` is computed for it *like any other kind* rather than left
/// empty by a branch that never ran.
///
/// Through `knobas_app::assets::create` and not by hand: `path_text` is the
/// store's to maintain and `props_text` is the database's to generate, and a
/// test that inserted both itself would witness neither.
#[tokio::test]
async fn the_asset_prefix_answers_from_the_estate_with_its_path() {
    let pool = pool().await;
    let t = seed(&pool, "prefix").await;

    let site = knobas_app::assets::create(&pool, None, "site", &format!("site {t}"), &[])
        .await
        .unwrap()
        .value;
    let vm = knobas_app::assets::create(&pool, Some(&site.id), "vm", &format!("vm {t}"), &[])
        .await
        .unwrap()
        .value;

    // The prefix, typed, and nothing else: no filter set by the caller.
    let response = search_inner(&pool, query(&format!("asset: {t}")))
        .await
        .unwrap();
    assert_eq!(
        response.interpreted.prefix,
        Some(knobas_search::Prefix::Asset)
    );
    assert_eq!(response.interpreted.filters.kinds, ["asset"]);
    let kinds: Vec<_> = response.groups.iter().map(|g| g.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["asset"],
        "`asset:` reaches the estate and keeps the mirror's rows out: {kinds:?}"
    );
    let hit = response.groups[0]
        .hits
        .iter()
        .find(|hit| hit.row.entity_id == vm.id)
        .expect("the VM the prefix was typed for");
    assert_eq!(hit.row.path.as_deref(), Some(format!("site {t}").as_str()));

    // **The prefix and the filter it parses to now answer the same thing**,
    // which is the whole of what the short-circuit broke and the sharpest way
    // to say it: the Tree's box has set `kinds = ["asset"]` by hand since #430
    // and always reached the corpus, while the prefix that parses to exactly
    // that filter returned nothing. Groups *and* coverage, because coverage
    // was the other half `empty()` skipped -- and "assets are reported on like
    // any other kind" is what that comes to.
    let mut by_filter = query(&t);
    by_filter.filters.kinds = vec!["asset".to_owned()];
    let filtered = search_inner(&pool, by_filter).await.unwrap();
    let named = |answer: &knobas_search::SearchResponse| {
        answer
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .map(|hit| hit.row.entity_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(named(&response), named(&filtered));
    assert_eq!(response.total, filtered.total);
    assert_eq!(response.coverage.len(), filtered.coverage.len());

    // And with an author filter on top, both are the *same* empty report --
    // empty because no source has synced an asset, which is the reason
    // `coverage_of` gives for leaving a source out, and not because a branch
    // never ran. `note:` answers identically, which is the point of "like any
    // other kind".
    let with_author = |raw: &str, kinds: Vec<String>| {
        let mut q = query(raw);
        q.filters.authors = vec!["jonas".to_owned()];
        q.filters.kinds = kinds;
        q
    };
    let by_prefix = search_inner(&pool, with_author(&format!("asset: {t}"), Vec::new()))
        .await
        .unwrap();
    let by_kind = search_inner(&pool, with_author(&t, vec!["asset".to_owned()]))
        .await
        .unwrap();
    assert_eq!(by_prefix.coverage, by_kind.coverage);
    assert_eq!(by_prefix.groups.len(), by_kind.groups.len());
}

/// **A hostname finds the VM, and a port finds the route** (#436, migration
/// `0019`).
///
/// The ticket's two "find it by what it *is*" criteria, at the wire and through
/// the real store. The hostname is the half `0019` adds: `0017` left an asset's
/// property values out of `fts` because *"what a search for '8080' should mean
/// is a decision"*, and this is that decision read back on a database that ran
/// the migration. The port is the half that already worked, asserted here
/// because nothing at the wire said so and because the URL shape is the real
/// estate's rather than an invented one -- `corpus::ROUTE`'s docs carry where
/// that stops, and `corpus_seam.rs` carries the negatives.
///
/// The mirror's ticket and PR carry the same token, so an estate-only answer
/// is evidence that the estate was searched rather than evidence that nothing
/// else matched.
#[tokio::test]
async fn a_property_value_and_a_url_are_what_a_reader_types() {
    use knobas_app::assets::{PropertyValue, Visibility};

    let pool = pool().await;
    let t = seed(&pool, "props").await;
    let host = format!("{t}-db-01");

    let site = knobas_app::assets::create(&pool, None, "site", &format!("site {t}"), &[])
        .await
        .unwrap()
        .value;
    let vm = knobas_app::assets::create(
        &pool,
        Some(&site.id),
        "vm",
        &format!("vm {t}"),
        &[
            (
                "hostname".to_owned(),
                PropertyValue::Text {
                    value: host.clone(),
                },
            ),
            (
                "os".to_owned(),
                PropertyValue::Text {
                    value: "Debian 13".to_owned(),
                },
            ),
        ],
    )
    .await
    .unwrap()
    .value;

    let found = search_inner(&pool, query(&host)).await.unwrap();
    let ids: Vec<_> = found
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(ids, [vm.id.as_str()], "the hostname finds the VM");
    // And it is drawn as an estate hit: the path is what tells two machines
    // with the same name apart.
    assert_eq!(
        found.groups[0].hits[0].row.path.as_deref(),
        Some(format!("site {t}").as_str())
    );

    // A **key** is schema and finds nothing: `hostname` stands on every VM in
    // the estate, so a query for it that answered with all of them would be a
    // launcher answering a question nobody asked.
    let by_key = search_inner(&pool, query(&format!("hostname {t}")))
        .await
        .unwrap();
    assert!(
        !by_key
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .any(|hit| hit.row.entity_id == vm.id),
        "the property key is not indexed; only its value is"
    );

    // The route, at the shape every route in `testenv/hetzner/estate.json`
    // actually has -- a loopback host, no path, and the port that is the only
    // thing telling nine of them apart (ADR-0013).
    let route = knobas_app::assets::create_route(
        &pool,
        &vm.id,
        &format!("Uptime Kuma {t}"),
        "http://127.0.0.1:53002/",
        None,
        Visibility::Internal,
        &[],
    )
    .await
    .unwrap()
    .value;

    let by_port = search_inner(&pool, query("53002")).await.unwrap();
    let ports: Vec<_> = by_port
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(ports, [route.id.as_str()], "the port finds the route");
    // And it opens at the asset exposing it: the row carries that asset's own
    // path *plus its name*, which is where the route sits.
    assert_eq!(
        by_port.groups[0].hits[0].row.path.as_deref(),
        Some(format!("site {t} / vm {t}").as_str())
    );
}

// ---------------------------------------------------------------------------
// The three estate smart lists (#504)
// ---------------------------------------------------------------------------
//
// **A scratch database each, unlike the rest of this file.** Every one of these
// lists is a whole-estate aggregate -- "how many assets has nobody attached a
// monitor to" is a question about the *table*, not about a token -- so there is
// no per-test token that can isolate a count, and the delta trick `knobas-
// search`'s own `tests/lists.rs` uses cannot be applied to a fixture that is a
// whole file. `knobas_db::test_util::scratch_database` is what the estate tests
// in `adapter_to_mirror.rs` already reach for, for the same reason.
//
// **The estate is the real file**, `testenv/hetzner/estate.json`, imported
// through `knobas_app::assets::apply_import`: the same door `--demo` and
// `estate_exit.rs` use. Not twenty-three assets typed out here, because a
// fixture written beside the assertion agrees with it by construction, and
// because two of these rules run over the *tree* -- an alert routed through an
// ancestor needs an ancestor somebody drew on purpose.

/// The estate the three lists are read over -- the real file.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

/// How many assets that file holds. A literal, and deliberately not counted out
/// of the file: a count derived from the fixture agrees with the fixture
/// however wrong the import is.
const ESTATE_ASSETS: i64 = 23;

/// One mirrored monitor, and what its payload says about a certificate.
///
/// By hand, the way `assets_ipc.rs` seeds one: what is under test here is the
/// **read**, and `crates/knobas-source-kuma` is where "the adapter writes this
/// key" is the claim. `cert_days` is `None` for a monitor watching no
/// certificate -- which is most of them, and which is the miss the predicate's
/// `jsonb_typeof` guard has to survive.
async fn mirror_monitor(pool: &sqlx::PgPool, name: &str, cert_days: Option<f64>) -> String {
    let id = format!("kuma:{}", name.replace(|c: char| !c.is_alphanumeric(), "-"));
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'monitor',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .expect("the monitor's entity row");
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         values ($1,'kuma','monitor',$2,'', now(), now(),
                 jsonb_build_object('state','up','cert_days_remaining', $3::float8))",
    )
    .bind(&id)
    .bind(name)
    .bind(cert_days)
    .execute(pool)
    .await
    .expect("the mirrored monitor");
    id
}

/// An unarchived context holding one entity by a confirmed link.
async fn context_holding(pool: &sqlx::PgPool, key: &str, entity: &str) -> String {
    let id = format!("ctx:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ctx',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .expect("the context's entity row");
    sqlx::query("insert into knobas.context (id, kind, title) values ($1,'adhoc',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .expect("the context");
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
         values ($1,$2,'related','manual','test', now())",
    )
    .bind(&id)
    .bind(entity)
    .execute(pool)
    .await
    .expect("the confirmed link");
    id
}

/// One open alert on a monitor.
async fn open_alert(pool: &sqlx::PgPool, monitor: &str) {
    sqlx::query("insert into knobas.monitor_alert (entity_id, state) values ($1,'down')")
        .bind(monitor)
        .execute(pool)
        .await
        .expect("the open alert");
}

/// The estate, three monitors on it, and the links the import draws.
///
/// The three monitors are the three cases the certificate rule has to tell
/// apart, and they are attached to three different branches of the tree so no
/// two lists can be satisfied by the same asset:
///
/// * `gitea` -- five days left, on `asset:knobas-gitea`, under
///   `asset:orbstack-docker`.
/// * `knobas-jira` -- **31** days left, on `asset:hetzner-jira`. The
///   certificate negative the criterion asks for by name.
/// * `jira (tunnel)` -- no certificate at all, on `asset:knobas-jira`. The miss
///   direction: a payload with the key absent contributes nothing rather than
///   raising.
///
/// The names are the estate file's own, so the links are drawn by the import's
/// resolution rule rather than inserted here -- which is what makes *Not
/// monitored*'s negative a real attachment and not a row this test wrote.
async fn estate_with_monitors(label: &str) -> sqlx::PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    mirror_monitor(&pool, "gitea", Some(5.0)).await;
    mirror_monitor(&pool, "knobas-jira", Some(31.0)).await;
    mirror_monitor(&pool, "jira (tunnel)", None).await;

    let outcome = knobas_app::assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the estate imports")
        .value;
    assert_eq!(
        outcome.assets_created, ESTATE_ASSETS,
        "the real estate file is what these lists are read over"
    );
    assert_eq!(
        outcome.monitors_linked, 3,
        "the three seeded names resolved; the file's other four are still \
         waiting on a monitor nobody has mirrored"
    );
    pool
}

/// The count of one list, off the board the launcher draws.
async fn list_count(pool: &sqlx::PgPool, id: &str) -> i64 {
    smart_lists_inner(pool)
        .await
        .expect("the board")
        .into_iter()
        .find(|list| list.id == id)
        .unwrap_or_else(|| panic!("no list {id}"))
        .count
}

/// Every asset id one list answers with, in the order it answers.
async fn list_rows(pool: &sqlx::PgPool, id: &str) -> Vec<String> {
    smart_list_items_inner(pool, id, 200)
        .await
        .expect("the list's rows")
        .groups
        .into_iter()
        .flat_map(|group| group.hits)
        .map(|hit| hit.row.entity_id)
        .collect()
}

/// **Not monitored, at the wire** -- and it is the Monitors tab's roster.
///
/// The count, the rows and the negative in one test because they are one claim:
/// the number on the launcher's rail and the rows behind it come from the same
/// predicate, which is the failure `knobas_search::lists` calls the worst one
/// available. The comparison against `assets::unmonitored_assets` is the other
/// half -- two statements in two crates hard-coding the same two words, with no
/// compiler between them.
#[tokio::test]
async fn the_launchers_not_monitored_list_is_the_monitors_tabs_roster() {
    let pool = estate_with_monitors("lists-unmonitored").await;

    let rows = list_rows(&pool, "not-monitored").await;
    assert_eq!(
        list_count(&pool, "not-monitored").await,
        rows.len() as i64,
        "the rail's number and the rows behind it are one predicate"
    );
    assert_eq!(
        rows.len() as i64,
        ESTATE_ASSETS - 3,
        "three of the estate's assets have a monitor attached and the rest do not"
    );

    // The negative the criterion asks for: an asset something watches is absent.
    for watched in [
        "asset:knobas-gitea",
        "asset:hetzner-jira",
        "asset:knobas-jira",
    ] {
        assert!(
            !rows.contains(&watched.to_owned()),
            "{watched} has a confirmed monitored-by link and must not be on the roster"
        );
    }
    assert!(
        rows.contains(&"asset:notebook".to_owned()),
        "an asset nothing watches is: {rows:?}"
    );

    // And the same question asked of the Monitors tab, which is the read this
    // list was written from.
    let tab: Vec<String> = knobas_app::assets::unmonitored_assets(&pool)
        .await
        .expect("the tab's roster")
        .into_iter()
        .map(|asset| asset.id)
        .collect();
    assert_eq!(
        rows, tab,
        "the launcher and the Monitors tab answer the same roster, in the same \
         order -- path, then name, then id"
    );
}

/// **Open alerts in my contexts, at the wire** -- routed by membership, and
/// through an ancestor.
///
/// The context holds `asset:orbstack-docker`, which is the *parent* of the
/// asset the alerting monitor watches. So the row that comes back is there
/// through ADR-0008's `held` recursion and not through a direct link, which is
/// the half of *"directly or through an ancestor"* a flat membership rule would
/// get wrong while passing every other assertion here.
#[tokio::test]
async fn an_open_alert_reaches_the_list_through_the_contexts_ancestors() {
    let pool = estate_with_monitors("lists-alerts").await;

    open_alert(&pool, "kuma:gitea").await;
    // ...and one on an asset no context holds. The negative the criterion asks
    // for by name: `asset:hetzner-jira` is in the estate, has a monitor, and is
    // nobody's business.
    open_alert(&pool, "kuma:knobas-jira").await;

    assert_eq!(
        list_rows(&pool, "alerts-in-context").await,
        Vec::<String>::new(),
        "with no context at all, an open alert is nobody's business -- spec \
         #427 story 60"
    );

    context_holding(&pool, "shipping", "asset:orbstack-docker").await;

    let rows = list_rows(&pool, "alerts-in-context").await;
    assert_eq!(
        rows,
        ["asset:knobas-gitea"],
        "the container under the engine the context holds, and nothing else"
    );
    assert_eq!(
        list_count(&pool, "alerts-in-context").await,
        1,
        "the rail's number and the rows behind it are one predicate"
    );

    // An archived context is one the reader put away, and stops routing.
    sqlx::query("update knobas.context set archived_at = now() where id = 'ctx:shipping'")
        .execute(&pool)
        .await
        .expect("the archive");
    assert_eq!(
        list_rows(&pool, "alerts-in-context").await,
        Vec::<String>::new(),
        "an archived context routes nothing"
    );
}

/// An **acked** alert is still open, and this is not the inbox.
///
/// The one place this list and the inbox's sixth rule deliberately differ. Ack
/// clears the inbox item and leaves the alert open (#446, story 62), and every
/// other surface -- the Assets view's strip, the top strip's badge -- keeps
/// showing it. A predicate that copied the inbox's `acked_at is null` would
/// make this list empty itself the moment somebody said "seen".
#[tokio::test]
async fn an_acked_alert_is_still_open_and_still_on_the_list() {
    let pool = estate_with_monitors("lists-alerts-acked").await;
    open_alert(&pool, "kuma:gitea").await;
    context_holding(&pool, "shipping", "asset:orbstack-docker").await;

    sqlx::query("update knobas.monitor_alert set acked_at = now() where entity_id = 'kuma:gitea'")
        .execute(&pool)
        .await
        .expect("the ack");

    assert_eq!(
        list_rows(&pool, "alerts-in-context").await,
        ["asset:knobas-gitea"],
        "an ack is seen, not fixed"
    );

    // Recovery is what takes it off, and it is the same clause the reconciler
    // writes.
    sqlx::query("update knobas.monitor_alert set closed_at = now() where entity_id = 'kuma:gitea'")
        .execute(&pool)
        .await
        .expect("the recovery");
    assert_eq!(
        list_rows(&pool, "alerts-in-context").await,
        Vec::<String>::new()
    );
}

/// **Certificates expiring, at the wire** -- with the 31-day negative.
///
/// Three monitors, three answers: five days is on the list, 31 days is not, and
/// a payload with no `cert_days_remaining` at all is not -- the third being the
/// miss direction ADR-0007 asks a payload read to fail in. The 31-day row is
/// the negative the criterion names, and the asset it watches is *in the
/// estate* and *does have a monitor*, so its absence is the window and nothing
/// else.
#[tokio::test]
async fn only_a_certificate_inside_the_window_is_expiring() {
    let pool = estate_with_monitors("lists-certs").await;

    let rows = list_rows(&pool, "certs-expiring").await;
    assert_eq!(
        rows,
        ["asset:knobas-gitea"],
        "five days left is expiring; 31 days is not, and no certificate at all \
         is not"
    );
    assert_eq!(
        list_count(&pool, "certs-expiring").await,
        1,
        "the rail's number and the rows behind it are one predicate"
    );

    // The boundary itself, walked on the one monitor whose reading this test
    // moves rather than asserted from a second fixture: 31 is out (above), 30
    // is out and 29 is in, which is what *under 30 days* means.
    for (days, on_the_list) in [(30.0_f64, false), (29.0_f64, true)] {
        sqlx::query(
            "update sync.item
                set payload = jsonb_set(payload, '{cert_days_remaining}', to_jsonb($1::float8))
              where entity_id = 'kuma:knobas-jira'",
        )
        .bind(days)
        .execute(&pool)
        .await
        .expect("the new reading");
        assert_eq!(
            list_rows(&pool, "certs-expiring")
                .await
                .contains(&"asset:hetzner-jira".to_owned()),
            on_the_list,
            "a certificate with {days} days left"
        );
    }
}

/// A payload that stopped carrying a number empties the list rather than
/// failing the board.
///
/// The `jsonb_typeof` guard, at the wire. Without it the `::numeric` cast
/// raises on the first drifted row and `launcher_home` -- the whole board, not
/// this one list -- comes back as an error. The direction matters: a launcher
/// that will not open is worse than a list that says nothing.
#[tokio::test]
async fn a_certificate_reading_that_is_not_a_number_is_a_miss_and_not_a_failure() {
    let pool = estate_with_monitors("lists-certs-drift").await;

    sqlx::query(
        "update sync.item
            set payload = jsonb_set(payload, '{cert_days_remaining}', '\"nine\"'::jsonb)
          where entity_id = 'kuma:gitea'",
    )
    .execute(&pool)
    .await
    .expect("the drifted payload");

    assert_eq!(
        list_rows(&pool, "certs-expiring").await,
        Vec::<String>::new()
    );
    // And the board still loads, which is the half that would have been an
    // outage.
    assert!(
        launcher_home_inner(&pool)
            .await
            .expect("the board still loads")
            .smart_lists
            .iter()
            .any(|list| list.id == "certs-expiring")
    );
}

/// The three estate lists badge and clear like the four mirror lists.
///
/// One test over all three, because what is under test is the mechanism they
/// share: `SUMMARY_SQL`'s per-list stamp against the one `knobas.setting` row,
/// and `smart_list_items` writing that row as it answers. The stamps are
/// **three different expressions** -- an asset's own `updated_at`, an alert's
/// `opened_at`, a monitor's `synced_at` -- so a list whose stamp did not exist
/// would never badge and one whose stamp was somebody else's would never clear.
#[tokio::test]
async fn every_estate_list_badges_until_it_is_opened() {
    let pool = estate_with_monitors("lists-estate-badge").await;
    open_alert(&pool, "kuma:gitea").await;
    context_holding(&pool, "shipping", "asset:orbstack-docker").await;

    let badged = |pool: sqlx::PgPool| async move {
        smart_lists_inner(&pool)
            .await
            .expect("the board")
            .into_iter()
            .filter(|list| list.changed)
            .map(|list| list.id)
            .collect::<Vec<_>>()
    };

    let before = badged(pool.clone()).await;
    for id in ["not-monitored", "alerts-in-context", "certs-expiring"] {
        assert!(
            before.contains(&id.to_owned()),
            "{id} holds something and has never been opened: {before:?}"
        );
        smart_list_items_inner(&pool, id, 20)
            .await
            .expect("opening the list");
    }

    let after = badged(pool).await;
    for id in ["not-monitored", "alerts-in-context", "certs-expiring"] {
        assert!(
            !after.contains(&id.to_owned()),
            "{id} was just opened and still claims to be new: {after:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Saved smart lists (#506, spec #491 stories 56-58 and 60)
// ---------------------------------------------------------------------------

/// A database of this test's own.
///
/// Unlike everything above, a saved list is **global state on the board**: the
/// row this makes is on every other test's rail, and the counts are read over
/// a corpus this binary's other tests are also writing to. So these tests take
/// a scratch database each, the way the estate-list tests do, and can then say
/// what the whole rail is rather than only what is somewhere on it.
async fn saved_pool(label: &str) -> sqlx::PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// One mirrored row, with the author and the synced stamp the tests need.
///
/// `synced` is an **interval bound as a parameter** and added to the server's
/// `now()`, never a stamp the test computed: "this row arrived after the list
/// was opened" has to be one clock's answer, and a Rust `Utc::now()` compared
/// against a PostgreSQL `now()` is two.
async fn mirror_row(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    title: &str,
    author: &str,
    synced: &str,
) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,'jira',$2,$3,$4,$5, now() - interval '1 day',
                 now() + $6::interval, '{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .bind(title)
    .bind(author)
    .bind(synced)
    .execute(pool)
    .await
    .unwrap();
}

/// The whole rail, as ids.
async fn rail(pool: &sqlx::PgPool) -> Vec<knobas_search::SmartListSummary> {
    smart_lists_inner(pool).await.expect("the rail")
}

/// One list off the rail.
async fn rail_row(pool: &sqlx::PgPool, id: &str) -> knobas_search::SmartListSummary {
    rail(pool)
        .await
        .into_iter()
        .find(|list| list.id == id)
        .unwrap_or_else(|| panic!("no list {id} on the rail"))
}

/// **The whole of story 56, at the wire**: a saved query keeps its text, its
/// chips and its prefix, and the number beside it counts what it matches.
///
/// The corpus is built so that every clause of `#<token> @mara` has something
/// to exclude: a ticket by somebody else (the author chip), a PR by Mara (the
/// `#` prefix's kind), and a ticket by Mara that does not carry the token (the
/// text). Without all three an answer of two rows is not evidence that
/// anything was narrowed -- it is evidence that the corpus is small.
#[tokio::test]
async fn a_saved_query_keeps_its_text_its_chips_and_its_prefix() {
    let pool = saved_pool("saved-keeps").await;
    let t = token("keep");
    let stamp = "-4 minutes";
    mirror_row(
        &pool,
        "jira:A",
        "ticket",
        &format!("{t} one"),
        "mara",
        stamp,
    )
    .await;
    mirror_row(
        &pool,
        "jira:B",
        "ticket",
        &format!("{t} two"),
        "mara",
        stamp,
    )
    .await;
    mirror_row(
        &pool,
        "jira:C",
        "ticket",
        &format!("{t} three"),
        "jonas",
        stamp,
    )
    .await;
    mirror_row(&pool, "jira:D", "pr", &format!("{t} four"), "mara", stamp).await;
    mirror_row(
        &pool,
        "jira:E",
        "ticket",
        "nothing to do with it",
        "mara",
        stamp,
    )
    .await;

    let raw = format!("#{t} @mara");
    let made = create_smart_list_inner(&pool, "Mara's open ones", &raw)
        .await
        .expect("the query is one the grammar runs");

    // The row as the rail draws it, on the wire the panel reads.
    let wire = serde_json::to_value(&made).unwrap();
    assert_eq!(wire["id"], "mara-s-open-ones", "{wire}");
    assert_eq!(wire["label"], "Mara's open ones", "{wire}");
    assert_eq!(wire["saved"], true, "{wire}");
    assert_eq!(wire["needs_attention"], false, "{wire}");
    // A saved list's blurb is the query it stands for.
    assert_eq!(wire["description"], raw, "{wire}");
    assert_eq!(
        wire["count"], 2,
        "the two tickets Mara wrote carrying {t}: {wire}"
    );

    // And on the rail itself: after the built-ins, never among them.
    let listed = rail(&pool).await;
    let saved: Vec<&knobas_search::SmartListSummary> =
        listed.iter().filter(|list| list.saved).collect();
    assert_eq!(saved.len(), 1, "one saved list");
    assert_eq!(saved[0].id, made.id);
    assert_eq!(
        listed.iter().position(|list| list.saved),
        Some(listed.len() - 1),
        "the built-ins come first and the saved ones after"
    );
    assert!(
        listed.iter().filter(|list| !list.saved).count() >= 4,
        "the built-ins are still on the rail beside it"
    );

    // Opening it: the rows the count promised, and the *saved query's* own
    // interpretation -- which is what draws the chip row above the results.
    let opened = smart_list_items_inner(&pool, &made.id, 20)
        .await
        .expect("the saved list opens");
    let v = serde_json::to_value(&opened).unwrap();
    assert_eq!(v["interpreted"]["prefix"], "ticket", "{v}");
    assert_eq!(v["interpreted"]["text"], t, "{v}");
    assert_eq!(
        v["interpreted"]["filters"]["authors"],
        serde_json::json!(["mara"]),
        "{v}"
    );
    assert_eq!(
        v["interpreted"]["filters"]["kinds"],
        serde_json::json!(["ticket"]),
        "{v}"
    );
    assert_eq!(opened.total, 2, "{v}");
    let mut ids: Vec<String> = opened
        .groups
        .iter()
        .flat_map(|group| group.hits.iter().map(|hit| hit.row.entity_id.clone()))
        .collect();
    // Sorted, because the order is the *search's* -- rank, then recency -- and
    // these two rows tie on rank and differ only by the microsecond they were
    // inserted in. Which rows is the claim here; `search.rs`' own suite is
    // where the ordering is pinned.
    ids.sort();
    assert_eq!(ids, ["jira:A", "jira:B"], "{v}");
    assert_eq!(
        opened
            .groups
            .iter()
            .map(|g| g.kind.as_str())
            .collect::<Vec<_>>(),
        ["ticket"],
        "the PR is out because of the prefix, not because it did not match: {v}"
    );
}

/// **The count on the rail and the rows behind it are one predicate.**
///
/// The failure `knobas_search::lists` calls the worst one available -- a
/// number on screen that no test comparing the list against itself can see --
/// and for a saved list the two come from *two generated statements*
/// (`sql::saved_summary_sql` and `sql::search_sql`), which is one more chance
/// to disagree than a built-in has. Several query shapes, because the shapes
/// are where a builder diverges: text alone, a chip alone, a prefix, a window,
/// and the combination.
#[tokio::test]
async fn a_saved_lists_count_is_the_number_of_rows_it_answers_with() {
    let pool = saved_pool("saved-count").await;
    let t = token("count");
    // **Two rows that do not carry the token**, and they are not decoration:
    // without them every row in the corpus matches the text, and a summary
    // statement that dropped the `@@` predicate altogether would agree with
    // the page it is supposed to be checked against. That mutant survived this
    // test until these two rows existed.
    let other = token("other");
    for (id, kind, author, days) in [
        ("jira:1", "ticket", "mara", 1),
        ("jira:2", "ticket", "mara", 40),
        ("jira:3", "pr", "mara", 1),
        ("jira:4", "ticket", "jonas", 1),
        ("jira:5", "build", "jonas", 90),
        ("jira:6", "ticket", "mara", 1),
        ("jira:7", "ticket", "jonas", 1),
    ] {
        let t = if id == "jira:6" || id == "jira:7" {
            other.clone()
        } else {
            t.clone()
        };
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
             on conflict (id) do nothing",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
            .bind(id)
            .bind(kind)
            .bind(format!("{t} {id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item
               (entity_id, source_id, kind, title, body_text, author, item_updated_at,
                synced_at, payload)
             values ($1,'jira',$2,$3,$3,$4, now() - make_interval(days => $5),
                     now() - interval '4 minutes', '{}'::jsonb)",
        )
        .bind(id)
        .bind(kind)
        .bind(format!("{t} {id}"))
        .bind(author)
        .bind(days)
        .execute(&pool)
        .await
        .unwrap();
    }

    for (name, raw) in [
        ("text alone", t.clone()),
        ("an author chip", format!("{t} @mara")),
        ("a prefix", format!("#{t}")),
        ("a window", format!("{t} updated:7d")),
        ("all of them", format!("#{t} @mara updated:7d")),
        ("a query that matches nothing", format!("{t} @nobody")),
    ] {
        let made = create_smart_list_inner(&pool, name, &raw)
            .await
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let opened = smart_list_items_inner(&pool, &made.id, 200)
            .await
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(
            u64::try_from(made.count).expect("a count is not negative"),
            u64::from(opened.total),
            "{name} ({raw}): the rail says {} and the list answers with {}",
            made.count,
            opened.total
        );
        // A pair of zeroes agrees about nothing, so every shape but the last
        // one has to have found something.
        if name != "a query that matches nothing" {
            assert!(made.count > 0, "{name} ({raw}) counted nothing");
        } else {
            assert_eq!(made.count, 0, "{name} ({raw})");
        }
    }
}

/// The change badge, in both directions, on a list nobody shipped.
///
/// Story 57: *"a saved list shows a live count and a change badge like the
/// built-ins"*. The badge is `max(synced_at)` in the list against the stamp in
/// `knobas.setting`, and both are the database's clock -- the new row is
/// inserted at `now() + interval '1 second'` so that "after it was opened" is
/// a fact rather than a race on two statements that could read the same
/// microsecond.
#[tokio::test]
async fn a_saved_lists_badge_lights_when_the_corpus_moves_and_clears_when_it_is_opened() {
    let pool = saved_pool("saved-badge").await;
    let t = token("badge");
    mirror_row(
        &pool,
        "jira:one",
        "ticket",
        &format!("{t} first"),
        "mara",
        "-1 hour",
    )
    .await;

    let made = create_smart_list_inner(&pool, "Payments", &t)
        .await
        .expect("saved");
    assert!(
        rail_row(&pool, &made.id).await.changed,
        "it holds something and has never been opened"
    );

    smart_list_items_inner(&pool, &made.id, 20)
        .await
        .expect("opening it clears the badge");
    assert!(
        !rail_row(&pool, &made.id).await.changed,
        "it was just opened and still claims to be new"
    );
    assert_eq!(rail_row(&pool, &made.id).await.count, 1);

    mirror_row(
        &pool,
        "jira:two",
        "ticket",
        &format!("{t} second"),
        "mara",
        "1 second",
    )
    .await;
    let after = rail_row(&pool, &made.id).await;
    assert!(after.changed, "a sync brought a new row into the list");
    assert_eq!(after.count, 2, "and the count moved with it");
}

/// **Story 60**: a saved query today's grammar refuses is one line on the rail
/// and not an error, and the board still draws.
///
/// The rows are written with SQL, and that is the point rather than a
/// shortcut: `create_smart_list` runs the grammar first and refuses a query it
/// could not run, so **no command can make one of these**. What a row like
/// this is, is what a *grammar change* leaves behind -- which is the state
/// story 60 asks the launcher to survive, and the only way to have one is to
/// write it the way an older knobas would have.
#[tokio::test]
async fn a_saved_query_the_grammar_refuses_reads_needs_attention_and_the_board_still_draws() {
    let pool = saved_pool("saved-attention").await;
    let t = token("att");
    mirror_row(
        &pool,
        "jira:ok",
        "ticket",
        &format!("{t} fine"),
        "mara",
        "-4 minutes",
    )
    .await;
    let healthy = create_smart_list_inner(&pool, "Still fine", &t)
        .await
        .expect("saved");

    // Four rows an older grammar could have written and this one cannot run:
    // a prefix that is not a search, a list of a list, a paste, and a query
    // whose every token the parser no longer keeps.
    for (id, label, query) in [
        ("legacy-palette", "Palette", "> palette".to_owned()),
        ("legacy-alias", "Alias", "list:mine".to_owned()),
        ("legacy-paste", "Paste", "x".repeat(600)),
        // A source the reader has since deleted: the alias resolves to
        // nothing, so the saved query has no text and no filter left in it --
        // and `0025`'s own check constraint is why the row is `/gone` and not
        // a blank string, which the table refuses outright.
        ("legacy-gone", "Gone", "/gone".to_owned()),
    ] {
        sqlx::query("insert into knobas.smart_list (id, label, query) values ($1,$2,$3)")
            .bind(id)
            .bind(label)
            .bind(&query)
            .execute(&pool)
            .await
            .unwrap();
    }

    let listed = rail(&pool).await;
    for id in [
        "legacy-palette",
        "legacy-alias",
        "legacy-paste",
        "legacy-gone",
    ] {
        let row = listed
            .iter()
            .find(|list| list.id == id)
            .unwrap_or_else(|| panic!("{id} is not on the rail at all"));
        assert!(row.needs_attention, "{id}: {row:?}");
        assert!(row.saved, "{id}: {row:?}");
        assert_eq!(row.count, 0, "{id} was never asked: {row:?}");
        assert!(!row.changed, "{id} was never asked: {row:?}");
        assert!(
            row.description
                .starts_with(knobas_search::saved::NEEDS_ATTENTION),
            "{id}: {}",
            row.description
        );
        // Each says *which* rule refused it, or the reader is told something
        // is wrong and never what.
        assert!(
            row.description.len() > knobas_search::saved::NEEDS_ATTENTION.len() + 20,
            "{id}: {}",
            row.description
        );
    }

    // The rest of the rail is untouched -- which is the half of story 60 that
    // says "rather than an error".
    let still = listed
        .iter()
        .find(|list| list.id == healthy.id)
        .expect("the healthy saved list");
    assert!(!still.needs_attention);
    assert_eq!(still.count, 1);
    assert!(
        listed.iter().filter(|list| !list.saved).count() >= 4,
        "the built-ins are still there"
    );

    // And the whole board draws, which is what a launcher opening actually
    // asks for.
    let home = launcher_home_inner(&pool).await.expect("the board");
    assert_eq!(home.smart_lists.len(), listed.len());
    assert!(!home.recent.is_empty());

    // Typing `list:<id>` for one answers with the reason, not with an empty
    // page: a refusal a reader can act on beats a list that looks empty.
    let refused = smart_list_items_inner(&pool, "legacy-palette", 20)
        .await
        .expect_err("a list that cannot run cannot be opened");
    assert_eq!(refused.code, IpcErrorCode::Invalid);
    assert!(
        refused
            .message
            .contains(knobas_search::saved::NEEDS_ATTENTION),
        "{refused:?}"
    );
}

/// Rename keeps the id and the query; delete takes the row off the rail; and
/// neither reaches a built-in.
///
/// The id not moving is the load-bearing part: it is what `list:<id>` names
/// and what the change badge's seen-stamp is keyed on, so a rename that
/// re-derived it would silently un-read the list and break a `list:` the
/// reader had learned to type.
#[tokio::test]
async fn a_saved_list_can_be_renamed_and_deleted_and_a_built_in_can_be_neither() {
    let pool = saved_pool("saved-crud").await;
    let t = token("crud");
    mirror_row(
        &pool,
        "jira:r",
        "ticket",
        &format!("{t} row"),
        "mara",
        "-4 minutes",
    )
    .await;
    let made = create_smart_list_inner(&pool, "Payments retries", &t)
        .await
        .expect("saved");
    assert_eq!(made.id, "payments-retries");

    // Opened once, so the seen-stamp exists and the rename can be shown not to
    // have lost it.
    smart_list_items_inner(&pool, &made.id, 20)
        .await
        .expect("opens");

    rename_smart_list_inner(&pool, &made.id, "  SEPA  ")
        .await
        .expect("renamed");
    let renamed = rail_row(&pool, &made.id).await;
    assert_eq!(renamed.label, "SEPA", "the name is trimmed");
    assert_eq!(renamed.id, made.id, "the id does not move");
    assert_eq!(renamed.description, t, "the query is untouched");
    assert!(!renamed.changed, "and the badge is still cleared");
    assert_eq!(renamed.count, 1);

    // A blank name is refused rather than stored.
    assert_eq!(
        rename_smart_list_inner(&pool, &made.id, "   ")
            .await
            .expect_err("a list needs a name")
            .code,
        IpcErrorCode::Invalid
    );
    assert_eq!(rail_row(&pool, &made.id).await.label, "SEPA");

    // A built-in is code and is not a saved list, so neither write reaches it.
    for id in ["mine", "not-monitored"] {
        assert_eq!(
            rename_smart_list_inner(&pool, id, "Mine, renamed")
                .await
                .expect_err("a built-in cannot be renamed")
                .code,
            IpcErrorCode::NotFound,
            "{id}"
        );
        assert_eq!(
            delete_smart_list_inner(&pool, id)
                .await
                .expect_err("a built-in cannot be deleted")
                .code,
            IpcErrorCode::NotFound,
            "{id}"
        );
        assert!(
            rail(&pool).await.iter().any(|list| list.id == id),
            "{id} is still on the rail"
        );
    }

    delete_smart_list_inner(&pool, &made.id)
        .await
        .expect("deleted");
    assert!(
        !rail(&pool).await.iter().any(|list| list.saved),
        "nothing saved is left"
    );
    assert_eq!(
        smart_list_items_inner(&pool, &made.id, 20)
            .await
            .expect_err("it is gone")
            .code,
        IpcErrorCode::NotFound
    );
    assert_eq!(
        delete_smart_list_inner(&pool, &made.id)
            .await
            .expect_err("and stays gone")
            .code,
        IpcErrorCode::NotFound
    );
}

/// Creating refuses what it cannot run, and never shadows a built-in.
///
/// `list:<id>` has one meaning, so the two id spaces are one: a saved list
/// called *My items* slugs to `mine`, which is the built-in that answers
/// `list:mine`, and the generator has to walk past it. Asserted by **opening**
/// the built-in id afterwards and getting the built-in, not by reading the
/// generated string alone -- the string is a representation of the collision
/// and the answer is the collision itself.
#[tokio::test]
async fn creating_refuses_a_query_it_cannot_run_and_never_shadows_a_built_in() {
    let pool = saved_pool("saved-refuse").await;
    let t = token("refuse");
    mirror_row(
        &pool,
        "jira:s",
        "ticket",
        &format!("{t} row"),
        "mara",
        "-4 minutes",
    )
    .await;

    for (why, label, query) in [
        ("a prefix that is not a search", "Palette", "> palette"),
        ("a list of a list", "Alias", "list:mine"),
        ("the help card", "Help", "?"),
        ("nothing to search for", "Nothing", "   "),
        (
            "a paste rather than a query",
            "Paste",
            &"x".repeat(600) as &str,
        ),
    ] {
        let refused = create_smart_list_inner(&pool, label, query)
            .await
            .err()
            .unwrap_or_else(|| panic!("{why} was saved"));
        assert_eq!(refused.code, IpcErrorCode::Invalid, "{why}");
        assert!(
            refused
                .message
                .contains(knobas_search::saved::NEEDS_ATTENTION),
            "{why}: the reader is told which rule refused it: {refused:?}"
        );
    }
    // A name is owed too, and its refusal is a different sentence: the query
    // was fine.
    let unnamed = create_smart_list_inner(&pool, "   ", &t)
        .await
        .expect_err("a saved list needs a name");
    assert_eq!(unnamed.code, IpcErrorCode::Invalid);
    assert!(
        !unnamed
            .message
            .contains(knobas_search::saved::NEEDS_ATTENTION),
        "a blank name is not a broken query: {unnamed:?}"
    );

    // Nothing above left a row behind.
    assert!(!rail(&pool).await.iter().any(|list| list.saved));

    // And the collision: *My items* slugs onto the built-in that answers
    // `list:mine`.
    let made = create_smart_list_inner(&pool, "My items", &t)
        .await
        .expect("saved");
    assert_ne!(made.id, "mine", "a saved list may not shadow a built-in");
    assert_eq!(made.id, "my-items");
    let clash = create_smart_list_inner(&pool, "mine", &t)
        .await
        .expect("saved");
    assert_eq!(clash.id, "mine-2");
    // The proof is what `list:mine` opens, not what the generator returned.
    let built_in = smart_list_items_inner(&pool, "mine", 20)
        .await
        .expect("the built-in still answers");
    assert!(
        built_in.coverage.is_empty(),
        "a built-in list reports no coverage; a saved one would: {built_in:?}"
    );
    assert_eq!(
        rail_row(&pool, "mine").await.label,
        "My items",
        "and it is still the built-in on the rail"
    );
    assert!(!rail_row(&pool, "mine").await.saved);
}
