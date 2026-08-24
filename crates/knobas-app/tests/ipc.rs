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
//! in declaration order, and both real commands take `State<'_, AppState>`
//! first. A mock app manages no `AppState`, so every call to them stops there
//! -- before `progress` is looked at. (2) therefore uses commands of the same
//! two argument shapes with no state in front of them, which is the only way
//! to watch the decoding itself happen.
//!
//! A fourth section rides along, for a different contract: ruling P13's demo
//! guard. Argument resolution runs to completion *before* any command body, so
//! reaching the guard means managing both the `Profile` and an `AppState` --
//! the latter over a pool pointing at a closed port, so that "the guard
//! refused" and "the database was touched" cannot be confused for one another.

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
            knobas_app::commands::sources::demo_load,
            knobas_app::commands::sources::sync_now,
            knobas_app::commands::sources::sync_now_with_progress,
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

/// What a rejection from *inside* a command looks like.
///
/// A mock app manages no `AppState`, so a call that Tauri accepted and
/// dispatched gets exactly this far. It is therefore the marker for "this
/// command is registered and was reached", as distinct from "no such command"
/// or "the ACL refused it".
const REACHED_THE_BODY: &str = "state not managed";

/// Both halves of the split are registered under the names the TypeScript
/// mirror invokes, and both dispatch.
///
/// Not a decoding test -- `State<'_, AppState>` is the first argument of each,
/// so neither call gets past it -- but the one that catches the mistake a
/// two-command surface invites: adding the command and forgetting the handler
/// list, which is a frontend that fails at runtime with "command not found"
/// and a Rust side that compiles perfectly.
#[test]
fn both_halves_of_sync_now_are_registered_and_reachable() {
    for cmd in ["sync_now", "sync_now_with_progress"] {
        let rejection = invoke(cmd, serde_json::json!({ "sourceId": "mock" }))
            .expect_err("no AppState is managed");
        assert!(
            rejection.contains(REACHED_THE_BODY),
            "{cmd} was not dispatched: {rejection}"
        );
    }
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

/// Invoke `demo_load` with `profile` and a database nothing is listening on.
fn invoke_demo_load(profile: knobas_app::Profile) -> Result<(), String> {
    invoke_managing("demo_load", serde_json::json!({}), move |app| {
        app.manage(profile);
        app.manage(knobas_app::AppState::over_pool(unreachable_pool()));
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
        !rejection.contains(REACHED_THE_BODY),
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
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // Registers `mock` in `source_config`, which is what `prepare_sync`
    // requires. The sync it performs is incidental.
    knobas_app::demo::demo_load_inner(&pool).await.unwrap();

    let seen: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
    let recorder = std::sync::Arc::clone(&seen);

    let pool_for_state = pool.clone();
    let returned = invoke_managing(
        "sync_now",
        serde_json::json!({ "sourceId": "mock" }),
        move |app| {
            app.manage(knobas_app::AppState::over_pool(pool_for_state));
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

    // Emitted before the spawn, so it is already there.
    let started = seen.lock().unwrap().first().cloned().expect(
        "a `running` sync:state must be emitted before sync_now returns -- \
         otherwise EVENTS.syncState is a promise the docs make and nothing keeps",
    );
    assert_eq!(started["running"], serde_json::json!(true));
    assert_eq!(started["run_id"], serde_json::json!(returned));
    assert_eq!(started["source_id"], serde_json::json!("mock"));

    // The run itself lands on its own task; wait for the log row to close.
    let mut outcome = None;
    for _ in 0..100 {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "select outcome from knobas.sync_run where id = $1 and finished_at is not null",
        )
        .bind(returned)
        .fetch_optional(&pool)
        .await
        .unwrap();
        if let Some((Some(value),)) = row {
            outcome = Some(value);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        outcome.as_deref(),
        Some("ok"),
        "the spawned run must close its own log row"
    );

    let terminal = seen
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|status| status["running"] == serde_json::json!(false))
        .cloned()
        .expect("a terminal sync:state must follow the run");
    assert_eq!(terminal["run_id"], serde_json::json!(returned));
    assert_eq!(terminal["last_outcome"], serde_json::json!("ok"));
}
