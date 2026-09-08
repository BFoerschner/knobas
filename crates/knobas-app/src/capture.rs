//! The capture window and its global shortcut (issue #503, spec #491 stories
//! 40--43).
//!
//! `CONTEXT.md`, **Capture**: *"a note made from the global shortcut while some
//! other window has the focus, in a small window of its own, created on the
//! first keystroke and never on an empty one."* This module is the operating
//! system's half of that sentence -- the shortcut, its setting, the window --
//! and the note itself is `commands::entity::create_note` (#502), called by the
//! capture window with the two links this module's [`Recorded`] pair supplies.
//!
//! # Three things live here and each is here for its own reason
//!
//! 1. **The shortcut setting.** One `knobas.setting` row
//!    ([`SHORTCUT_KEY`]), **empty by default**, so a fresh install registers
//!    nothing at all until somebody chooses a key. That is the ticket's first
//!    criterion and it is a deliberate refusal to guess: a global shortcut is
//!    the one preference an application can take away from another
//!    application, and knobas takes none until asked.
//!
//! 2. **Whether the operating system took it.** A stored accelerator is not a
//!    registered one -- another app may hold the combination, and it may hold
//!    it only on some days. So the refusal is *state*, not an exception:
//!    [`set_shortcut`] stores what the reader asked for and answers a
//!    [`ShortcutView`] carrying why it is not active, and [`register_stored`]
//!    records the same thing at bring-up, where there is nobody to hand an
//!    error to. The settings section draws that one field either way, which is
//!    what the ticket means by *"a shortcut the OS refuses to register is
//!    reported in settings"*. **The alternative was refusing the write** the
//!    way `set_checkout_command` refuses an unusable template; it is rejected
//!    because it can only ever cover half the population -- a combination
//!    Alfred grabbed after it was stored has no write to refuse -- and two
//!    mechanisms for one sentence is how a settings pane comes to have two
//!    ways of saying the same thing.
//!
//! 3. **What the main window last recorded.** The capture window is a webview
//!    of its own with no shell in it, so it does not know which room the reader
//!    was standing in or what was in front of them; the main window does, and
//!    it pushes both here whenever they change ([`record`]). What the capture
//!    then draws its links from is [`context`], and the two ids are exactly the
//!    two the in-app *New note* passes -- `captured-in` for the **context** of
//!    the last stored room (a derived room has none, so nothing then) and
//!    `captured-from` for the **foreground**, the word as `CONTEXT.md`'s
//!    **Passive attribution** defines it and as `shell/timer.ts`'s
//!    `roomForeground` spells it. Nothing here recomputes either of them: this
//!    module is a letterbox, and a second opinion about what the reader was
//!    looking at is exactly the divergence the deputy's ruling of 2026-09-08 on
//!    #502 refused to create.
//!
//! # The plugin is registered from Rust and the webview never calls it
//!
//! `tauri-plugin-global-shortcut` is driven entirely from here: bring-up
//! registers what is stored, [`set_shortcut`] re-registers on every change, and
//! the plugin's four commands (`register`, `unregister`, `unregister_all`,
//! `is_registered`) are reachable from no window, because no capability grants
//! them. That is why there is no `@tauri-apps/plugin-global-shortcut` in
//! `app/package.json` and no `global-shortcut:*` line in
//! `capabilities/default.json`. The one capability this feature does add is the
//! **capture window's own** (`capabilities/capture.json`), and it grants one
//! permission: `core:window:allow-close`, because a window with no decorations
//! has no other way to shut itself.

use std::str::FromStr;
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use sqlx::PgPool;
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::Shortcut;

use crate::IpcError;
use crate::settings;

/// The `knobas.setting` key holding the capture shortcut.
///
/// In `knobas.setting` and not in a column of its own, for the clones root's
/// reason (migration `0002`, comment 6): one accelerator a person types once is
/// not a relation. **No migration is owed by this feature**, and
/// `the_shortcut_is_stored_under_its_own_key` pins the spelling.
pub const SHORTCUT_KEY: &str = "capture.shortcut";

