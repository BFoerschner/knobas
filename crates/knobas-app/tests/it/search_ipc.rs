//! The four §2.4 search commands, exercised through their bodies.
//!
//! A `#[tauri::command]` cannot be called without a window, so what is tested
//! here is the `*_inner` function each command is a two-line shim over -- the
//! pattern `tests/it/demo.rs` already uses for `demo_load_inner`. `tests/it/ipc.rs`
//! covers the other half (that the argument shapes decode and the handler list
//! dispatches); this file covers what the shims are *for*: the wire shape the
//! hand-written TypeScript mirrors, the error codes the frontend branches on,
//! and the one DTO that is a composition of two streams' data.
//!
//! The tests in this file share one database (`knobas_db::test_util`), and
//! libtest does not order them. So: nothing truncates, every corpus assertion
//! is scoped to a token this test seeded, and a test that reads a count of
//! anything global takes a database of its own (#1907).

use knobas_app::IpcErrorCode;
use knobas_app::commands::search::{
    create_smart_list_inner, delete_smart_list_inner, filter_suggestions_inner,
    launcher_home_inner, rename_smart_list_inner, search_inner, smart_lists_inner,
};
use knobas_app::sources::Registry;
use knobas_search::{SearchFilters, SearchQuery};

/// A pool with the schema on it.
async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A token nothing else in this file can match.
fn token(tag: &str) -> String {
    format!("zi{tag}{}", uuid::Uuid::new_v4().simple())
}

/// One ticket and one PR carrying `tag`, plus a source to attribute them to.
///
/// The source row is what `launcher_home` reads its health out of, so it has
/// to exist before that assertion -- and it is inserted with `on conflict do
/// nothing` because another test in this file may have got there first.
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
        titles: false,
        raw: raw.to_owned(),
        limit: 20,
        filters: SearchFilters::default(),
    }
}

