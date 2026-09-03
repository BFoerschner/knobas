//! The IPC surface as Tauri actually decodes it.
//!
//! Ruling P3 asks one thing to be verified before seven streams build on it:
//! does `Option<tauri::ipc::Channel<_>>` decode when the caller **omits** the
//! argument? The answer in Tauri 2.11 is **no, and not at runtime either** --
//! it does not compile. `Channel` implements `Serialize` and its own
//! `CommandArg`, but no `Deserialize`, and the only route from
//! `Option<Channel<_>>` to `CommandArg` is the blanket
//! `impl<'de, D: Deserialize<'de>, R> CommandArg<'de, R> for D`. rustc, handed
//! a `#[tauri::command]` with that argument, says so in as many words:
//!
//! ```text
//! error[E0277]: the trait bound `Channel<Progress>: serde::Deserialize<'de>` is not satisfied
//!   = note: required for `Option<Channel<Progress>>` to implement `Deserialize<'_>`
//!   = note: required for `Option<Channel<Progress>>` to implement `CommandArg<'_, _>`
//! ```
//!
//! So P3's documented escape hatch is the one in force: the surface is split
//! into `sync_now` (no channel) and `sync_now_with_progress` (channel
//! required). A compile error cannot itself be a `#[test]`, so the verdict is
//! pinned here three ways that *can* run:
//!
//! 1. [`the_optional_channel_is_not_a_decodable_argument`] asks the trait
//!    solver the same question the command macro asks, and answers it at
//!    runtime -- with controls either side, so a probe that had silently
//!    stopped working would fail rather than pass.
//! 2. [`the_two_argument_shapes_decode_as_the_split_needs`] drives both shapes
//!    through the real IPC pipeline: `{ sourceId }` alone, and `{ sourceId }`
//!    with and without a channel where one is required.
//! 3. [`both_halves_of_sync_now_are_registered_and_reachable`] checks that the
//!    app's own two commands are in the handler list and dispatch.
//!
//! Why (2) and (3) are separate: a `#[tauri::command]` resolves its arguments
//! in declaration order, and both real commands take `app: AppHandle<R>` and
//! then `State<'_, Lifecycle>` before they reach `source_id` or `progress`.
//! The handle always resolves; the state does not, because a mock app built
//! here manages nothing by default -- so every call to them stops there,
//! before `progress` is looked at. (2) therefore uses commands of the same two
//! *trailing* argument shapes with nothing in front of them, which is the only
//! way to watch the decoding itself happen.
//!
//! A fourth section rides along, for a different contract: ruling P13's demo
//! guard. Argument resolution runs to completion *before* any command body, so
//! reaching the guard means managing both the `Profile` and a `Lifecycle` with
//! an `AppState` in it -- the latter over a pool pointing at a closed port, so
//! that "the guard refused" and "the database was touched" cannot be confused
//! for one another.
//!
//! A fifth section covers what the asynchronous bring-up made reachable
//! (carry-over §10.6(a)): a command that arrives *before* the database is up
//! now answers `not_ready` with a code, and `app_status` answers at all.

use std::marker::PhantomData;

use knobas_sync::{SyncPhase, SyncProgress};
use tauri::ipc::{CallbackFn, Channel, CommandArg};
use tauri::test::MockRuntime;
use tauri::{Listener, Manager};

// ---------------------------------------------------------------------------
// 1. The verdict, asked of the trait solver.
// ---------------------------------------------------------------------------

/// A type-level question: would `#[tauri::command]` accept `T` as an argument?
///
/// `#[tauri::command]` requires each argument to implement [`CommandArg`] for
/// the app's runtime and any lifetime, so that is exactly the bound asked
/// about here.
struct Argument<T>(PhantomData<T>);

/// Answers `true`, and applies only when `T` really is a command argument.
trait Decodable {
    fn is_decodable(&self) -> bool {
        true
    }
}
impl<T> Decodable for Argument<T> where T: for<'de> CommandArg<'de, MockRuntime> {}

/// Answers `false`. Implemented for `&Argument<T>` with no bound at all, so it
/// sits one autoref further out than [`Decodable`] and is reached only when
/// the bound above does not hold -- the same inherent-vs-autoref probe
/// `anyhow` uses to tell a `Display` from an `Error`.
trait NotDecodable {
    fn is_decodable(&self) -> bool {
        false
    }
}
impl<T> NotDecodable for &Argument<T> {}

/// Whether `T` can be a `#[tauri::command]` argument.
macro_rules! decodable {
    ($t:ty) => {
        (&Argument::<$t>(PhantomData)).is_decodable()
    };
}

/// P3's open question, answered: `Option<Channel<_>>` is **not** a command
/// argument in Tauri 2.11, so "omitted means no progress" was never available.
///
/// The three controls are the point. Without them this test would pass just as
/// happily if the probe had degenerated into "always false" -- and the whole
/// case for splitting the command rests on this one `assert!`.
#[test]
fn the_optional_channel_is_not_a_decodable_argument() {
    // Control: an ordinary deserializable argument, and an optional one --
    // `Option<T>` as such is fine, so the failure below is about `Channel`.
    assert!(decodable!(String), "the probe rejects a plain argument");
    assert!(
        decodable!(Option<String>),
        "the probe rejects an optional argument"
    );
    // Control: a bare channel *is* an argument -- it has its own `CommandArg`
    // impl, which is precisely why wrapping it in an `Option` loses it.
    assert!(
        decodable!(Channel<SyncProgress>),
        "the probe rejects a bare channel"
    );

    // The verdict.
    assert!(
        !decodable!(Option<Channel<SyncProgress>>),
        "an optional channel decodes after all -- P3's fallback can be undone \
         and the two sync commands collapsed back into one"
    );
}