/// The label of the capture window, and the label
/// `capabilities/capture.json` scopes its one permission to.
pub const WINDOW_LABEL: &str = "capture";

/// The label of the window the *Open in knobas* button brings forward -- the
/// one `tauri.conf.json` declares.
const MAIN_LABEL: &str = "main";

/// The document the capture window loads.
///
/// Its **own entry point**, not a route inside the shell. `capture.html` mounts
/// one component and nothing else: no context switcher, no health
/// subscription, no timer, no inbox poll. The shortcut's whole promise is that
/// a thought costs one keystroke, and booting the application's entire frontend
/// over somebody else's window to write two lines is not that.
const WINDOW_URL: &str = "capture.html";

/// What the main window last told this module about where the reader is.
///
/// Both halves are optional and their absences mean different things, which is
/// why this is a pair of `Option`s rather than one `Option` of a pair: a
/// **derived** room -- *All work*, a source, a project -- has no context at
/// all, and a reader browsing with nothing open has no foreground, and either
/// can be true without the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Recorded {
    /// The `ctx:` entity of the last **stored** room, or `None`.
    pub context: Option<String>,
    /// The foreground entity, or `None`.
    pub foreground: Option<String>,
}

/// The shortcut as the settings pane draws it.
///
/// Two fields and three states, and the third is the one this shape exists for:
/// nothing stored (`accelerator: None`), stored and registered
/// (`refusal: None`), and stored but **not** registered, which is an
/// accelerator and a sentence saying why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShortcutView {
    /// What is stored, or `None` when nothing is -- the default, in which case
    /// no shortcut is registered and knobas has taken no key from anybody.
    pub accelerator: Option<String>,
    /// Why the stored accelerator is not registered, or `None` when it is (or
    /// when there is nothing to register).
    pub refusal: Option<String>,
}

/// The process-wide capture state: what the main window recorded, and why the
/// stored shortcut is not registered.
///
/// Managed by Tauri like [`Lifecycle`](crate::commands::app::Lifecycle), and
/// **in memory rather than in the database on purpose**. Both fields are facts
/// about *this run*: a room the reader stood in last Tuesday is not where they
/// are now, and a refusal is the answer the window server gave this process.
/// Storing either would make a stale one survive a restart, which is the one
/// direction that cannot be noticed.
#[derive(Debug, Default)]
pub struct Capture {
    recorded: Mutex<Recorded>,
    refusal: Mutex<Option<String>>,
}

impl Capture {
    /// A capture state that has been told nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn take_refusal(&self) -> Option<String> {
        self.refusal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_refusal(&self, refusal: Option<String>) {
        *self.refusal.lock().unwrap_or_else(PoisonError::into_inner) = refusal;
    }
}

/// What this module does to the operating system, behind a trait.
///
/// The seam, and it is here so that the two decisions above it -- *what is
/// stored* and *what is reported* -- can be driven by a test with no window
/// server, no event loop and no keyboard. `tests/capture_ipc.rs` registers a
/// stand-in that refuses one named accelerator, which is the ticket's *"IPC-seam
/// test with a shortcut the plugin refuses"*: the assertion is about the answer
/// the settings pane gets, and that answer must not depend on which application
/// happened to hold ⌘⇧N on the machine the gate ran on.
pub trait Registrar: Send + Sync {
    /// Make `accelerator` the one shortcut this application holds, replacing
    /// whatever it held before. `Err` is the refusal as a sentence a person
    /// reads -- a combination the plugin cannot parse, or one the operating
    /// system will not give up.
    ///
    /// # Errors
    ///
    /// The refusal, verbatim from the plugin.
    fn register(&self, accelerator: &str) -> Result<(), String>;

