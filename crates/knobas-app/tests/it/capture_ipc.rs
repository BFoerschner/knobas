//! The capture surface as the interface sees it (#503).
//!
//! Three joins live here, and each of them is invisible from either side alone:
//! the shortcut round-tripping through `knobas.setting`, the refusal that is
//! **state** rather than an exception, and the pair the main window records for
//! the capture window to read.
//!
//! # What a stand-in registrar can witness, and what it cannot
//!
//! `knobas_app::capture::Registrar` is the seam between the two decisions this
//! module makes -- *what is stored* and *what is reported* -- and the one act
//! that needs a window server. The stand-in below refuses one named
//! accelerator, so the ticket's *"IPC-seam test with a shortcut the plugin
//! refuses"* is an assertion about **the answer the settings pane gets**, and
//! not about which application happens to hold ⌘⇧N on the machine the gate ran
//! on.
//!
//! What it therefore does **not** witness, said plainly rather than left to be
//! discovered: that `capture::Plugin` clears the old registration before
//! parsing the new one, that a real refusal from the operating system reads as
//! a sentence worth showing, and that a registered accelerator opens anything.
//! Those are the desktop witness's (`testenv/desktop-witness/drivers/capture.sh`)
//! and are owed to #525 with the rest of that harness's first green run.
//!
//! # Why every test gets a database of its own
//!
//! The shortcut is **one `knobas.setting` row per database** and every test
//! here either sets it or asserts what it is -- `checkout_ipc.rs`'s reason,
//! measured there: a global setting cannot be namespaced by a fixture id, so
//! sharing this file's database would put the tests in each other's setting.

use std::sync::Mutex;

use knobas_app::capture::{
    CaptureState, Recorded, Registrar, ShortcutView, context, record, register_stored,
    set_shortcut, shortcut,
};
use sqlx::PgPool;