// ---------------------------------------------------------------------------
// 2. The same verdict, through the real IPC pipeline.
// ---------------------------------------------------------------------------

/// The two argument shapes the split produces, with no `State` in front of
/// them so that argument decoding is the only thing that can fail.
///
/// They are not the app's commands -- those are exercised by
/// [`both_halves_of_sync_now_are_registered_and_reachable`] -- but they are the
/// app's *signatures*, minus the state argument that would stop a mock call
/// before `progress` is reached.
mod shapes {
    use super::{SyncPhase, SyncProgress};

    /// `sync_now`'s shape: no channel at all.
    #[tauri::command]
    pub fn without_progress(source_id: String) -> Result<String, String> {
        Ok(source_id)
    }

    /// `sync_now_with_progress`'s shape: a channel, required.
    ///
    /// It sends on the channel rather than only accepting one, so the test
    /// covers the whole round trip -- a `SyncProgress` that decoded as an
    /// argument but could not be *sent* would be a channel in name only.
    #[tauri::command]
    pub fn with_progress(
        source_id: String,
        progress: tauri::ipc::Channel<SyncProgress>,
    ) -> Result<u32, String> {
        progress
            .send(SyncProgress {
                run_id: 1,
                source_id,
                phase: SyncPhase::Started,
                items: 0,
                elapsed_ms: 0,
                message: None,
            })
            .map_err(|error| error.to_string())?;
        Ok(progress.id())
    }
}

/// The origin an invoke has to come from for Tauri to treat it as the app's
/// own frontend rather than as remote content.
///
/// Load-bearing, and the reason it is spelled per-platform: a request Tauri
/// does not recognise as local is refused by the ACL *before* any argument is
/// decoded, and `mock_context` builds an app with an empty ACL. Point this at
/// the wrong scheme and every test below still "passes" -- against the
/// permission check, having never reached the command. [`invoke`] asserts that
/// this has not happened.
#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// Invoke `cmd` on a mock app carrying both the app's sync commands and the
/// two shapes above, and return the outcome with the rejection stringified.
fn invoke(cmd: &str, body: serde_json::Value) -> Result<tauri::ipc::InvokeResponseBody, String> {
    invoke_managing(cmd, body, |_| {})
}

/// [`invoke`], with `manage` given the app first.
///
/// A `#[tauri::command]` resolves *every* argument before its body runs, so a
/// command guarding on managed state is unreachable until that state exists --
/// which is why the demo tests below hand this a `Profile` and an `AppState`
/// rather than asserting against "state not managed".
fn invoke_managing(
    cmd: &str,
    body: serde_json::Value,
    manage: impl FnOnce(&tauri::App<MockRuntime>),
) -> Result<tauri::ipc::InvokeResponseBody, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::app::app_status,
            knobas_app::commands::app::frontend_ready,
            knobas_app::commands::entity::create_link,
            knobas_app::commands::entity::recent_activity,
            knobas_app::commands::entity::unlink,
            knobas_app::commands::entity::submit_write,
            knobas_app::commands::sources::demo_load,
            knobas_app::commands::sources::list_adapters,
            knobas_app::commands::sources::list_sources,
            knobas_app::commands::sources::add_source,
            knobas_app::commands::sources::update_source,
            knobas_app::commands::sources::delete_source,
            knobas_app::commands::sources::set_source_secret,
            knobas_app::commands::sources::test_source,
            knobas_app::commands::sources::credential_health,
            knobas_app::commands::sources::sync_now,
            knobas_app::commands::sources::sync_now_with_progress,
            knobas_app::commands::sources::backfill_source,
            knobas_app::commands::sources::sync_all,
            knobas_app::commands::sources::sync_status,
            knobas_app::commands::sources::list_sync_runs,
            knobas_app::commands::sources::db_stats,
            knobas_app::commands::sources::reindex_fts,
            shapes::without_progress,
            shapes::with_progress
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    manage(&app);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("mock webview");

    let outcome = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"));

    if let Err(rejection) = &outcome {
        assert!(
            !rejection.contains("not allowed"),
            "the permission check refused {cmd} before its arguments were decoded, \
             so this harness proves nothing about decoding: {rejection}"
        );
    }
    outcome
}

/// The split surface, decoded by Tauri itself: an argument list without a
/// channel is complete, and one with a required channel is not complete
/// without it.
///
/// All three directions, because only the set is evidence. The middle case is
/// what a caller that *forgot* the channel gets, and it is the failure the
/// first case must not produce.
#[test]
fn the_two_argument_shapes_decode_as_the_split_needs() {
    let plain = invoke(
        "without_progress",
        serde_json::json!({ "sourceId": "mock" }),
    )
    .expect("`sourceId` alone is a complete argument list")
    .deserialize::<String>()
    .expect("a string came back");
    assert_eq!(plain, "mock");

    let omitted = invoke("with_progress", serde_json::json!({ "sourceId": "mock" }))
        .expect_err("a required channel cannot be omitted");
    assert!(
        omitted.contains("progress"),
        "an omitted channel must be refused by name: {omitted}"
    );

    // `__CHANNEL__:` is Tauri's own wire form for a channel handle
    // (`tauri::ipc::channel::IPC_PAYLOAD_PREFIX`). Supplied, it decodes into a
    // channel bound to this webview, and a `SyncProgress` goes down it.
    let supplied = invoke(
        "with_progress",
        serde_json::json!({ "sourceId": "mock", "progress": "__CHANNEL__:7" }),
    )
    .expect("a supplied channel must decode and send")
    .deserialize::<u32>()
    .expect("a channel id came back");
    assert_eq!(
        supplied, 7,
        "the decoded channel is the one the caller named"
    );
}

