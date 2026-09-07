//! The link a reader pasted, at the seam the command is a shim over
//! (issue #496, spec #491 stories 9-11 and 15-17).
//!
//! Two halves, the split `assets_ipc.rs` and `time_ipc.rs` make: the
//! *behaviour*, driven through `resolve_url_inner`, which is what the
//! `#[tauri::command]` calls once it has a pool; and the *wiring* -- the
//! command registered under the name the mirror invokes, its one argument
//! decoding -- through the `tauri::test` mock runtime.
//!
//! Every behavioural test gets a **database of its own**
//! (`knobas_db::test_util::scratch_database`) rather than the binary's shared
//! one. Resolution is a question about a *whole* mirror: "no row anywhere
//! carries this URL" is the miss, and two tests sharing a database would make
//! one of them the other's fixture -- a URL another test seeded is
//! indistinguishable from one this test's own source reported.
//!
//! # What is asserted here and what is asserted next door
//!
//! The *normalisation rule* -- which spellings of a URL are one link -- is
//! `crates/knobas-core/tests/web_url.rs`, against the SQL that is its only
//! implementation. What is here is what the **command** answers over a mirror:
//! the four sources' shapes, the miss, the tombstone, two instances of one
//! adapter, and the two properties of the index the rule rests on.

use knobas_app::commands::entity::{RESOLVE_URL, UrlMatch, resolve_url_inner};
use knobas_app::{IpcError, IpcErrorCode};
use sqlx::PgPool;
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// A migrated database of this test's own.
async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// Put one mirrored item in the corpus, the way a sync run would leave it:
/// an entity row and the item that fills it in.
///
/// `web_url` is the address the *adapter* reported, so it is stored exactly as
/// the adapter composed it -- no normalisation on the way in. That is the
/// whole point of normalising on read: the mirror holds what the source said.
async fn mirror(pool: &PgPool, entity_id: &str, kind: &str, source_id: &str, web_url: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(entity_id)
        .bind(kind)
        .bind("Retry failed SEPA payouts")
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload, web_url)
         values ($1, $2, $3, $4, '{}'::jsonb, $5)",
    )
    .bind(entity_id)
    .bind(source_id)
    .bind(kind)
    .bind("Retry failed SEPA payouts")
    .bind(web_url)
    .execute(pool)
    .await
    .expect("the mirror row");
}

/// A row the mirror holds with no address at all -- a Gitea branch, which its
/// adapter reports with `web_url: None` because a branch has no page of its
/// own (interfaces §8 P5).
async fn mirror_without_url(pool: &PgPool, entity_id: &str, kind: &str, source_id: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, 'Untitled')")
        .bind(entity_id)
        .bind(kind)
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, $3, 'Untitled', '{}'::jsonb)",
    )
    .bind(entity_id)
    .bind(source_id)
    .bind(kind)
    .execute(pool)
    .await
    .expect("the mirror row");
}

/// The corpus every behavioural test below reads: one item per source, each
/// carrying the address its own adapter composes.
///
/// The four URLs are the real shapes, not four spellings of one: Jira's
/// `/browse/<key>`, Confluence's `viewpage.action?pageId=` -- whose identity
/// *is* its query -- Gitea's `<owner>/<repo>/issues/<n>` and Uptime Kuma's
/// `/dashboard/<id>`. A fixture of four look-alike URLs could not witness the
/// query rule at all.
async fn corpus(label: &str) -> PgPool {
    let pool = pool(label).await;
    mirror(
        &pool,
        "jira:PAY-231",
        "ticket",
        "jira",
        "https://jira.example/browse/PAY-231",
    )
    .await;
    mirror(
        &pool,
        "confluence:98307",
        "page",
        "confluence",
        "https://confluence.example/pages/viewpage.action?pageId=98307",
    )
    .await;
    mirror(
        &pool,
        "gitea:acme/payments-svc#142",
        "pr",
        "gitea",
        "https://gitea.example/acme/payments-svc/pulls/142",
    )
    .await;
    mirror(
        &pool,
        "kuma:8",
        "monitor",
        "kuma",
        "https://kuma.example/dashboard/8",
    )
    .await;
    pool
}

async fn resolved(pool: &PgPool, url: &str) -> Option<UrlMatch> {
    resolve_url_inner(pool, url)
        .await
        .expect("a resolvable URL is not a refusal")
}