/// The field names are the contract.
///
/// The shell's types are generated from these structs
/// (`app/src/lib/ipc/generated/search.ts`, #1329), so a rename on this side
/// reaches the frontend's types; this pins the shape of a **live answer**,
/// which is the only
/// place `interpreted`, `groups` and `took_ms` appear together and the only
/// place the flatten is observed on data that came out of PostgreSQL.
#[tokio::test]
async fn the_wire_shape_is_the_contract() {
    let pool = pool().await;
    let t = seed(&pool, "wire").await;

    let response = search_inner(&pool, &Registry::for_process(), query(&t))
        .await
        .unwrap();
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

    let response = search_inner(&pool, &Registry::for_process(), query(&t))
        .await
        .unwrap();
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
/// `not_found` is what makes a command naming a missing list say *no such
/// list* instead of *something went wrong*; the message is for humans and
/// nobody parses it.
#[tokio::test]
async fn errors_carry_a_code_the_frontend_can_branch_on() {
    let pool = pool().await;

    // `list:` is retired from the box (#2423): it is the words it is, and
    // no prefix.
    let typed = search_inner(&pool, &Registry::for_process(), query("list:nope"))
        .await
        .unwrap();
    assert_eq!(typed.interpreted.prefix, None, "{typed:?}");
    assert_eq!(typed.interpreted.text, "list:nope", "{typed:?}");
    assert!(typed.interpreted.unknown_tokens.is_empty(), "{typed:?}");

    // The commands that name a list say `not_found`.
    let unknown = delete_smart_list_inner(&pool, &Registry::for_process(), "nope")
        .await
        .unwrap_err();
    assert_eq!(unknown.code, IpcErrorCode::NotFound, "{unknown:?}");
    assert!(unknown.message.contains("nope"), "{unknown:?}");

    // A query outside the engine's bounds is the caller's fault, not the
    // database's: `invalid`, so the box can say so rather than offering
    // *Retry*.
    let huge = search_inner(&pool, &Registry::for_process(), query(&"p".repeat(1_000)))
        .await
        .unwrap_err();
    assert_eq!(huge.code, IpcErrorCode::Invalid, "{huge:?}");

    // And a database failure is `internal`. Taken against a closed pool so
    // the mapping is observed rather than argued: `SearchError::Db` is the one
    // arm no bad argument can reach.
    let closed = knobas_db::test_util::test_pool().await;
    closed.close().await;
    let dead = search_inner(&closed, &Registry::for_process(), query("sepa"))
        .await
        .unwrap_err();
    assert_eq!(dead.code, IpcErrorCode::Internal, "{dead:?}");
}

/// The board reports source health beside the recent rows (interfaces §2.4).
///
/// `LauncherHome` is the one DTO in this stream that composes two streams'
/// data: the recent rows are `knobas-search`'s, `sources` is
/// stream F's `CredentialHealth`, and `pending_writes` is stream G's write
/// queue. What is asserted here is that all three halves arrive and are shaped
/// as the mirror declares them; the *value* of the count belongs to
/// `launcher_home_counts_the_pending_writes`, which is the only test in this
/// file that queues anything and therefore the only one that may say a
/// number out loud about a table the whole file shares.
#[tokio::test]
async fn launcher_home_reports_source_health_beside_the_recent_rows() {
    let pool = pool().await;
    seed(&pool, "home").await;

    let home = launcher_home_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let v = serde_json::to_value(&home).unwrap();

    for field in ["recent", "sources", "pending_writes"] {
        assert!(!v[field].is_null(), "{field} missing: {v}");
    }
    // No lists on the board since #2423: a saved list is found behind `#l`.
    assert!(v.get("smart_lists").is_none(), "{v}");
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

    // The board's recent rows carry the provenance stamp.
    assert!(!home.recent.is_empty(), "this test seeded two items");
    assert!(v["recent"][0]["synced_at"].is_string(), "{v}");
}

/// The footer's number is the write queue's *pending* rows (issue #212).
///
/// `write_queue::counts` is the whole table's, so this test takes a database
/// of its own: on the file's shared one, another test's queued write moves
/// the count between the reads (#1907). Both directions are asserted, because
/// the direction is the decision: queueing a write raises the count, and *holding*
/// that same write lowers it again -- "N pending writes" says be patient, and a
/// held write is asking for a decision instead (`QueueCounts`' own rule that
/// "3 waiting" may never absorb a held write).
#[tokio::test]
async fn launcher_home_counts_the_pending_writes() {
    let pool = knobas_db::test_util::scratch_database("search_ipc_pending_writes")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let entity = knobas_core::entity::EntityRef::new("jira", &token("wq"));

    let before = launcher_home_inner(&pool, &Registry::for_process())
        .await
        .unwrap()
        .pending_writes;

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
        launcher_home_inner(&pool, &Registry::for_process())
            .await
            .unwrap()
            .pending_writes,
        before + 1,
        "a queued write is one the launcher still owes a source"
    );

    knobas_core::write_queue::hold(&pool, queued.id, serde_json::json!({}))
        .await
        .unwrap()
        .expect("the write was pending, so it can be held");
    assert_eq!(
        launcher_home_inner(&pool, &Registry::for_process())
            .await
            .unwrap()
            .pending_writes,
        before,
        "a held write waits for the user, not for the network"
    );
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
    let response = search_inner(&pool, &Registry::for_process(), narrowed)
        .await
        .unwrap();

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
    let found = search_inner(&pool, &Registry::for_process(), ancestors)
        .await
        .unwrap();
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
        titles: false,
        raw: String::new(),
        limit: 50,
        filters: SearchFilters {
            kinds: vec!["asset".to_owned()],
            ..SearchFilters::default()
        },
    };
    let answer = search_inner(&pool, &Registry::for_process(), browse)
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
    let response = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("asset: {t}")),
    )
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
    let filtered = search_inner(&pool, &Registry::for_process(), by_filter)
        .await
        .unwrap();
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
    let by_prefix = search_inner(
        &pool,
        &Registry::for_process(),
        with_author(&format!("asset: {t}"), Vec::new()),
    )
    .await
    .unwrap();
    let by_kind = search_inner(
        &pool,
        &Registry::for_process(),
        with_author(&t, vec!["asset".to_owned()]),
    )
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

    let found = search_inner(&pool, &Registry::for_process(), query(&host))
        .await
        .unwrap();
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
    let by_key = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("hostname {t}")),
    )
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

    // The route, at the shape every route in `testenv/estate.json`
    // actually has -- a loopback host, no path, and the port that is the only
    // thing telling six of them apart (ADR-0013).
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

    let by_port = search_inner(&pool, &Registry::for_process(), query("53002"))
        .await
        .unwrap();
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