// ---------------------------------------------------------------------------
// 3. The app's own commands, in the handler list.
// ---------------------------------------------------------------------------

/// Tauri's own refusal when a command declares state that is not managed.
///
/// Not an `IpcError`: no code, nothing the frontend can branch on. Carry-over
/// §10.6(a) is that this must never be what a call during bring-up gets, which
/// is why no command declares `State<'_, AppState>` or `State<'_, SourcesState>`.
const TAURI_STATE_REFUSAL: &str = "state not managed";

/// What a stream-F command answers when the sync engine has not started.
///
/// A real `IpcErrorCode::NotReady`, produced by the command's own body after
/// `crate::sources::state(&app)` found nothing -- which is only possible
/// because those commands take an `AppHandle` (always resolvable) rather than
/// declaring the state as an argument. It is therefore the marker for "this
/// command is registered and its body ran", as distinct from "no such command".
const SOURCES_NOT_READY: &str = "not_ready";

/// Every stream-F command is registered under the name the TypeScript mirror
/// invokes, and dispatches.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at run
/// time with "command not found" and a Rust side that compiles perfectly.
/// Seventeen commands is well past the point where that is noticed by hand.
#[test]
fn every_sources_command_is_registered_and_reachable() {
    // A complete argument list per command, in the camelCase spelling Tauri
    // renames arguments to -- which is the spelling `app/src/lib/ipc/sources.ts`
    // sends. A missing or misspelled key fails here as an argument-resolution
    // error rather than as `not_ready`, so this covers the mirror's call sites
    // as well as the handler list.
    let draft = serde_json::json!({
        "source_id": null, "adapter_kind": "mock", "base_url": "",
        "auth_kind": "Pat", "config": {}, "secret": null
    });
    let new_source = serde_json::json!({
        "id": "mock2", "adapter_kind": "mock", "display_name": "M", "base_url": "",
        "auth_kind": "Pat", "config": {}, "secret": { "value": "x" },
        "sync_interval_secs": 300, "enabled": true
    });
    for (cmd, args) in [
        ("list_sources", serde_json::json!({})),
        ("add_source", serde_json::json!({ "input": new_source })),
        (
            "update_source",
            serde_json::json!({ "id": "mock", "patch": {} }),
        ),
        (
            "delete_source",
            serde_json::json!({ "id": "mock", "purgeItems": false }),
        ),
        (
            "set_source_secret",
            serde_json::json!({ "id": "mock", "secret": { "value": "x" } }),
        ),
        ("test_source", serde_json::json!({ "draft": draft })),
        ("credential_health", serde_json::json!({})),
        ("sync_now", serde_json::json!({ "sourceId": "mock" })),
        (
            "sync_now_with_progress",
            serde_json::json!({ "sourceId": "mock", "progress": "__CHANNEL__:1" }),
        ),
        ("backfill_source", serde_json::json!({ "sourceId": "mock" })),
        ("sync_all", serde_json::json!({})),
        ("sync_status", serde_json::json!({})),
        (
            "list_sync_runs",
            serde_json::json!({ "sourceId": null, "limit": 5 }),
        ),
        ("db_stats", serde_json::json!({})),
        ("reindex_fts", serde_json::json!({})),
    ] {
        let rejection = invoke(cmd, args).expect_err("no SourcesState is managed");
        assert!(
            rejection.contains(SOURCES_NOT_READY),
            "{cmd} was not dispatched, or its arguments do not decode: {rejection}"
        );
        assert!(
            !rejection.contains(TAURI_STATE_REFUSAL),
            "{cmd} declares managed state as an argument -- carry-over §10.6(a): \
             {rejection}"
        );
    }

    // `list_adapters` is the exception, deliberately: it touches neither the
    // database nor the keychain, so it answers on a cold start -- which is what
    // lets the Add-source form be drawn before bring-up finishes.
    let adapters = invoke("list_adapters", serde_json::json!({}))
        .expect("list_adapters must answer without any managed state")
        .deserialize::<serde_json::Value>()
        .expect("a descriptor list came back");
    assert!(
        adapters.as_array().is_some_and(|a| !a.is_empty()),
        "list_adapters returned nothing: {adapters}"
    );
}

// ---------------------------------------------------------------------------
// 4. The demo profile's guard, through the same pipeline (ruling P13).
// ---------------------------------------------------------------------------

/// A pool over a port nothing listens on.
///
/// `connect_lazy` opens no connection, so this costs nothing while the guard
/// holds -- and *fails* the moment a command gets past the guard and tries to
/// query, which is exactly the signal the second test below reads. The timeout
/// bounds that failure: nothing is listening on port 1, so the only question is
/// how long the pool retries before saying so.
///
/// Built inside Tauri's runtime because sqlx spawns the pool's reaper on the
/// ambient one, and this is the runtime the command bodies will run on anyway.
fn unreachable_pool() -> sqlx::PgPool {
    tauri::async_runtime::block_on(async {
        sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://knobas@127.0.0.1:1/knobas")
            .expect("a lazy pool needs no server")
    })
}

/// A lifecycle holding a live-looking pool, the way bring-up leaves one.
/// A real, running `SourcesState` over `pool` -- the state the sync commands
/// ask the `AppHandle` for.
///
/// Built by the same `sources::start`-shaped call the app makes, so this test
/// drives the scheduler rather than a stand-in: the run it triggers is a real
/// run on a real dedicated connection.
///
/// `db` is where those dedicated connections come from, and it has to be the
/// database `pool` is on: a caller on the binary's shared database passes
/// [`test_connector`](knobas_db::test_util::test_connector), and one on a
/// database of its own passes that (#300).
fn sources_state(
    app: &tauri::App<MockRuntime>,
    db: knobas_db::embedded::Connector,
    pool: sqlx::PgPool,
) -> knobas_app::sources::SourcesState {
    use std::sync::Arc;
    let handle = app.handle().clone();
    // `block_in_place` first: this runs inside the test's multi-thread runtime,
    // and `block_on` from within one panics.
    tokio::task::block_in_place(|| {
        tauri::async_runtime::block_on(async move {
            knobas_app::sources::SourcesState {
                scheduler: knobas_app::sources::test_scheduler(&handle, db, pool.clone())
                    .await
                    .expect("a scheduler over the test pool"),
                pool,
                secrets: Arc::new(knobas_secrets::MemoryStore::new()),
                registry: Arc::new(knobas_app::sources::Registry::builtin()),
            }
        })
    })
}