    /// Hold no shortcut at all.
    ///
    /// # Errors
    ///
    /// The refusal, verbatim from the plugin.
    fn clear(&self) -> Result<(), String>;
}

/// The real registrar: the plugin, over one application handle.
pub struct Plugin<R: Runtime>(tauri::AppHandle<R>);

impl<R: Runtime> Plugin<R> {
    /// The registrar a command builds from the handle Tauri injected.
    #[must_use]
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self(app)
    }
}

impl<R: Runtime> Plugin<R> {
    /// The plugin's state, or a sentence saying it is not there.
    ///
    /// `try_state` and not `state`, which **panics** when nothing is managed. A
    /// panic inside a `#[tauri::command]` is a dead window, and the one caller
    /// that can meet the absence -- a mock application built without the plugin
    /// -- deserves the same answer as any other refusal rather than a crash.
    /// In a real knobas the plugin is registered in `run()` before any window
    /// exists, so this arm is unreachable there.
    fn shortcuts(
        &self,
    ) -> Result<tauri::State<'_, tauri_plugin_global_shortcut::GlobalShortcut<R>>, String> {
        self.0
            .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<R>>()
            .ok_or_else(|| "the global-shortcut plugin is not running".to_owned())
    }
}

impl<R: Runtime> Registrar for Plugin<R> {
    fn register(&self, accelerator: &str) -> Result<(), String> {
        // Cleared **before** the parse, and that order is the point: what is
        // stored is now this accelerator, so the previous one has to stop
        // working even when this one turns out to be unusable. Registering
        // first and clearing on failure would leave a key that fires for a
        // shortcut the settings pane no longer shows.
        self.clear()?;
        let shortcut = Shortcut::from_str(accelerator).map_err(|refusal| refusal.to_string())?;
        self.shortcuts()?
            .register(shortcut)
            .map_err(|refusal| refusal.to_string())
    }

    fn clear(&self) -> Result<(), String> {
        self.shortcuts()?
            .unregister_all()
            .map_err(|refusal| refusal.to_string())
    }
}

/// The stored accelerator and whether it is holding.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn shortcut(pool: &PgPool, capture: &Capture) -> Result<ShortcutView, IpcError> {
    let accelerator = settings::read(pool, SHORTCUT_KEY).await?;
    Ok(ShortcutView {
        // The refusal is reported only beside an accelerator. A stored
        // shortcut that was cleared while a refusal was on record would
        // otherwise draw "not registered" under an empty field.
        refusal: accelerator.as_ref().and_then(|_| capture.take_refusal()),
        accelerator,
    })
}

/// Store the capture shortcut, or clear it with a blank one, and register what
/// was stored.
///
/// Answers the fresh view rather than nothing, which is `set_checkout_command`'s
/// rule and for its reason: the field draws what is **stored**, so a write that
/// normalised or could not register its value says so in the same round trip.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails. A shortcut
/// the plugin or the operating system refuses is **not** an error -- see this
/// module's docs, point 2.
pub async fn set_shortcut(
    pool: &PgPool,
    capture: &Capture,
    registrar: &dyn Registrar,
    accelerator: Option<&str>,
) -> Result<ShortcutView, IpcError> {
    let accelerator = settings::settable(accelerator);
    settings::write(pool, SHORTCUT_KEY, accelerator).await?;
    capture.set_refusal(apply(registrar, accelerator));
    shortcut(pool, capture).await
}

/// Register whatever is stored, at bring-up.
///
/// Called once the database is up and off the main thread, because the plugin
/// hands its work to the main thread and waits for it: calling this from
/// `setup` would be the event loop waiting on itself.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails. A refusal is
/// recorded, not returned -- there is nobody to hand it to at bring-up, and the
/// settings pane is where it is read.
pub async fn register_stored(
    pool: &PgPool,
    capture: &Capture,
    registrar: &dyn Registrar,
) -> Result<(), IpcError> {
    let accelerator = settings::read(pool, SHORTCUT_KEY).await?;
    capture.set_refusal(apply(registrar, accelerator.as_deref()));
    Ok(())
}