/// **Moving an asset moves what `under:` returns, with nothing stale** (#600).
///
/// Through the store's own `create`, `create_route` and `move_to`, because a
/// move is the store's write: `under:` reads the `parent_id` it writes, and the
/// path line reads the `path_text` it rewrites for the whole subtree. A VM with
/// a container and a route moves from one site to another. The next search
/// finds all three under the new site and none of them under the old one, and
/// each row's path line names the new site.
#[tokio::test]
async fn moving_an_asset_moves_what_under_returns() {
    use knobas_app::assets::Visibility;

    let pool = pool().await;
    let t = token("move");
    let ids = |answer: &knobas_search::SearchResponse| {
        let mut out: Vec<String> = answer
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .map(|hit| hit.row.entity_id.clone())
            .collect();
        out.sort();
        out
    };
    let create = |parent: Option<String>, type_id: &'static str, name: String| {
        let pool = pool.clone();
        async move {
            knobas_app::assets::create(&pool, parent.as_deref(), type_id, &name, &[])
                .await
                .unwrap()
                .value
        }
    };

    let old = create(None, "site", format!("old-{t}")).await;
    let new = create(None, "site", format!("new-{t}")).await;
    let vm = create(Some(old.id.clone()), "vm", format!("vm-{t}")).await;
    let pg = create(Some(vm.id.clone()), "container", format!("pg-{t}")).await;
    let route = knobas_app::assets::create_route(
        &pool,
        &pg.id,
        &format!("postgres-{t}"),
        "http://127.0.0.1:5432/",
        None,
        Visibility::Internal,
        &[],
    )
    .await
    .unwrap()
    .value;
    let mut moved = vec![vm.id.clone(), pg.id.clone(), route.id.clone()];
    moved.sort();

    let before = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("under:old-{t}")),
    )
    .await
    .unwrap();
    let mut expected = [moved.clone(), vec![old.id.clone()]].concat();
    expected.sort();
    assert_eq!(ids(&before), expected);

    knobas_app::assets::move_to(&pool, &vm.id, Some(&new.id))
        .await
        .unwrap();

    let after_old = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("under:old-{t}")),
    )
    .await
    .unwrap();
    assert_eq!(
        ids(&after_old),
        std::slice::from_ref(&old.id),
        "nothing stale under the old site"
    );

    let after_new = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("under:new-{t}")),
    )
    .await
    .unwrap();
    let mut expected = [moved, vec![new.id.clone()]].concat();
    expected.sort();
    assert_eq!(ids(&after_new), expected);
    let path = |id: &str| {
        after_new
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .find(|hit| hit.row.entity_id == id)
            .and_then(|hit| hit.row.path.clone())
    };
    assert_eq!(path(&vm.id), Some(format!("new-{t}")));
    assert_eq!(path(&pg.id), Some(format!("new-{t} / vm-{t}")));
    assert_eq!(path(&route.id), Some(format!("new-{t} / vm-{t} / pg-{t}")));
}

// ---------------------------------------------------------------------------
// Saved smart lists (#506, spec #491 stories 56-58 and 60)
// ---------------------------------------------------------------------------

/// A database of this test's own.
///
/// Unlike everything above, a saved list is **global state on the board**: the
/// row this makes is on every other test's rail, and the counts are read over
/// a corpus this file's other tests are also writing to. So these tests take
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

/// Apply a saved list the way the launcher does (#2423): its own text in the
/// box, followed by the space the box leaves after it, searched.
async fn apply(
    pool: &sqlx::PgPool,
    list: &knobas_search::SmartListSummary,
    limit: u32,
) -> Result<knobas_search::SearchResponse, knobas_app::IpcError> {
    let applied = SearchQuery {
        titles: false,
        raw: format!("{} ", list.description),
        limit,
        filters: SearchFilters::default(),
    };
    search_inner(pool, &Registry::for_process(), applied).await
}