fn ready_over(pool: sqlx::PgPool) -> knobas_app::Lifecycle {
    let lifecycle = knobas_app::Lifecycle::new();
    lifecycle.install(knobas_app::AppState::over_pool(pool));
    lifecycle.set(knobas_app::DbState::Ready);
    lifecycle
}

/// Invoke `demo_load` with `profile` and a database nothing is listening on.
fn invoke_demo_load(profile: knobas_app::Profile) -> Result<(), String> {
    invoke_managing("demo_load", serde_json::json!({}), move |app| {
        app.manage(profile);
        app.manage(ready_over(unreachable_pool()));
    })
    .map(|_| ())
}

/// The fixture is refused outside the demo profile -- and refused *before* the
/// database is touched, which is what the unreachable pool proves: a guard that
/// ran after the first query would surface a connection error instead.
#[test]
fn demo_load_is_refused_outside_the_demo_profile() {
    let real = knobas_app::Profile::from_args(
        Vec::<String>::new(),
        std::path::Path::new("/tmp/knobas-test"),
    );
    assert!(!real.demo, "the control profile is not the demo one");

    let rejection = invoke_demo_load(real).expect_err("demo data belongs to the demo profile");
    assert!(
        rejection.contains(knobas_app::DEMO_FLAG),
        "the refusal must name the flag that fixes it: {rejection}"
    );
    assert!(
        rejection.contains("invalid"),
        "the refusal carries IpcErrorCode::Invalid: {rejection}"
    );
    assert!(
        !rejection.contains(TAURI_STATE_REFUSAL),
        "the command was never dispatched, so this proves nothing: {rejection}"
    );
}

/// ...and the demo profile is let through. Without this the test above would
/// pass just as well against a `demo_load` that refuses everybody.
///
/// It gets as far as the database and no further -- the pool points at nothing
/// -- so the assertion is that the failure is *not* the guard's.
#[test]
fn demo_load_passes_the_guard_in_the_demo_profile() {
    let demo = knobas_app::Profile::from_args(
        vec![knobas_app::DEMO_FLAG.to_owned()],
        std::path::Path::new("/tmp/knobas-test"),
    );
    assert!(demo.demo, "the flag selects the demo profile");

    let rejection = invoke_demo_load(demo).expect_err("the pool points at no server");
    assert!(
        !rejection.contains(knobas_app::DEMO_FLAG),
        "the demo profile was refused its own fixture: {rejection}"
    );
}