/// The pair a hit answers with, as `(entity_id, kind)`, so an assertion reads
/// as the address the frontend is about to build.
fn pair(answer: Option<UrlMatch>) -> Option<(String, String)> {
    answer.map(|found| (found.entity_id, found.kind))
}

fn code(error: &IpcError) -> IpcErrorCode {
    error.code
}

/// Story 9: a link from chat, from each of the four systems, names its entity.
#[tokio::test]
async fn a_url_from_each_of_the_four_sources_resolves_to_its_entity() {
    let pool = corpus("url_four_sources").await;

    for (url, entity_id, kind) in [
        (
            "https://jira.example/browse/PAY-231",
            "jira:PAY-231",
            "ticket",
        ),
        (
            "https://confluence.example/pages/viewpage.action?pageId=98307",
            "confluence:98307",
            "page",
        ),
        (
            "https://gitea.example/acme/payments-svc/pulls/142",
            "gitea:acme/payments-svc#142",
            "pr",
        ),
        ("https://kuma.example/dashboard/8", "kuma:8", "monitor"),
    ] {
        assert_eq!(
            pair(resolved(&pool, url).await),
            Some((entity_id.to_owned(), kind.to_owned())),
            "{url} names {entity_id}"
        );
    }
}

/// Story 11, at the command: the three spellings a pasted link picks up on its
/// way through a chat window still name the entity the adapter stored.
#[tokio::test]
async fn a_fragment_a_trailing_slash_and_a_shouted_host_still_resolve() {
    let pool = corpus("url_spellings").await;

    for url in [
        "https://jira.example/browse/PAY-231#comment-42",
        "https://jira.example/browse/PAY-231/",
        "https://JIRA.Example/browse/PAY-231",
        "HTTPS://jira.example/browse/PAY-231/#comment-42",
    ] {
        assert_eq!(
            pair(resolved(&pool, url).await),
            Some(("jira:PAY-231".to_owned(), "ticket".to_owned())),
            "{url} is the ticket the mirror holds"
        );
    }

    // The query is the other half of the same story, and it is kept: the page
    // resolves through a fragment and a host in caps, and only that page.
    assert_eq!(
        pair(
            resolved(
                &pool,
                "https://CONFLUENCE.example/pages/viewpage.action?pageId=98307#Payments",
            )
            .await
        ),
        Some(("confluence:98307".to_owned(), "page".to_owned())),
    );
}

/// Story 10's other side. Two ways a paste misses, and they are different
/// mistakes: the same page-viewer with a different `pageId` -- which a
/// resolver that dropped the query would answer wrongly rather than miss --
/// and a host no configured source ever wrote.
#[tokio::test]
async fn a_different_query_and_an_unknown_host_both_miss() {
    let pool = corpus("url_misses").await;

    assert_eq!(
        resolved(
            &pool,
            "https://confluence.example/pages/viewpage.action?pageId=98308"
        )
        .await
        .map(|found| found.entity_id),
        None,
        "the query is the page's identity, so another pageId is another page"
    );
    assert_eq!(
        resolved(&pool, "https://jira.other-company.example/browse/PAY-231")
            .await
            .map(|found| found.entity_id),
        None,
        "a host nothing in this mirror came from is a miss"
    );
    // And a path that exists nowhere on a host that does.
    assert_eq!(
        resolved(&pool, "https://jira.example/browse/PAY-999")
            .await
            .map(|found| found.entity_id),
        None,
    );
}

/// Story 17: two instances of one adapter, on two hosts, each answering only
/// for its own items -- because the stored URL carries the source's own base
/// and nothing here compares keys.
///
/// Both instances hold a ticket with **the same key**, which is what makes
/// this fixture able to witness the rule: a resolver that matched on anything
/// but the whole URL would answer one of them for both.
#[tokio::test]
async fn two_jira_instances_resolve_their_own_items_only() {
    let pool = pool("url_two_instances").await;
    mirror(
        &pool,
        "jira:PAY-231",
        "ticket",
        "jira",
        "https://jira.example/browse/PAY-231",
    )
    .await;
    mirror(
        &pool,
        "jira-eu:PAY-231",
        "ticket",
        "jira-eu",
        "https://jira-eu.example/browse/PAY-231",
    )
    .await;

    assert_eq!(
        pair(resolved(&pool, "https://jira.example/browse/PAY-231").await),
        Some(("jira:PAY-231".to_owned(), "ticket".to_owned())),
    );
    assert_eq!(
        pair(resolved(&pool, "https://jira-eu.example/browse/PAY-231").await),
        Some(("jira-eu:PAY-231".to_owned(), "ticket".to_owned())),
    );
}