/// Every saved list.
async fn rail(pool: &sqlx::PgPool) -> Vec<knobas_search::SmartListSummary> {
    smart_lists_inner(pool, &Registry::for_process())
        .await
        .expect("the rail")
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
/// The corpus is built so that every clause of `#t <token> @mara` has something
/// to exclude: a ticket by somebody else (the author chip), a PR by Mara (the
/// `#t` filter's kind), and a ticket by Mara that does not carry the token (the
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

    let raw = format!("#t {t} @mara");
    let made = create_smart_list_inner(&pool, &Registry::for_process(), "Mara's open ones", &raw)
        .await
        .expect("the query is one the grammar runs");

    // The row as the rail draws it, on the wire the panel reads.
    let wire = serde_json::to_value(&made).unwrap();
    assert_eq!(wire["id"], "mara-s-open-ones", "{wire}");
    assert_eq!(wire["label"], "Mara's open ones", "{wire}");
    assert_eq!(wire["needs_attention"], false, "{wire}");
    // A saved list's blurb is the query it stands for.
    assert_eq!(wire["description"], raw, "{wire}");
    assert_eq!(
        wire["count"], 2,
        "the two tickets Mara wrote carrying {t}: {wire}"
    );

    // And in `smart_lists`, alone: there are no built-ins beside it (#2423).
    let listed = rail(&pool).await;
    assert_eq!(
        listed
            .iter()
            .map(|list| list.id.as_str())
            .collect::<Vec<_>>(),
        [made.id.as_str()]
    );

    // Applying it: the rows the count promised, and the saved query's own
    // interpretation -- which is what draws the chip row above the results.
    let opened = apply(&pool, &made, 20)
        .await
        .expect("the saved list applies");
    let v = serde_json::to_value(&opened).unwrap();
    // `#t` is a kind filter and claims no prefix (#2420); `@mara` claims one.
    assert_eq!(v["interpreted"]["prefix"], "person", "{v}");
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
/// The worst failure available -- a number nobody can check against the rows
/// -- and for a saved list the two come from *two generated statements*
/// (`sql::saved_summary_sql` and `sql::search_sql`). Several query shapes, because the shapes
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
        ("a kind filter", format!("#t {t}")),
        ("a window", format!("{t} updated:7d")),
        ("all of them", format!("#t {t} @mara updated:7d")),
        ("a query that matches nothing", format!("{t} @nobody")),
    ] {
        let made = create_smart_list_inner(&pool, &Registry::for_process(), name, &raw)
            .await
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let opened = apply(&pool, &made, 200)
            .await
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // The items it holds: applied, the box also finds the saved lists
        // whose text holds the words (#2423), which a list does not count.
        let lists = opened
            .groups
            .iter()
            .filter(|group| group.kind == "list")
            .map(|group| group.total)
            .sum::<u32>();
        assert_eq!(
            u64::try_from(made.count).expect("a count is not negative"),
            u64::from(opened.total - lists),
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
    let healthy = create_smart_list_inner(&pool, &Registry::for_process(), "Still fine", &t)
        .await
        .expect("saved");

    // Five rows an older grammar could have written and this one cannot run:
    // a prefix that is not a search, a list of a list, a source named the old
    // way, a paste, and a query whose every token the parser no longer keeps.
    for (id, label, query) in [
        ("legacy-palette", "Palette", "> palette".to_owned()),
        ("legacy-alias", "Alias", "list:mine".to_owned()),
        ("legacy-slash", "Slash", format!("/jira {t}")),
        ("legacy-paste", "Paste", "x".repeat(600)),
        // A source the reader has since deleted: the alias resolves to
        // nothing, so the saved query has no text and no filter left in it --
        // and `0025`'s own check constraint is why the row is `source:gone`
        // and not a blank string, which the table refuses outright. It was
        // `/gone` until #2420 made `/` a word.
        ("legacy-gone", "Gone", "source:gone".to_owned()),
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
    let broken = [
        "legacy-palette",
        "legacy-alias",
        "legacy-slash",
        "legacy-paste",
        "legacy-gone",
    ];
    for id in broken {
        let row = listed
            .iter()
            .find(|list| list.id == id)
            .unwrap_or_else(|| panic!("{id} is not on the rail at all"));
        assert!(row.needs_attention, "{id}: {row:?}");
        assert_eq!(row.count, 0, "{id} was never asked: {row:?}");
        assert!(
            row.description
                .starts_with(knobas_search::saved::NEEDS_ATTENTION),
            "{id}: {}",
            row.description
        );
    }
    // Each says *which* rule refused it, or the reader is told something is
    // wrong and never what -- asserted as *five different sentences* rather
    // than as a length, because "long enough" is a stand-in for the property
    // and this is the property.
    let mut reasons: Vec<&str> = listed
        .iter()
        .filter(|list| broken.contains(&list.id.as_str()))
        .map(|list| list.description.as_str())
        .collect();
    assert_eq!(reasons.len(), broken.len());
    reasons.sort_unstable();
    reasons.dedup();
    assert_eq!(
        reasons.len(),
        broken.len(),
        "two of the five rows are refused with the same sentence: {reasons:?}"
    );

    // The rest of the rail is untouched -- which is the half of story 60 that
    // says "rather than an error".
    let still = listed
        .iter()
        .find(|list| list.id == healthy.id)
        .expect("the healthy saved list");
    assert!(!still.needs_attention);
    assert_eq!(still.count, 1);

    // And `#l` draws all six, each broken one carrying its own sentence as
    // its state: the launcher refuses to apply it with that reason (#2423).
    let found = search_inner(&pool, &Registry::for_process(), query("#l"))
        .await
        .expect("#l answers");
    let group = found
        .groups
        .iter()
        .find(|g| g.kind == "list")
        .expect("lists");
    assert_eq!(group.hits.len(), broken.len() + 1, "{found:?}");
    for hit in &group.hits {
        let summary = listed
            .iter()
            .find(|list| list.id == hit.row.entity_id)
            .unwrap();
        match &hit.row.state {
            Some(state) => assert_eq!(state.text, summary.description, "{hit:?}"),
            None => assert_eq!(hit.row.entity_id, healthy.id, "{hit:?}"),
        }
    }
}

/// Rename keeps the id and the query; delete takes the row out of `#l`.
///
/// The id not moving is the load-bearing part: it is the `list` hit's entity
/// id, which a favourite or a key bound to the list holds (spec #2419).
#[tokio::test]
async fn a_saved_list_can_be_renamed_and_deleted() {
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
    let made = create_smart_list_inner(&pool, &Registry::for_process(), "Payments retries", &t)
        .await
        .expect("saved");
    assert_eq!(made.id, "payments-retries");

    rename_smart_list_inner(&pool, &Registry::for_process(), &made.id, "  SEPA  ")
        .await
        .expect("renamed");
    let renamed = rail_row(&pool, &made.id).await;
    assert_eq!(renamed.label, "SEPA", "the name is trimmed");
    assert_eq!(renamed.id, made.id, "the id does not move");
    assert_eq!(renamed.description, t, "the query is untouched");
    assert_eq!(renamed.count, 1);

    // A blank name is refused rather than stored.
    assert_eq!(
        rename_smart_list_inner(&pool, &Registry::for_process(), &made.id, "   ")
            .await
            .expect_err("a list needs a name")
            .code,
        IpcErrorCode::Invalid
    );
    assert_eq!(rail_row(&pool, &made.id).await.label, "SEPA");

    delete_smart_list_inner(&pool, &Registry::for_process(), &made.id)
        .await
        .expect("deleted");
    assert!(rail(&pool).await.is_empty(), "nothing saved is left");
    let gone = search_inner(&pool, &Registry::for_process(), query("#l"))
        .await
        .expect("#l answers");
    assert!(gone.groups.is_empty(), "{gone:?}");
    assert_eq!(
        delete_smart_list_inner(&pool, &Registry::for_process(), &made.id)
            .await
            .expect_err("and stays gone")
            .code,
        IpcErrorCode::NotFound
    );
}

/// Creating refuses what it cannot run, and two lists of one name get two ids.
#[tokio::test]
async fn creating_refuses_a_query_it_cannot_run() {
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

    let mut refusals: Vec<String> = Vec::new();
    for (why, label, query) in [
        ("a prefix that is not a search", "Palette", "> palette"),
        ("a list of a list", "Alias", "list:mine"),
        ("a source named the old way", "Slash", "/jira sepa"),
        ("the help card", "Help", "?"),
        ("nothing to search for", "Nothing", "   "),
        (
            "a paste rather than a query",
            "Paste",
            &"x".repeat(600) as &str,
        ),
    ] {
        let refused = create_smart_list_inner(&pool, &Registry::for_process(), label, query)
            .await
            .err()
            .unwrap_or_else(|| panic!("{why} was saved"));
        assert_eq!(refused.code, IpcErrorCode::Invalid, "{why}");
        // The rule, and not the rail's *needs attention* line: at creation
        // time there is no stored row to delete (#1413).
        assert!(
            !refused
                .message
                .contains(knobas_search::saved::NEEDS_ATTENTION)
                && !refused.message.contains("Delete it"),
            "{why}: the reader is told the rule, not to delete a list: {refused:?}"
        );
        refusals.push(refused.message);
    }
    // Each names *which* rule, or the reader is told something is wrong and
    // never what.
    let distinct: std::collections::HashSet<&String> = refusals.iter().collect();
    assert_eq!(distinct.len(), refusals.len(), "{refusals:?}");
    // A name is owed too, and its refusal is a different sentence: the query
    // was fine.
    let unnamed = create_smart_list_inner(&pool, &Registry::for_process(), "   ", &t)
        .await
        .expect_err("a saved list needs a name");
    assert_eq!(unnamed.code, IpcErrorCode::Invalid);
    assert!(
        !refusals.contains(&unnamed.message),
        "a blank name is not a broken query: {unnamed:?}"
    );

    // Nothing above left a row behind.
    assert!(rail(&pool).await.is_empty());

    // No built-in holds `mine` any more (#2423), so it is a saved list's to
    // take, and a second of the same name walks past it.
    let made = create_smart_list_inner(&pool, &Registry::for_process(), "mine", &t)
        .await
        .expect("saved");
    assert_eq!(made.id, "mine");
    let clash = create_smart_list_inner(&pool, &Registry::for_process(), "Mine", &t)
        .await
        .expect("saved");
    assert_eq!(clash.id, "mine-2");
    assert_eq!(rail_row(&pool, "mine-2").await.label, "Mine");
}

/// The wire between the registry and the search (#595): `search_inner` hands
/// the compiled-in adapters' declarations to the searcher, so `under:` reads
/// the parent the **real** Gitea adapter declares. Without that wiring every
/// `under:` here would match nothing and be reported as unknown -- the
/// search crate's own tests, which pass the template by hand, could not see
/// it go.
#[tokio::test]
async fn under_reads_the_parent_the_compiled_in_gitea_adapter_declares() {
    let pool = pool().await;
    let t = token("under");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('gitea', 'gitea', 'Gitea', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(&pool)
    .await
    .unwrap();
    let id = format!("gitea:tidewater/{t}@c90d11");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'commit','jitter')")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, 'gitea', 'commit', 'jitter', 'jitter',
                 jsonb_build_object('sha', 'c90d11', 'repository',
                   jsonb_build_object('full_name', 'tidewater/' || $2, 'name', $2::text)))",
    )
    .bind(&id)
    .bind(&t)
    .execute(&pool)
    .await
    .unwrap();

    let response = search_inner(
        &pool,
        &Registry::for_process(),
        query(&format!("under:{t}")),
    )
    .await
    .unwrap();
    let hits: Vec<&str> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.as_str()))
        .collect();
    assert_eq!(hits, [id.as_str()]);
    assert!(response.interpreted.unknown_tokens.is_empty());
}