/// A demo load ends with the terminal `sync:state` its run owes (#240).
///
/// The contract's event table says `sync:state` fires on every run transition
/// and P3 was granted as "all runs emit coarse `sync:state`". The demo load
/// syncs through the bare `run_once`, which emits nothing, so it was the one
/// run whose ending nothing in the window could hear -- and the projects
/// store, which re-lists the census only on a terminal `sync:state`, never
/// learned about the two projects the fixture exists for unless *Finish* was
/// pressed. This pins the command end to end: the real handler, the real
/// `TauriEvents`, the real event name, listened for the way the window does.
///
/// The command awaits its run, so the event is already there when it returns;
/// no polling. `run_id` is null because the demo load writes no `sync_run`
/// row (out of scope by ruling), and the store only needs `running: false`.
#[tokio::test(flavor = "multi_thread")]
async fn demo_load_ends_with_a_terminal_sync_state_for_the_mock() {
    // A database of its own, not the binary's shared one: `sync_now_answers_..`
    // opens a real `mock` run on the shared database, and `status_for` reads
    // whatever run is open for the source -- so on the shared database this
    // emit came back `running: true, run_id: <the sibling's run>` in one gate
    // run out of three, which is the packaged-app edge the PR records (the
    // scheduler's own run open while the demo's is in flight), reproduced by
    // a neighbour. The isolation keeps the assertion about *this* command's
    // emit rather than about which test won a race.
    let pool = knobas_db::test_util::scratch_database("demo_load_sync_state")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    let demo = knobas_app::Profile::from_args(
        vec![knobas_app::DEMO_FLAG.to_owned()],
        std::path::Path::new("/tmp/knobas-test"),
    );

    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let recorder = std::sync::Arc::clone(&seen);

    let report = invoke_managing("demo_load", serde_json::json!({}), move |app| {
        app.manage(demo);
        app.manage(ready_over(pool));
        app.listen("sync:state", move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                recorder.lock().unwrap().push(value);
            }
        });
    })
    .expect("the demo profile loads its fixture")
    .deserialize::<serde_json::Value>()
    .expect("a report came back");
    assert_eq!(report["source_id"], serde_json::json!("mock"));

    let seen = seen.lock().unwrap();
    let terminal = seen
        .iter()
        .find(|status| status["running"] == serde_json::json!(false))
        .unwrap_or_else(|| {
            panic!(
                "a demo load must end with a terminal sync:state -- the projects store \
                 re-lists the census on nothing else; saw {seen:?}"
            )
        });
    assert_eq!(terminal["source_id"], serde_json::json!("mock"));
    assert_eq!(
        seen.iter()
            .filter(|s| s["running"] == serde_json::json!(true))
            .count(),
        0,
        "the demo load has no open run row, so it must not claim to be running: {seen:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. `sync_now` answers before the run does, and says so on `sync:state`.
// ---------------------------------------------------------------------------

/// Ruling P3's other half: the id comes back immediately and the coarse event
/// is what a UI watches.
///
/// The docs on both commands promise `EVENTS.syncState` carries the run, so
/// the promise is asserted rather than written: a `running: true` status must
/// already have been emitted by the time the command returns (it is emitted
/// before the spawn, so this is deterministic), and a terminal one must follow
/// with the same `run_id` once the spawned run lands.
///
/// What is deliberately *not* asserted is a timing race -- "the row was still
/// open when the command returned" is true and unprovable, because the mock
/// syncs an in-memory fixture and can beat the assertion. The evidence that
/// the run is not awaited is structural instead: the command's own value
/// arrives with only the `started` event behind it, and the counts only turn
/// up later.
#[tokio::test(flavor = "multi_thread")]
async fn sync_now_answers_before_the_run_and_reports_it_on_the_event() {
    // A database of its own, not the binary's shared one -- the remedy
    // `demo_load_ends_with_a_terminal_sync_state_for_the_mock` already needed,
    // for the same reason (#300).
    //
    // The scheduler this test manages is the real one, so on the shared
    // database it is one of several live over a single `source_config`, and
    // `emit_state` does not build its payload from the run it is emitting for:
    // it re-reads the source through `status_for`, whose query takes
    // `coalesce(r.id, f.id)` from *whichever* run is open. A neighbour's open
    // `mock` run therefore puts a **neighbour's** id on this run's `running`
    // emit, and the run id stops telling the events apart -- PR #302's gate
    // failed exactly there, on `running` rather than on `run_id`.
    //
    // One scheduler over one database has at most one open `mock` run: while
    // this run is in flight `trigger` joins it rather than starting a second,
    // and `sync_interval_secs` (300 s) keeps the ticker from starting another
    // inside the five seconds this test waits. So every emit here is about the
    // run it was triggered for, and the run-id keying below is then belt and
    // braces rather than the load-bearing part it was asked to be.
    let db = knobas_db::test_util::scratch_database("sync_now_sync_state").await;
    let pool = db
        .pool(5)
        .await
        .expect("a pool onto this test's own database");
    // Registers `mock` in `source_config`, which is what `prepare_sync`
    // requires. The sync it performs is incidental.
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .unwrap();

    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let recorder = std::sync::Arc::clone(&seen);

    let pool_for_state = pool.clone();
    let returned = invoke_managing(
        "sync_now",
        serde_json::json!({ "sourceId": "mock" }),
        move |app| {
            app.manage(ready_over(pool_for_state.clone()));
            app.manage(sources_state(app, db, pool_for_state));
            app.listen("sync:state", move |event| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    recorder.lock().unwrap().push(value);
                }
            });
        },
    )
    .expect("sync_now must answer")
    .deserialize::<i64>()
    .expect("a run id came back");

    // **This run's** events, by the id `sync_now` returned, rather than the
    // first and last to arrive (#300).
    //
    // The private database above is what makes the id trustworthy; this is
    // what makes each assertion say *which* run it is about. `first()` and the
    // last terminal event of any run are claims about arrival order, and this
    // scheduler is real: should a second `mock` run ever overlap this one --
    // a shorter `sync_interval_secs`, a second source in the demo fixture, a
    // future test reaching this database -- the searches fail loudly instead
    // of asserting against the wrong run. They also carry the `run_id` claim
    // the two `assert_eq!`s here used to make after the fact, which is why
    // neither asserts it again.
    let mine = |status: &serde_json::Value| status["run_id"] == serde_json::json!(returned);

    // Emitted before the spawn, so it is already there.
    let started = seen
        .lock()
        .unwrap()
        .iter()
        .find(|status| mine(status))
        .cloned()
        .expect(
            "a `running` sync:state must be emitted before sync_now returns -- \
             otherwise EVENTS.syncState is a promise the docs make and nothing keeps",
        );
    assert_eq!(started["running"], serde_json::json!(true));
    assert_eq!(started["source_id"], serde_json::json!("mock"));

    // The run lands on its own task. Wait for the **event**, not for the log
    // row: the terminal emit happens after `run_log::finish` commits, so a
    // poll that stops at the closed row can read `seen` before the emit that
    // follows it and fail intermittently. The event is the last thing to
    // happen, so waiting for it is what makes both assertions safe.
    let mut terminal = None;
    for _ in 0..200 {
        terminal = seen
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|status| mine(status) && status["running"] == serde_json::json!(false))
            .cloned();
        if terminal.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let terminal = terminal.expect("a terminal sync:state must follow the run");
    assert_eq!(terminal["last_outcome"], serde_json::json!("ok"));

    // And by then the row it describes is closed, because the emit is the last
    // thing the task does.
    let outcome: Option<String> = sqlx::query_scalar(
        "select outcome from knobas.sync_run where id = $1 and finished_at is not null",
    )
    .bind(returned)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outcome.as_deref(),
        Some("ok"),
        "the spawned run must close its own log row"
    );
}