/// Ask the registrar for `accelerator`, and answer the refusal if there is one.
///
/// One function because the two callers above must not be able to disagree
/// about what *stored but not registered* means.
fn apply(registrar: &dyn Registrar, accelerator: Option<&str>) -> Option<String> {
    match accelerator {
        Some(accelerator) => registrar.register(accelerator).err(),
        // A cleared setting is a cleared registration, and a registrar that
        // could not let go is worth saying so about: the reader would otherwise
        // have an empty field and a key that still fires.
        None => registrar.clear().err(),
    }
}

/// Record where the main window is, so a capture can attach it.
pub fn record(capture: &Capture, recorded: Recorded) {
    *capture
        .recorded
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = recorded;
}

/// What the main window last recorded.
#[must_use]
pub fn context(capture: &Capture) -> Recorded {
    capture
        .recorded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Show the capture window: build it if it is not there, focus it if it is.
///
/// Called from the plugin's shortcut handler, which macOS delivers on the main
/// thread.
///
/// **Built fresh rather than hidden and shown.** A capture is one thought and
/// the window ends with it, so a rebuilt window is an empty editor by
/// construction rather than by remembering to clear one -- and a window that
/// was merely hidden would come back holding whatever the last capture typed
/// into it after an error stopped it being saved. The cost is the webview's
/// start-up on each press, which is one small document.
///
/// A failure to build is logged and swallowed: the caller is a keystroke, there
/// is no window to report into, and a panic inside the plugin's handler would
/// take the event loop with it.
pub fn open_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(failure) = existing.set_focus() {
            tracing::warn!(%failure, "the capture window would not take focus");
        }
        return;
    }
    let built = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App(WINDOW_URL.into()))
        .title("Capture")
        .inner_size(560.0, 260.0)
        .resizable(false)
        .always_on_top(true)
        // No title bar: this is an overlay over somebody else's window, and
        // there is nothing on a title bar it has a use for. Escape and ⌘Enter
        // are what close it, which is why the window needs
        // `core:window:allow-close` and why that is the only permission it has.
        .decorations(false)
        .skip_taskbar(true)
        .center()
        .focused(true)
        .build();
    if let Err(failure) = built {
        tracing::error!(%failure, "the capture window would not open");
    }
}