/// The query box's suggestions cross the bridge in the shape
/// `app/src/lib/ipc/generated/search.ts` declares (#601), and through the searcher the
/// app builds: the compiled-in adapters' declarations, so a Gitea commit's
/// repo is offered for `under:` with its kind.
#[tokio::test]
async fn the_suggestions_carry_kinds_sources_and_parents_with_their_kind() {
    let pool = pool().await;
    let t = token("suggest");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('gitea', 'gitea', 'Gitea', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(&pool)
    .await
    .unwrap();
    let id = format!("gitea:o/{t}@abc");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', 'fix')")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, 'gitea', 'commit', 'fix', 'fix', $2)",
    )
    .bind(&id)
    .bind(serde_json::json!({ "sha": "abc", "repository": { "full_name": format!("o/{t}"), "name": t } }))
    .execute(&pool)
    .await
    .unwrap();

    let v = serde_json::to_value(
        filter_suggestions_inner(&pool, &Registry::for_process(), "", false)
            .await
            .unwrap(),
    )
    .unwrap();

    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["fields", "kinds", "parents", "sources"], "{v}");
    let kind = v["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "commit")
        .unwrap_or_else(|| panic!("a commit has a row: {v}"));
    assert!(kind["label"].is_string(), "{v}");
    assert!(
        v["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "gitea" && s["display_name"] == "Gitea"),
        "{v}"
    );
    // The parents by prefix, as the box asks for them since #761: one ask
    // per name this test seeded, each answered in the same wire shape.
    let mut ours: Vec<(String, String)> = Vec::new();
    for prefix in [format!("o/{t}"), t.clone()] {
        let v = serde_json::to_value(
            filter_suggestions_inner(&pool, &Registry::for_process(), &prefix, false)
                .await
                .unwrap(),
        )
        .unwrap();
        ours.extend(
            v["parents"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["name"].as_str().unwrap().contains(&t))
                .map(|p| {
                    (
                        p["name"].as_str().unwrap().to_owned(),
                        p["kind"].as_str().unwrap().to_owned(),
                    )
                }),
        );
    }
    ours.sort();
    ours.dedup();
    assert_eq!(
        ours,
        [
            (format!("o/{t}"), "repo".to_owned()),
            (t.clone(), "repo".to_owned())
        ]
    );
}

/// *Titles* (spec #2419 story 51, #2430): with the query's `titles` flag on,
/// the engine answers only rows whose title or key holds every word, so a
/// ticket is found by its key and by its summary and not by a word only its
/// description holds. With the flag off it answers as it always has, bodies
/// included.
#[tokio::test]
async fn titles_matches_the_title_and_the_key_and_never_the_body() {
    let pool = pool().await;
    let key = format!("{}-210", token("key"));
    let named = token("named");
    let written = token("written");
    let id = format!("jira:{key}");
    mirror_row(
        &pool,
        &id,
        "ticket",
        &format!("Payouts {named} retry"),
        "mara",
        "-4 minutes",
    )
    .await;
    sqlx::query("update sync.item set body_text = $2 where entity_id = $1")
        .bind(&id)
        .bind(format!("The batch {written} gives up after one retry."))
        .execute(&pool)
        .await
        .unwrap();

    async fn ids(pool: &sqlx::PgPool, raw: &str, titles: bool) -> Vec<String> {
        let asked = SearchQuery {
            titles,
            ..query(raw)
        };
        search_inner(pool, &Registry::for_process(), asked)
            .await
            .unwrap()
            .groups
            .iter()
            .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
            .collect()
    }
    let ours = vec![id.clone()];

    // The key, the title, and the two together: on screen, found either way.
    for raw in [key.clone(), named.clone(), format!("{key} {named}")] {
        assert_eq!(ids(&pool, &raw, true).await, ours, "titles: {raw}");
        assert_eq!(ids(&pool, &raw, false).await, ours, "everything: {raw}");
    }
    // A word only the body holds: Everything finds it, Titles does not, and
    // neither does a title word beside it.
    assert_eq!(ids(&pool, &written, false).await, ours);
    assert!(ids(&pool, &written, true).await.is_empty());
    assert!(
        ids(&pool, &format!("{named} {written}"), true)
            .await
            .is_empty()
    );
    // The last word half-typed is still a prefix under Titles.
    assert_eq!(ids(&pool, &named[..named.len() - 4], true).await, ours);
}

/// The flag is optional on the wire, as `authors` is (§10.8): a caller
/// written before it sends none and is answered as Everything.
#[test]
fn a_query_without_the_titles_flag_reads_as_everything() {
    let decoded: SearchQuery = serde_json::from_value(serde_json::json!({
        "raw": "pay-210",
        "limit": 20,
        "filters": { "sources": [], "kinds": [], "updated_within_days": null, "mine": false },
    }))
    .unwrap();
    assert!(!decoded.titles);
}

/// The hits of one kind in an answer, as `(entity_id, title, path)`.
fn hits_of(
    response: &knobas_search::SearchResponse,
    kind: &str,
) -> Vec<(String, String, Option<String>)> {
    response
        .groups
        .iter()
        .filter(|group| group.kind == kind)
        .flat_map(|group| group.hits.iter())
        .map(|hit| {
            (
                hit.row.entity_id.clone(),
                hit.row.title.clone(),
                hit.row.path.clone(),
            )
        })
        .collect()
}

/// Saved lists are a kind in the index (#2423, spec #2419 story 13): `#l`
/// answers every one, `#l <word>` the ones whose name or text holds it, and
/// a list's path line is its own text, which Enter puts back into the box.
#[tokio::test]
async fn hash_l_answers_the_saved_lists_with_their_text_as_the_path_line() {
    let pool = saved_pool("list-kind").await;
    let t = token("lk");
    let gitea = format!("#p {t} gitea");
    let other = format!("#t {t} payouts");
    let made = create_smart_list_inner(&pool, &Registry::for_process(), "Reviews", &gitea)
        .await
        .expect("a query the grammar runs");
    create_smart_list_inner(&pool, &Registry::for_process(), "Payout tickets", &other)
        .await
        .expect("a query the grammar runs");

    let every = search_inner(&pool, &Registry::for_process(), query("#l"))
        .await
        .expect("#l answers");
    let mut lists = hits_of(&every, "list");
    lists.sort();
    assert_eq!(
        lists,
        [
            (
                "payout-tickets".to_owned(),
                "Payout tickets".to_owned(),
                Some(other.clone())
            ),
            (made.id.clone(), "Reviews".to_owned(), Some(gitea.clone())),
        ],
        "{every:?}"
    );
    assert!(
        every.groups.iter().all(|group| group.kind == "list"),
        "#l narrows to lists: {every:?}"
    );
    let group = every.groups.iter().find(|g| g.kind == "list").unwrap();
    assert_eq!(group.monogram, "LI");

    // By the text, and by the name.
    let by_text = search_inner(&pool, &Registry::for_process(), query("#l gitea"))
        .await
        .expect("#l gitea answers");
    assert_eq!(
        hits_of(&by_text, "list"),
        [(made.id.clone(), "Reviews".to_owned(), Some(gitea.clone()))]
    );
    let by_name = search_inner(&pool, &Registry::for_process(), query("#l payout"))
        .await
        .expect("#l payout answers");
    assert_eq!(
        hits_of(&by_name, "list")
            .into_iter()
            .map(|hit| hit.0)
            .collect::<Vec<_>>(),
        ["payout-tickets"]
    );
}

/// A saved list whose text today's grammar refuses says so on its row (#2423):
/// its state is the reason, in the warning tone, and a healthy list has none.
#[tokio::test]
async fn a_list_that_needs_attention_carries_the_reason_as_its_state() {
    let pool = saved_pool("list-attention").await;
    let t = token("la");
    create_smart_list_inner(
        &pool,
        &Registry::for_process(),
        "Healthy",
        &format!("#t {t}"),
    )
    .await
    .expect("a query the grammar runs");
    // Written as an older knobas could have: `>` is the palette, not a search.
    sqlx::query("insert into knobas.smart_list (id, label, query) values ('broken', 'Broken', '> settings')")
        .execute(&pool)
        .await
        .unwrap();

    let every = search_inner(&pool, &Registry::for_process(), query("#l"))
        .await
        .expect("#l answers");
    let group = every
        .groups
        .iter()
        .find(|g| g.kind == "list")
        .expect("a list group");
    let state = |id: &str| {
        group
            .hits
            .iter()
            .find(|hit| hit.row.entity_id == id)
            .unwrap_or_else(|| panic!("no {id} in {every:?}"))
            .row
            .state
            .clone()
    };
    assert_eq!(state("healthy"), None);
    let broken = state("broken").expect("the broken list says so");
    assert_eq!(broken.tone, knobas_core::tone::Tone::Warn);
    assert!(broken.text.starts_with("Needs attention"), "{broken:?}");
}