/// Issue #43: `submit_write` queues the write, the queue delivers it, and the
/// mirror is re-read for the source **without waiting for the next scheduled
/// sync** (story 15).
///
/// Three claims, and each is asserted where it actually happens:
///
/// * the command answers with the write **as queued**, before the attempt --
///   which is why the state it comes back with is `pending` and not `sent`;
/// * the queue then delivers it, so the row settles as `sent`;
/// * a sync run for that source follows. knobas has no per-entity read -- the
///   whole of the read direction is `Source::sync` -- so "refresh the mirror
///   for the affected entity" is an *incremental* run of its source, which is
///   what the log row proves happened.
///
/// The mock adapter declares `comment` and accepts it, so the write really
/// goes; nothing here stubs the queue.
#[tokio::test(flavor = "multi_thread")]
async fn a_submitted_write_is_queued_delivered_and_followed_by_a_re_read() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    // A **second instance** of the mock adapter, not the demo's `mock`:
    // `sync_now_answers_before_the_run_and_reports_it_on_the_event` drives that
    // one, this test triggers a run of its own, and the scheduler's in-flight
    // dedupe would hand the two the same run. Two sources never wait on one
    // another (story 21), which is exactly what makes them safe to run in
    // parallel here.
    let source_id = "mock-write";
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config
                (id, kind, display_name, base_url, auth_kind, config,
                 sync_interval_secs, enabled)
         values ($1, 'mock', 'Mock (writes)', '', 'none', '{}'::jsonb, 86400, true)
         on conflict (id) do nothing",
    )
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();

    // **A run this source has already had**, recorded as finished now. Without
    // it the scheduler's own tick syncs `mock-write` immediately -- a source
    // with no run behind it is due at once -- and the assertion below would
    // pass with the write-triggered re-read deleted. Found by mutation: it did.
    // With a day's interval and a run a moment ago, the *only* thing that can
    // start another run here is the write landing.
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, started_at, finished_at, outcome)
         values ($1, 'schedule', now(), now(), 'ok')",
    )
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();

    let runs_before: i64 =
        sqlx::query_scalar("select count(*) from knobas.sync_run where source_id = $1")
            .bind(source_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let pool_for_state = pool.clone();
    // The shared database `pool` is on, which is where this test's sources are.
    let db = knobas_db::test_util::test_connector().await;
    let queued = invoke_managing(
        "submit_write",
        serde_json::json!({
            "payload": { "Comment": { "entity": "mock-write:PAY-231", "body": "on it" } }
        }),
        move |app| {
            app.manage(ready_over(pool_for_state.clone()));
            app.manage(sources_state(app, db, pool_for_state));
        },
    )
    .expect("submit_write must answer")
    .deserialize::<serde_json::Value>()
    .expect("the queued write came back");

    assert_eq!(queued["source_id"], serde_json::json!(source_id));
    assert_eq!(queued["op"], serde_json::json!("comment"));
    assert_eq!(
        queued["state"],
        serde_json::json!("pending"),
        "the row is the write *as queued*, before the attempt -- reporting `sent` \
         from it would be reporting a hope"
    );
    let id = queued["id"].as_i64().expect("a queue id");

    let settled: Option<String> =
        sqlx::query_scalar("select state from knobas.write_queue where id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        settled.as_deref(),
        Some("sent"),
        "the queue delivers a write a source can take, before the command returns"
    );

    // The re-read is asked for after the write lands and runs on its own task.
    let mut runs_after = runs_before;
    for _ in 0..200 {
        runs_after =
            sqlx::query_scalar("select count(*) from knobas.sync_run where source_id = $1")
                .bind(source_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        if runs_after > runs_before {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        runs_after > runs_before,
        "a write that landed must be followed by a run that re-reads the source, or the \
         app disagrees with itself until the next scheduled sync (story 15)"
    );
}

// ---------------------------------------------------------------------------
// 6. The window can now call before the database is up (carry-over §10.6(a)).
// ---------------------------------------------------------------------------

/// A command that needs the pool, called during bring-up, answers `not_ready`.
///
/// This is the carry-over the asynchronous bring-up turned from latent into
/// live, and the reason every such command takes `State<'_, Lifecycle>` rather
/// than `State<'_, AppState>`. With the old shape the call never reaches a
/// command body at all: Tauri refuses it while resolving arguments and the
/// frontend gets the bare string `"state not managed"`, which carries no code,
/// which means the shell cannot tell "still starting" from "broken".
///
/// A `Lifecycle` is managed here and no `AppState` is installed -- exactly the
/// window between `setup` and a live pool.
#[test]
fn a_command_that_beats_the_database_is_told_to_try_again() {
    let rejection = invoke_managing(
        "recent_activity",
        serde_json::json!({ "limit": 5 }),
        |app| {
            app.manage(knobas_app::Lifecycle::new());
        },
    )
    .expect_err("there is no pool yet");

    assert!(
        rejection.contains("not_ready"),
        "the refusal must carry IpcErrorCode::NotReady, or the shell cannot \
         tell a starting database from a broken one: {rejection}"
    );
    assert!(
        !rejection.contains(TAURI_STATE_REFUSAL),
        "Tauri refused this while resolving arguments, so the command's own \
         answer was never reached: {rejection}"
    );
}

/// ...and once the pool is there the same call gets past the guard.
///
/// Without this the test above would pass just as well against a
/// `recent_activity` that answers `not_ready` to everybody. The pool points at
/// nothing, so the assertion is that the failure is no longer the lifecycle's.
#[test]
fn the_same_command_gets_through_once_the_database_is_up() {
    let rejection = invoke_managing(
        "recent_activity",
        serde_json::json!({ "limit": 5 }),
        |app| {
            app.manage(ready_over(unreachable_pool()));
        },
    )
    .expect_err("the pool points at no server");

    assert!(
        !rejection.contains("not_ready"),
        "a live lifecycle must not refuse as if it were still starting: {rejection}"
    );
    assert!(
        rejection.contains("internal"),
        "a connection failure is internal: {rejection}"
    );
}

/// `app_status` answers while the database is still coming up -- which is the
/// entire point of it (interfaces §2.1) and impossible for any command taking
/// `State<'_, AppState>`.
#[test]
fn app_status_answers_before_the_database_does() {
    let lifecycle = knobas_app::Lifecycle::new();
    lifecycle.set(knobas_app::DbState::Starting {
        detail: Some("first run: downloading and initialising PostgreSQL".to_owned()),
    });

    let status = invoke_managing("app_status", serde_json::json!({}), move |app| {
        app.manage(lifecycle);
        app.manage(knobas_app::Profile::from_args(
            vec![knobas_app::DEMO_FLAG.to_owned()],
            std::path::Path::new("/tmp/knobas-test"),
        ));
    })
    .expect("app_status must answer with no database at all")
    .deserialize::<serde_json::Value>()
    .expect("an AppStatus came back");

    assert_eq!(
        status["db"],
        serde_json::json!({
            "state": "starting",
            "detail": "first run: downloading and initialising PostgreSQL",
        }),
        "the boot screen renders this verbatim"
    );
    assert_eq!(status["demo"], serde_json::json!(true));
    // Not "0 sources": nothing has been counted, and claiming a count nobody
    // took is how a first-run wizard fires at somebody's real corpus.
    assert_eq!(status["source_count"], serde_json::json!(0));
    assert_eq!(status["first_run"], serde_json::json!(false));
    assert!(
        status["app_version"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "the failure screen offers this line to copy: {status}"
    );
}

/// `frontend_ready` replays the current state onto `db:state`.
///
/// Gotcha 9: the backend cannot emit before the webview listens, so a
/// `db:state` fired during startup is lost unless something replays it. If
/// this command stopped emitting, a window that attached late would sit on
/// "starting" until its next poll -- which looks like a slow database and is
/// actually a lost event.
#[test]
fn frontend_ready_replays_the_current_state() {
    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let recorder = std::sync::Arc::clone(&seen);

    invoke_managing("frontend_ready", serde_json::json!({}), move |app| {
        let lifecycle = knobas_app::Lifecycle::new();
        lifecycle.set(knobas_app::DbState::Migrating);
        app.manage(lifecycle);
        app.listen("db:state", move |event| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                recorder.lock().unwrap().push(value);
            }
        });
    })
    .expect("frontend_ready must answer");

    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [serde_json::json!({ "state": "migrating" })],
        "exactly one replay of the state the lifecycle actually holds"
    );
}