/// Story 15: a stale link explains itself. The read is over `sync.item` and
/// not `sync.live_item`, so a withdrawn entity resolves and the detail's own
/// banner says it is gone -- which is a better answer than *Not in the
/// mirror*, that would send the reader to the browser to find out the same
/// thing.
#[tokio::test]
async fn a_tombstoned_item_still_resolves_to_its_id() {
    let pool = corpus("url_tombstone").await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind("jira:PAY-231")
        .execute(&pool)
        .await
        .expect("the tombstone");

    // The mirror row survives a withdrawal (§5a), and so does the address.
    assert_eq!(
        pair(resolved(&pool, "https://jira.example/browse/PAY-231").await),
        Some(("jira:PAY-231".to_owned(), "ticket".to_owned())),
    );
}

/// Not a miss and not an answer: a value that is not an absolute `http`/`https`
/// URL is refused by name. Offering *Open in browser* on it would offer the
/// reader something `shell/open-external.ts` refuses.
#[tokio::test]
async fn something_that_is_not_a_web_url_is_refused_rather_than_missed() {
    let pool = corpus("url_refusal").await;
    for not_a_url in [
        "file:///Users/bjoern/notes.md",
        "vscode://file/tmp",
        "PAY-231",
        "jira.example/browse/PAY-231",
        "",
    ] {
        let error = resolve_url_inner(&pool, not_a_url)
            .await
            .expect_err("not a web URL");
        assert_eq!(
            code(&error),
            IpcErrorCode::Invalid,
            "{not_a_url:?} is refused, not missed"
        );
    }
}

/// The resolver never reaches a row with no address, whatever is pasted --
/// including the empty-ish shapes SQL `null` and an unusable stored value
/// collapse to.
#[tokio::test]
async fn a_row_with_no_address_is_unreachable_by_paste() {
    let pool = corpus("url_no_address").await;
    mirror_without_url(&pool, "gitea:acme/payments-svc@main", "branch", "gitea").await;
    // A stored value that is not an absolute URL: the mirror holds whatever an
    // adapter reported, and the rule misses rather than guessing.
    mirror(&pool, "mock:relative", "ticket", "mock", "/browse/PAY-231").await;

    assert_eq!(
        resolved(&pool, "https://jira.example/browse/PAY-231")
            .await
            .map(|found| found.entity_id),
        Some("jira:PAY-231".to_owned()),
        "the usable row is still found beside the unusable ones"
    );
    assert_eq!(
        resolved(&pool, "http://x.example/browse/PAY-231")
            .await
            .map(|found| found.entity_id),
        None,
    );
}

// ---------------------------------------------------------------------------
// The index the read rests on.
// ---------------------------------------------------------------------------

/// Migration `0023` carries a **copy** of the normalisation expression,
/// because an index definition cannot expand a Rust macro. This is the check
/// that the copy is still the same characters.
///
/// It is cheap and exact, and it is deliberately not the only one: a copy can
/// be character-identical and still not be reached, and it can drift without
/// making a single answer wrong. The test below asks the planner the other
/// half of the question.
#[test]
fn the_migration_carries_the_macros_expression_verbatim() {
    const MIGRATION: &str =
        include_str!("../../knobas-db/migrations/0023_the_url_a_paste_names.sql");
    let expression = knobas_core::web_url_normalized!("web_url");
    assert!(
        MIGRATION.contains(expression),
        "migration 0023's index expression has drifted from `web_url_normalized!`, so the \
         resolver's statement no longer matches the index and every paste is a sequential \
         scan of the mirror. The macro now expands to:\n{expression}"
    );
}