async fn pool() -> PgPool {
    knobas_db::test_util::scratch_database("capture-ipc")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

/// A registrar that records what it was asked to do and refuses one named
/// accelerator.
///
/// The refusal is a **sentence**, because that is what the real one is: the
/// plugin's `Error` renders a parse failure and a window server's *hotkey
/// already registered* as text, and the settings pane shows whichever it was.
#[derive(Default)]
struct Stub {
    refuse: Option<&'static str>,
    calls: Mutex<Vec<String>>,
}

impl Stub {
    fn refusing(accelerator: &'static str) -> Self {
        Self {
            refuse: Some(accelerator),
            calls: Mutex::default(),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("the stub's log").clone()
    }
}

impl Registrar for Stub {
    fn register(&self, accelerator: &str) -> Result<(), String> {
        self.calls
            .lock()
            .expect("the stub's log")
            .push(format!("register {accelerator}"));
        if self.refuse == Some(accelerator) {
            return Err(format!(
                "{accelerator} is registered by another application"
            ));
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        self.calls
            .lock()
            .expect("the stub's log")
            .push("clear".to_owned());
        Ok(())
    }
}

const SHORTCUT: &str = "CmdOrCtrl+Shift+N";
const OTHER: &str = "Alt+Space";

/// A fresh knobas holds no key at all.
///
/// The ticket's first criterion, and the assertion is on **both** halves: the
/// setting is empty *and* the registrar was asked for nothing but a clear. A
/// default that lived only in the settings pane would be a shortcut registered
/// by bring-up that no field ever showed.
#[tokio::test]
async fn nothing_is_registered_until_a_shortcut_is_chosen() {
    let pool = pool().await;
    let capture = CaptureState::new();
    let stub = Stub::default();

    register_stored(&pool, &capture, &stub)
        .await
        .expect("bring-up reads the setting");

    assert_eq!(
        shortcut(&pool, &capture).await.expect("the view"),
        ShortcutView {
            accelerator: None,
            refusal: None,
        },
        "a knobas nobody has configured has no shortcut and nothing to explain"
    );
    assert_eq!(
        stub.calls(),
        ["clear"],
        "bring-up asked the operating system for no key"
    );
}

/// What is stored is what comes back, and a restart registers it again.
#[tokio::test]
async fn a_stored_shortcut_is_registered_now_and_again_at_the_next_start() {
    let pool = pool().await;
    let capture = CaptureState::new();
    let stub = Stub::default();

    let answered = set_shortcut(&pool, &capture, &stub, Some(SHORTCUT))
        .await
        .expect("the write");
    assert_eq!(
        answered,
        ShortcutView {
            accelerator: Some(SHORTCUT.to_owned()),
            refusal: None,
        }
    );
    assert_eq!(stub.calls(), [format!("register {SHORTCUT}")]);

    // A second process, over the same database: the setting is the only thing
    // that survived, and it is enough.
    let restarted = CaptureState::new();
    let after = Stub::default();
    register_stored(&pool, &restarted, &after)
        .await
        .expect("bring-up");
    assert_eq!(after.calls(), [format!("register {SHORTCUT}")]);
    assert_eq!(
        shortcut(&pool, &restarted).await.expect("the view"),
        ShortcutView {
            accelerator: Some(SHORTCUT.to_owned()),
            refusal: None,
        }
    );
}

/// A shortcut the plugin refuses is **stored**, and the answer says why it is
/// not holding -- the ticket's *"a refused registration is shown in settings"*.
///
/// Two assertions, and the second is the one that makes this a report rather
/// than a rejection: the refusal is still there on a **later read**, so a
/// settings pane opened an hour after the write shows it. A refusal that lived
/// only in the return value of the write would be gone by the time anybody
/// looked, which is the whole reason this is state.
#[tokio::test]
async fn a_refused_shortcut_is_stored_and_the_answer_says_why() {
    let pool = pool().await;
    let capture = CaptureState::new();
    let stub = Stub::refusing(SHORTCUT);

    let answered = set_shortcut(&pool, &capture, &stub, Some(SHORTCUT))
        .await
        .expect("a refused registration is not a refused write");
    assert_eq!(
        answered,
        ShortcutView {
            accelerator: Some(SHORTCUT.to_owned()),
            refusal: Some(format!("{SHORTCUT} is registered by another application")),
        }
    );

    assert_eq!(
        shortcut(&pool, &capture).await.expect("a later read"),
        answered,
        "the pane is opened later than the write and has to see the same thing"
    );
}

/// A refusal belongs to the accelerator that earned it, and goes when it does.
///
/// Both directions in one test, because they are one rule: a stored shortcut
/// that registers clears the refusal, and clearing the field leaves nothing to
/// report under an empty box.
#[tokio::test]
async fn a_refusal_does_not_outlive_the_shortcut_that_earned_it() {
    let pool = pool().await;
    let capture = CaptureState::new();
    let stub = Stub::refusing(SHORTCUT);

    set_shortcut(&pool, &capture, &stub, Some(SHORTCUT))
        .await
        .expect("the refused write");

    let replaced = set_shortcut(&pool, &capture, &stub, Some(OTHER))
        .await
        .expect("the second write");
    assert_eq!(
        replaced,
        ShortcutView {
            accelerator: Some(OTHER.to_owned()),
            refusal: None,
        },
        "the new shortcut registered, so there is nothing left to explain"
    );

    let cleared = set_shortcut(&pool, &capture, &stub, Some("   "))
        .await
        .expect("a blank field clears");
    assert_eq!(
        cleared,
        ShortcutView {
            accelerator: None,
            refusal: None,
        },
        "a blank field is *forget this*, and knobas then holds no key"
    );
    assert_eq!(
        stub.calls(),
        [
            format!("register {SHORTCUT}"),
            format!("register {OTHER}"),
            "clear".to_owned()
        ],
        "clearing the setting cleared the registration, rather than leaving a key that fires"
    );
}

/// The pair the main window records is the pair the capture window reads.
///
/// The ticket's third criterion. Both halves are asserted **and so is their
/// independence**: a derived room records no context while still recording a
/// foreground, which is the case a single nullable pair would get wrong.
#[tokio::test]
async fn the_recorded_room_and_foreground_are_what_the_capture_reads() {
    let capture = CaptureState::new();

    assert_eq!(
        context(&capture),
        Recorded {
            context: None,
            foreground: None,
        },
        "before the main window has drawn anything there is nothing to attach"
    );

    // A stored room with a ticket open: both links.
    record(
        &capture,
        Recorded {
            context: Some("ctx:sepa".to_owned()),
            foreground: Some("mock:PAY-231".to_owned()),
        },
    );
    assert_eq!(
        context(&capture),
        Recorded {
            context: Some("ctx:sepa".to_owned()),
            foreground: Some("mock:PAY-231".to_owned()),
        }
    );

    // A **derived** room -- *All work*, a source, a project -- has no context
    // (`CONTEXT.md`, **Room**), and the reader is still looking at something.
    record(
        &capture,
        Recorded {
            context: None,
            foreground: Some("mock:PAY-198".to_owned()),
        },
    );
    assert_eq!(
        context(&capture),
        Recorded {
            context: None,
            foreground: Some("mock:PAY-198".to_owned()),
        },
        "a derived room records no context, and the foreground is unaffected by that"
    );

    // And a room with nothing open at all: a context and no foreground, the
    // other half of the same independence.
    record(
        &capture,
        Recorded {
            context: Some("ctx:sepa".to_owned()),
            foreground: None,
        },
    );
    assert_eq!(
        context(&capture),
        Recorded {
            context: Some("ctx:sepa".to_owned()),
            foreground: None,
        }
    );
}

/// What the capture window reads is what the **latest** record said.
///
/// Separate from the test above because it is a different claim: that this is a
/// letterbox and not an accumulator. A reader who moved from a stored room to
/// *All work* and then captured must not get the room they left -- the note
/// would say it belongs to a working set the reader had already walked out of.
#[tokio::test]
async fn a_later_record_replaces_the_earlier_one_whole() {
    let capture = CaptureState::new();
    record(
        &capture,
        Recorded {
            context: Some("ctx:sepa".to_owned()),
            foreground: Some("mock:PAY-231".to_owned()),
        },
    );
    record(
        &capture,
        Recorded {
            context: None,
            foreground: None,
        },
    );
    assert_eq!(
        context(&capture),
        Recorded {
            context: None,
            foreground: None,
        },
        "moving into a derived room with nothing open leaves nothing to attach, \
         and not half of what was there before"
    );
}

// ---------------------------------------------------------------------------
// The wiring: registered, named, and decoding.
// ---------------------------------------------------------------------------

use tauri::Manager;
use tauri::ipc::CallbackFn;

/// The origin a webview's own page has, which is what makes a call **local** --
/// and a remote origin is refused by the ACL before its arguments are looked
/// at, which would make every assertion below vacuous. Windows serves the app
/// over `http://tauri.localhost`; everywhere else it is a custom scheme.
#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// Invoke `cmd` on a mock app that manages a `Lifecycle` with no pool and a
/// `CaptureState` with nothing recorded.
///
/// The two that read the database -- `capture_shortcut` and
/// `set_capture_shortcut` -- answer `not_ready`, which is the marker for
/// *registered and dispatched* as distinct from *no such command*.
/// `record_capture_context` and `capture_context` take no pool and answer;
/// `reveal_note` takes none either and answers `not_found`, because a mock app
/// has no `main` window to bring forward. All five are dispatched, which is the
/// only thing the caller below reads out of the answer.
fn invoke(cmd: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::entity::capture_shortcut,
            knobas_app::commands::entity::set_capture_shortcut,
            knobas_app::commands::entity::record_capture_context,
            knobas_app::commands::entity::capture_context,
            knobas_app::commands::entity::reveal_note,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(knobas_app::Lifecycle::new());
    app.manage(CaptureState::new());
    let webview: tauri::WebviewWindow<tauri::test::MockRuntime> =
        tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
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
             so this proves nothing about decoding: {rejection}"
        );
    }
    outcome.and_then(|answered| {
        answered
            .deserialize::<serde_json::Value>()
            .map_err(|error| format!("the answer is not JSON: {error}"))
    })
}

/// Every capture command is registered under the name the mirror invokes, and
/// its argument list decodes.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at run
/// time with "command not found" against a Rust side that compiles.
///
/// Each optional argument appears **three times** -- present, null and absent
/// -- because an `Option` declared as a plain `String` would be refused by name
/// in the second and third calls and by nothing in the first.
#[test]
fn every_capture_command_is_registered_and_its_arguments_decode() {
    for (cmd, args) in [
        ("capture_shortcut", serde_json::json!({})),
        (
            "set_capture_shortcut",
            serde_json::json!({ "accelerator": SHORTCUT }),
        ),
        (
            "set_capture_shortcut",
            serde_json::json!({ "accelerator": null }),
        ),
        ("set_capture_shortcut", serde_json::json!({})),
        (
            "record_capture_context",
            serde_json::json!({ "context": "ctx:sepa", "foreground": "mock:PAY-231" }),
        ),
        (
            "record_capture_context",
            serde_json::json!({ "context": null, "foreground": null }),
        ),
        ("record_capture_context", serde_json::json!({})),
        ("capture_context", serde_json::json!({})),
        (
            "reveal_note",
            serde_json::json!({ "noteId": "note:0f2c1a" }),
        ),
    ] {
        let outcome = invoke(cmd, args.clone());
        let rejection = match &outcome {
            Ok(_) => String::new(),
            Err(rejection) => rejection.clone(),
        };
        assert!(
            !rejection.contains("not found") && !rejection.contains("invalid args"),
            "{cmd} {args} is not reachable through the IPC pipeline: {rejection}"
        );
    }
}

/// The two commands that need no database answer during bring-up, and they
/// answer each other.
///
/// `record_capture_context` and `capture_context` take no `Lifecycle` pool, and
/// that is deliberate: the capture window can be opened seconds after launch,
/// while PostgreSQL is still starting, and where the reader was standing is
/// true whether or not the database is up. The note itself will wait for it.
#[test]
fn the_recorded_pair_crosses_the_bridge_before_the_database_is_up() {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::entity::record_capture_context,
            knobas_app::commands::entity::capture_context,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(knobas_app::Lifecycle::new());
    app.manage(CaptureState::new());
    let webview: tauri::WebviewWindow<tauri::test::MockRuntime> =
        tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("mock webview");

    let call = |cmd: &str, body: serde_json::Value| {
        tauri::test::get_ipc_response(
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
        .map_err(|error| format!("{error:?}"))
        .and_then(|answered| {
            answered
                .deserialize::<serde_json::Value>()
                .map_err(|error| format!("the answer is not JSON: {error}"))
        })
    };

    call(
        "record_capture_context",
        serde_json::json!({ "context": "ctx:sepa", "foreground": "mock:PAY-231" }),
    )
    .expect("the record");
    assert_eq!(
        call("capture_context", serde_json::json!({})).expect("the read"),
        serde_json::json!({ "context": "ctx:sepa", "foreground": "mock:PAY-231" }),
        "the pair crosses the bridge under the field names the mirror declares"
    );
}