// ---------------------------------------------------------------------------
// 7. The two link commands, through the real IPC pipeline (#52).
// ---------------------------------------------------------------------------

/// Both link commands are registered under the name the TypeScript mirror
/// invokes, and both argument shapes decode.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at run
/// time with "command not found" against a Rust side that compiles perfectly.
/// The argument names are the camelCase spellings Tauri renames to, which is
/// what `app/src/lib/ipc/entity.ts` sends -- so a misspelled key fails here as
/// an argument-resolution error rather than as `not_ready`.
///
/// `not_ready` is the marker for "dispatched, arguments resolved, body ran":
/// only a `Lifecycle` is managed, so `lifecycle.pool()` is the first thing that
/// can refuse.
#[test]
fn both_link_commands_are_registered_and_their_arguments_decode() {
    for (cmd, args) in [
        // `createLink` with everything, and with only the two ends -- the
        // "costs no extra decisions" call. An omitted `Option` argument
        // decodes as `None`, and the second case is what proves it: a
        // `relation` declared as `String` would be refused by name here.
        (
            "create_link",
            serde_json::json!({
                "fromId": "mock:PAY-231", "toId": "mock:PAY-228",
                "relation": "documents", "note": "why"
            }),
        ),
        (
            "create_link",
            serde_json::json!({ "fromId": "mock:PAY-231", "toId": "mock:PAY-228" }),
        ),
        (
            "unlink",
            serde_json::json!({ "linkId": "00000000-0000-0000-0000-000000000000" }),
        ),
    ] {
        let rejection = invoke_managing(cmd, args.clone(), |app| {
            app.manage(knobas_app::Lifecycle::new());
        })
        .expect_err("there is no pool yet");

        assert!(
            rejection.contains("not_ready"),
            "{cmd} was not dispatched, or {args} does not decode: {rejection}"
        );
        assert!(
            !rejection.contains(TAURI_STATE_REFUSAL),
            "{cmd} declares managed state as an argument -- carry-over §10.6(a): \
             {rejection}"
        );
    }

    // The control: without it the loop above would pass just as happily
    // against a harness that answered `not_ready` to anything at all. A
    // required argument left out is refused *by name*, before any body runs.
    let omitted = invoke_managing(
        "create_link",
        serde_json::json!({ "fromId": "mock:PAY-231" }),
        |app| {
            app.manage(knobas_app::Lifecycle::new());
        },
    )
    .expect_err("`toId` is not optional");
    assert!(
        omitted.contains("toId") || omitted.contains("to_id"),
        "a missing endpoint must be refused by name: {omitted}"
    );
    assert!(
        !omitted.contains("not_ready"),
        "the body ran despite an incomplete argument list: {omitted}"
    );
}