/// The shipped statement reaches `item_web_url_norm_idx`.
///
/// The check `the_view_still_reaches_the_fts_index` makes for the launcher's
/// GIN index, for the same reason: an expression index that the query's own
/// expression no longer matches is invisible -- every answer stays right, the
/// read silently becomes a sequential scan over the whole mirror, and only a
/// benchmark would notice.
///
/// `RESOLVE_URL` itself, not a paraphrase of it: what is being asked is
/// whether *the statement the command runs* matches the index.
#[tokio::test]
async fn the_resolvers_statement_reaches_the_expression_index() {
    let pool = corpus("url_index_plan").await;

    let mut tx = pool.begin().await.expect("a transaction");
    // A test corpus is small enough that a sequential scan wins on cost; this
    // asks the planner what it would do with one that is not.
    sqlx::query("set local enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .expect("seqscan off");
    let plan: Vec<String> =
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("explain {RESOLVE_URL}")))
            .bind("https://jira.example/browse/PAY-231")
            .fetch_all(&mut *tx)
            .await
            .expect("a plan");
    let plan = plan.join("\n");
    // **An `Index Cond`, not merely the index's name.** The index is partial,
    // so `where web_url is not null` alone lets the planner bitmap-scan the
    // whole of it and recheck the comparison as a `Filter:` -- a plan that
    // names the index and reads every mirrored URL in it. That is exactly what
    // a drifted expression produces, and it is what an assertion on the name
    // alone would call a pass (measured while mutating migration 0023: one
    // character class changed, this test stayed green until this line).
    //
    // The condition names the expression by a literal of its own rather than
    // by a fragment: `lower(` alone appears in the `Filter:` too, so a check
    // for that would pass on exactly the plan this rejects.
    const AUTHORITY: &str = "'^[^:/?#]+://[^/?#]*'";
    let lines: Vec<&str> = plan.lines().map(str::trim).collect();
    let key = lines.iter().find(|line| line.starts_with("Index Cond:"));
    assert!(
        plan.contains("item_web_url_norm_idx") && key.is_some_and(|line| line.contains(AUTHORITY)),
        "the normalised URL must be the index *key* migration 0023 created:\n{plan}"
    );
    // And nowhere else: a `Filter:` carrying it is the whole index read row by
    // row, which is the work this index exists to avoid.
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("Filter:") && line.contains(AUTHORITY)),
        "the comparison must not be rechecked per row:\n{plan}"
    );
    tx.rollback().await.expect("rollback");
}

/// The index is partial, and the rows it leaves out are the ones a paste can
/// never name.
#[tokio::test]
async fn the_index_holds_only_the_rows_that_carry_an_address() {
    let pool = pool("url_index_shape").await;
    let (definition,): (String,) = sqlx::query_as(
        "select indexdef from pg_indexes
          where schemaname = 'sync' and indexname = 'item_web_url_norm_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("migration 0023 created the index");

    assert!(
        definition.contains("WHERE (web_url IS NOT NULL)"),
        "the index must skip every row with no address:\n{definition}"
    );
    assert!(
        !definition.contains("UNIQUE"),
        "two mirrored items may report one address; a unique index would turn that into a \
         failed sync:\n{definition}"
    );
}

// ---------------------------------------------------------------------------
// The wiring: registered, named, and decoding.
// ---------------------------------------------------------------------------

use tauri::Manager;

/// Invoke `resolve_url` on a mock app that manages a `Lifecycle` with no pool.
///
/// The command asks `lifecycle.pool()?` first, so a call with nothing ready
/// reaches the *body* and answers `not_ready` -- which is the marker for
/// "registered and its argument decoded", as distinct from "no such command"
/// or "invalid args".
fn invoke(body: serde_json::Value) -> Result<serde_json::Value, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::entity::resolve_url
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(knobas_app::Lifecycle::new());
    let webview: tauri::WebviewWindow<MockRuntime> =
        tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("mock webview");

    tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "resolve_url".to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"))
    .and_then(|answered| {
        answered
            .deserialize::<serde_json::Value>()
            .map_err(|error| format!("the answer is not JSON: {error}"))
    })
}

/// `resolve_url` is dispatched under that name, with the one argument the
/// mirror sends.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at
/// run time with "command not found" against a Rust side that compiles. The
/// second call is the argument's own name: Tauri camelCases command arguments,
/// `url` is one word either way, and a call with the wrong key is refused
/// before the body runs rather than answering `not_ready`.
#[test]
fn resolve_url_is_registered_and_its_argument_decodes() {
    let rejection = invoke(serde_json::json!({ "url": "https://jira.example/browse/PAY-231" }))
        .expect_err("no pool is managed, so the body refuses");
    assert!(
        rejection.contains("not_ready"),
        "resolve_url must dispatch and reach its body; it answered: {rejection}"
    );

    let missing = invoke(serde_json::json!({})).expect_err("the argument is required");
    assert!(
        !missing.contains("not_ready"),
        "a call with no url must be refused while its arguments are decoded, not reach the \
         body: {missing}"
    );
}