/// Bring the main window forward and tell it which note to open.
///
/// The capture window's *Open in knobas* button. It emits rather than
/// navigating, because the address is the shell's to build and the shell is in
/// the other window: `App.svelte` listens for
/// [`CAPTURE_OPEN_NOTE`](crate::events::CAPTURE_OPEN_NOTE) and routes.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if there is no main window --
/// which on macOS is a reader who closed it and left the application running;
/// [`Internal`](crate::IpcErrorCode::Internal) if the window will not come
/// forward or the event will not send.
pub fn reveal_note<R: Runtime>(app: &tauri::AppHandle<R>, note_id: &str) -> Result<(), IpcError> {
    let main = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| IpcError::not_found("the knobas window is not open"))?;
    main.unminimize().map_err(IpcError::internal)?;
    main.show().map_err(IpcError::internal)?;
    main.set_focus().map_err(IpcError::internal)?;
    app.emit_to(MAIN_LABEL, crate::events::CAPTURE_OPEN_NOTE, note_id)
        .map_err(IpcError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key the module's own docs name, spelled once.
    ///
    /// The pin is cheap and the failure it catches is not: a renamed key is a
    /// shortcut that silently stops being read, on a machine where the row is
    /// still in the table.
    #[test]
    fn the_shortcut_is_stored_under_its_own_key() {
        assert_eq!(SHORTCUT_KEY, "capture.shortcut");
        assert!(
            SHORTCUT_KEY.starts_with("capture."),
            "settings keys are namespaced by the feature that owns them"
        );
    }

    /// The document the window loads is the second entry point, not the shell.
    ///
    /// `app/vite.config.ts` has to build it and `app/capture.html` has to
    /// exist; a mismatch here is a window that loads a 404 and shows a blank
    /// rectangle over somebody's screen.
    #[test]
    fn the_capture_window_loads_its_own_document() {
        assert_eq!(WINDOW_URL, "capture.html");
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../app/");
        assert!(
            std::path::Path::new(&format!("{root}{WINDOW_URL}")).exists(),
            "app/{WINDOW_URL} is what the capture window loads and it is not there"
        );
        let vite = include_str!("../../../app/vite.config.ts");
        assert!(
            vite.contains(WINDOW_URL),
            "app/vite.config.ts does not name {WINDOW_URL}, so a bundle would not contain it"
        );
    }

    /// A stand-in registrar: it records what it was asked for, and refuses
    /// anything whose accelerator is in its refusal list.
    #[derive(Default)]
    struct Stub {
        refuse: Option<String>,
        calls: Mutex<Vec<String>>,
    }

    impl Registrar for Stub {
        fn register(&self, accelerator: &str) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("register {accelerator}"));
            match &self.refuse {
                Some(refused) if refused == accelerator => {
                    Err(format!("{accelerator} is already taken"))
                }
                _ => Ok(()),
            }
        }

        fn clear(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("clear".to_owned());
            Ok(())
        }
    }

    /// Clearing the setting clears the registration, rather than leaving a key
    /// that fires for a shortcut the pane no longer shows.
    #[test]
    fn nothing_stored_registers_nothing() {
        let stub = Stub::default();
        assert_eq!(apply(&stub, None), None);
        assert_eq!(stub.calls.lock().unwrap().as_slice(), ["clear"]);
    }

    /// The refusal is the plugin's own sentence, carried through rather than
    /// replaced by one of this module's: the reader needs to know *which*
    /// refusal it was.
    #[test]
    fn a_refused_accelerator_answers_the_refusal() {
        let stub = Stub {
            refuse: Some("CmdOrCtrl+Space".to_owned()),
            ..Stub::default()
        };
        assert_eq!(
            apply(&stub, Some("CmdOrCtrl+Space")),
            Some("CmdOrCtrl+Space is already taken".to_owned())
        );
        assert_eq!(apply(&stub, Some("CmdOrCtrl+Shift+N")), None);
    }

    /// The plugin's parser is what decides whether a string is an accelerator
    /// at all, and it is reachable without a window server.
    ///
    /// Asserted here rather than trusted, because the settings field is free
    /// text and this is the half of the refusal a test on any machine can see:
    /// the other half -- a combination another application holds -- depends on
    /// what is installed on the machine the gate ran on.
    #[test]
    fn the_plugin_parses_an_accelerator_and_refuses_a_string_that_is_not_one() {
        assert!(Shortcut::from_str("CmdOrCtrl+Shift+N").is_ok());
        assert!(Shortcut::from_str("Alt+Space").is_ok());
        assert!(Shortcut::from_str("Cheese").is_err());
        assert!(Shortcut::from_str("CmdOrCtrl+").is_err());
        assert!(Shortcut::from_str("").is_err());
    }

    /// The recorded pair is a letterbox: what goes in comes out, both halves
    /// independently absent.
    #[test]
    fn what_the_main_window_recorded_is_what_comes_back() {
        let capture = Capture::new();
        assert_eq!(context(&capture), Recorded::default());

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

        // A derived room: no context, and a foreground all the same.
        record(
            &capture,
            Recorded {
                context: None,
                foreground: Some("mock:PAY-231".to_owned()),
            },
        );
        assert_eq!(
            context(&capture),
            Recorded {
                context: None,
                foreground: Some("mock:PAY-231".to_owned()),
            }
        );
    }
}