/// `create_link` puts its activity line on `activity:new`, and the payload is
/// the row that was written.
///
/// The event is what makes the status bar's "latest change" tick when the user
/// links something (story 19), and it is the half of the acceptance criterion
/// no store test can see: the command emits it *itself*, from the row the write
/// handed back, rather than re-reading the log the way the scheduler does.
///
/// Driven through the whole pipeline -- a mock app, a real migrated pool, a
/// listener -- so what is asserted is what a window would receive.
#[tokio::test(flavor = "multi_thread")]
async fn creating_a_link_announces_its_activity_line_on_the_event() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    // Two entities of this test's own: the database is shared by every test in
    // this binary, and a link is refused if either end has no entity row.
    let run = uuid::Uuid::new_v4();
    let from = format!("links-{run}:TICKET-1");
    let to = format!("links-{run}:PAGE-1");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let recorder = std::sync::Arc::clone(&seen);
    let pool_for_state = pool.clone();
    let (from_arg, to_arg) = (from.clone(), to.clone());

    let created = invoke_managing(
        "create_link",
        serde_json::json!({ "fromId": from_arg, "toId": to_arg, "relation": "documents" }),
        move |app| {
            app.manage(ready_over(pool_for_state));
            app.listen("activity:new", move |event| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    recorder.lock().unwrap().push(value);
                }
            });
        },
    )
    .expect("create_link must answer")
    .deserialize::<serde_json::Value>()
    .expect("a LinkRow came back");

    assert_eq!(created["from_id"], serde_json::json!(from));
    assert_eq!(created["to_id"], serde_json::json!(to));
    assert_eq!(created["origin"], serde_json::json!("manual"));

    // Emitted before the command returns, so it is already there -- no polling,
    // and therefore no way for this to pass by waiting long enough.
    let lines = seen.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "one line per mutation: {lines:?}");
    let line = &lines[0];
    assert_eq!(line["verb"], serde_json::json!("linked"));
    assert_eq!(line["actor"], serde_json::json!("user"));
    assert_eq!(
        line["entity_id"],
        serde_json::json!(from),
        "the line is named on the end the link was drawn from"
    );
    assert_eq!(line["detail"]["to_id"], serde_json::json!(to));
    assert_eq!(line["detail"]["relation"], serde_json::json!("documents"));
    assert_eq!(line["detail"]["link_id"], created["id"]);

    // The announced row is the row in the log, not one the command invented:
    // `id` and `at` are the database's to choose.
    let stored: (String, serde_json::Value) =
        sqlx::query_as("select verb, detail from knobas.activity where id = $1 and entity_id = $2")
            .bind(line["id"].as_i64().expect("the line carries its id"))
            .bind(&from)
            .fetch_one(&pool)
            .await
            .expect("the announced line is in the log");
    assert_eq!(stored.0, "linked");
    assert_eq!(stored.1, line["detail"]);
}

/// `unlink` announces its own line, and an already-withdrawn link announces
/// nothing.
///
/// The sibling above covers `linked` only, which left the other half of the
/// acceptance criterion -- "each mutation ... emits the activity event" --
/// resting on nobody: `unlink`'s `announce` could be deleted outright and the
/// whole `knobas-app` suite still passed. The second half is the one the
/// `Option` the store hands back exists for: an unlink that withdrew nothing is
/// not a mutation, so it writes no line and emits none.
#[tokio::test(flavor = "multi_thread")]
async fn unlinking_announces_its_line_and_an_already_withdrawn_link_announces_nothing() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    // Two entities of this test's own: the database is shared by every test in
    // this binary, and a link is refused if either end has no entity row.
    let run = uuid::Uuid::new_v4();
    let from = format!("unlinks-{run}:TICKET-1");
    let to = format!("unlinks-{run}:PAGE-1");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let created = {
        let pool_for_state = pool.clone();
        invoke_managing(
            "create_link",
            serde_json::json!({ "fromId": &from, "toId": &to, "relation": "blocks" }),
            move |app| {
                app.manage(ready_over(pool_for_state));
            },
        )
        .expect("create_link must answer")
        .deserialize::<serde_json::Value>()
        .expect("a LinkRow came back")
    };
    let link_id = created["id"].as_str().expect("the link carries its id");

    /// Invoke `unlink` over the bridge and collect every `activity:new` a
    /// window would have received.
    fn unlink_watching(pool: &sqlx::PgPool, link_id: &str) -> Vec<serde_json::Value> {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
        let recorder = std::sync::Arc::clone(&seen);
        let pool_for_state = pool.clone();
        invoke_managing(
            "unlink",
            serde_json::json!({ "linkId": link_id }),
            move |app| {
                app.manage(ready_over(pool_for_state));
                app.listen("activity:new", move |event| {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                        recorder.lock().unwrap().push(value);
                    }
                });
            },
        )
        .expect("unlink must answer");
        // Emitted before the command returns, so it is already there -- no
        // polling, and therefore no way for this to pass by waiting.
        seen.lock().unwrap().clone()
    }

    let announced = unlink_watching(&pool, link_id);
    assert_eq!(announced.len(), 1, "one line per mutation: {announced:?}");
    let line = &announced[0];
    assert_eq!(line["verb"], serde_json::json!("unlinked"));
    assert_eq!(line["actor"], serde_json::json!("user"));
    assert_eq!(
        line["entity_id"],
        serde_json::json!(from),
        "the line is named on the end the link was drawn from"
    );
    assert_eq!(line["detail"]["to_id"], serde_json::json!(to));
    assert_eq!(line["detail"]["relation"], serde_json::json!("blocks"));
    assert_eq!(line["detail"]["link_id"], created["id"]);

    // The announced row is the row in the log, not one the command invented.
    let stored: (String, serde_json::Value) =
        sqlx::query_as("select verb, detail from knobas.activity where id = $1 and entity_id = $2")
            .bind(line["id"].as_i64().expect("the line carries its id"))
            .bind(&from)
            .fetch_one(&pool)
            .await
            .expect("the announced line is in the log");
    assert_eq!(stored.0, "unlinked");
    assert_eq!(stored.1, line["detail"]);

    // Withdrawing it again mutates nothing, so it announces nothing.
    assert!(
        unlink_watching(&pool, link_id).is_empty(),
        "an already-withdrawn link is not a mutation and must announce nothing"
    );
    let (lines_written,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.activity where entity_id = $1 and verb = 'unlinked'",
    )
    .bind(&from)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lines_written, 1, "the second unlink wrote a second line");
}
