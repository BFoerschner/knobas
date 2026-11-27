//! Desktop notifications with a click that arrives (#339, spec #272
//! "Notifications").
//!
//! #290 sent them through `tauri-plugin-notification`, whose desktop
//! `notify` hands the desktop notification to `notify-rust` and drops the
//! handle -- so nothing in the process could learn of a click, and the
//! plugin's `register_listener` exists on mobile only. This module owns the
//! send instead: `notify-rust` shows the desktop notification (on Windows,
//! WinRT itself: see *Platforms*), the wait runs on a thread of its own, and a click becomes the
//! `notification:clicked` event with the item's address, which the store on
//! the other side already knows how to navigate to. The decision to own the
//! click on `notify-rust` rather than on a second wrapper is
//! `docs/research/2026-09-04-notification-library-alternatives.md`; that the
//! wait fires inside a signed Tauri bundle from a worker thread was witnessed
//! by the #339 prototype (its verdict table is on the issue).
//!
//! ## The wait is unbounded, so there is a registry
//!
//! The prototype showed `wait_for_action` returning only on a click or on the
//! user clearing the banner from Notification Center -- **never** on
//! the banner sliding away (and the Windows wait reads past it on purpose,
//! see *Platforms*). Every wait is therefore a thread blocked for as
//! long as the reader ignores that banner, which could be the whole session,
//! and knobas neither invents a timeout (the OS gives none, and a made-up one
//! drops real clicks) nor lets the count grow without bound. Two rules:
//!
//! - **One waiter per address.** A second send for an address already waited
//!   on shows the desktop notification and starts no second wait: the click
//!   on either banner lands on the one thread, and it goes to the same room.
//!   Not on Windows: there the wait listens on its own toast, so a click on
//!   the second toast opens nothing (#2529).
//! - **At most [`WAITER_CAP`] concurrent waiters.** A send beyond it still
//!   shows -- the reader is told -- but drops the handle, which on every
//!   backend sends fire-and-forget, and says so at `debug`. A clicked banner
//!   past the cap opens nothing, which is the pre-#339 behaviour for that one
//!   desktop notification rather than for all of them.
//!
//! A completed wait frees its slot, whichever way it completed. The registry
//! is tested with an injected backend so the tests show nothing on the OS;
//! [`platform::Desktop`] is the one real backend and is exercised by the signed
//! bundle check in `testenv/README.md`.
//!
//! ## Platforms
//!
//! macOS is the witnessed one, on the `UNUserNotificationCenter` backend
//! (`preview-macos-un`, Björn's ruling on the prototype's evidence: current
//! API, a clean banner, and a body click reported without a button). Linux
//! gets the freedesktop `default` action, which is what makes a body click
//! reportable at all there. Both wait on `notify-rust`'s handle.
//!
//! Windows does not (#2529). `notify-rust` 4.18.0's Windows handle reported a
//! body click as [`CLOSED`] through `wait_for_action`, returned on the
//! banner's timeout and so dropped the receiver a later click from Action
//! Center was sent to, and sent under PowerShell's AUMID for want of one. The
//! Windows backend ([`platform`]'s `toast`) sends on WinRT itself: under the
//! bundle identifier, registered per user with knobas's name and icon at
//! start ([`aumid_registration`]), holding the toast for the whole wait, and
//! reading its reports through [`decide`], which reads past a timed-out
//! banner. The rule is plain code tested on every OS; the WinRT shell around
//! it compiles under `just clippy-windows` and **has not been witnessed** on
//! Windows yet -- the check is in `testenv/README.md`.
//!
//! Whichever backend reports the click, the window comes up
//! ([`NotificationEvents::bring_forward`]) before the address leaves.

use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

/// How many desktop notifications may be waited on at once. See the module
/// note.
pub const WAITER_CAP: usize = 16;

/// The action id `notify-rust` reports when a desktop notification was
/// cleared or dismissed rather than clicked, on every backend.
pub const CLOSED: &str = "__closed";

/// What the frontend asks to have shown: the `notify` command's argument
/// (#339).
///
/// `address` is where the click goes, the inbox's own `addressOf` hash
/// (`lib/inbox/address.ts`, shared with the inbox row's *Open* since #447);
/// the backend carries it unread and hands it back on
/// `EVENTS.notificationClicked`, so both ends of the click path stay the
/// frontend's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NotificationDraft {
    pub title: String,
    pub body: String,
    /// Where the click goes -- the store's own `addressOf`, carried through
    /// unread so the two ends of the click path stay the frontend's.
    pub address: String,
}

/// The `notification:clicked` payload (#339): the `address` the notification
/// was sent with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NotificationClicked {
    pub address: String,
}

/// What a click does to the app: bring its window up, then leave the process
/// as an event.
///
/// A trait rather than an `AppHandle` for the reason `knobas_sync`'s
/// `SyncEvents` is one: the registry's tests observe the emit without Tauri.
/// `sources::events::TauriEvents` is the real implementation.
pub trait NotificationEvents: Send + Sync + 'static {
    /// Unminimize, show and focus the main window. Called before
    /// [`Self::notification_clicked`], so the reader sees the item open
    /// rather than a navigation behind another app's window (#2529).
    fn bring_forward(&self);
    fn notification_clicked(&self, address: String);
}

/// A shown desktop notification's wait: blocks until the reader acts and
/// answers with the action id (`"default"` for the body, [`CLOSED`] for a
/// clear).
///
/// Owns the platform handle; dropping it unwaited is how a desktop
/// notification beyond the cap is sent fire-and-forget.
pub type Wait = Box<dyn FnOnce() -> String>;

/// Show a draft and hand back its wait. The seam the registry is tested at.
///
/// Called on the waiter's own thread, so the handle never has to cross one --
/// `notify-rust`'s handles are not all `Send`, and the prototype showed and
/// waited on one thread too.
pub trait Backend: Send + Sync + 'static {
    /// # Errors
    ///
    /// Whatever the platform refused with, as a sentence for the log and the
    /// command's `internal` error.
    fn show(&self, draft: &NotificationDraft) -> Result<Wait, String>;
}

impl<F> Backend for F
where
    F: Fn(&NotificationDraft) -> Result<Wait, String> + Send + Sync + 'static,
{
    fn show(&self, draft: &NotificationDraft) -> Result<Wait, String> {
        self(draft)
    }
}

/// What became of a send, for the tests -- the command discards it, having
/// said what it needed to at `debug` on the thread. The desktop notification
/// was shown in every case; the variants say whether a click can reach anybody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Shown, and a thread is waiting on its click.
    Waiting,
    /// Shown; a wait for the same address was already running, so a click on
    /// either banner lands on it -- except on Windows, where only the first
    /// toast's click does.
    AlreadyWaiting,
    /// Shown fire-and-forget: [`WAITER_CAP`] waits were already running.
    OverCap,
}

/// The waiter registry over a backend. Managed once by the app.
pub struct Notifier {
    waiting: Arc<Mutex<HashSet<String>>>,
    cap: usize,
    backend: Arc<dyn Backend>,
    events: Arc<dyn NotificationEvents>,
}

/// What [`Notifier::admit`] decided: a slot to wait in, or why there is none.
enum Admission {
    Wait(Slot),
    Refused(Delivery),
}

/// A reserved place in the registry, given back on drop -- so a wait that
/// ends any way at all, a show that fails included, frees it.
struct Slot {
    waiting: Arc<Mutex<HashSet<String>>>,
    address: String,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.address);
    }
}

impl Notifier {
    pub fn new(backend: Arc<dyn Backend>, events: Arc<dyn NotificationEvents>) -> Self {
        Self {
            waiting: Arc::new(Mutex::new(HashSet::new())),
            cap: WAITER_CAP,
            backend,
            events,
        }
    }

    /// How many waits are running. For the tests.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Reserve a slot for `address`, or say why not. One lock, so two sends
    /// racing for the last slot cannot both take it.
    fn admit(&self, address: &str) -> Admission {
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        if waiting.contains(address) {
            return Admission::Refused(Delivery::AlreadyWaiting);
        }
        if waiting.len() >= self.cap {
            return Admission::Refused(Delivery::OverCap);
        }
        waiting.insert(address.to_owned());
        Admission::Wait(Slot {
            waiting: Arc::clone(&self.waiting),
            address: address.to_owned(),
        })
    }

    /// Show the draft and, when admitted, wait for its click on a thread of
    /// its own. Returns once the desktop notification is on screen (or
    /// refused), not when it is clicked.
    ///
    /// # Errors
    ///
    /// The platform's refusal, as a sentence. On macOS the one to expect is
    /// `UNUserNotificationCenter`'s *no bundle identifier* from a bare `tauri
    /// dev` binary -- that backend needs a `.app`.
    pub fn notify(&self, draft: NotificationDraft) -> Result<Delivery, String> {
        let admitted = self.admit(&draft.address);
        let delivery = match &admitted {
            Admission::Wait(_) => Delivery::Waiting,
            Admission::Refused(why) => *why,
        };
        let backend = Arc::clone(&self.backend);
        let events = Arc::clone(&self.events);
        let (shown, on_screen) = mpsc::channel::<Result<(), String>>();

        let spawned = std::thread::Builder::new()
            .name(format!("notification {}", draft.address))
            .spawn(move || {
                let wait = match backend.show(&draft) {
                    Ok(wait) => wait,
                    Err(error) => {
                        let _ = shown.send(Err(error));
                        return;
                    }
                };
                let _ = shown.send(Ok(()));
                let slot = match admitted {
                    Admission::Wait(slot) => slot,
                    Admission::Refused(Delivery::AlreadyWaiting) => {
                        tracing::debug!(
                            address = draft.address,
                            "shown without a wait: one is already waiting on this address"
                        );
                        drop(wait);
                        return;
                    }
                    Admission::Refused(_) => {
                        tracing::debug!(
                            address = draft.address,
                            cap = WAITER_CAP,
                            "shown without a wait: the waiter cap is reached"
                        );
                        drop(wait);
                        return;
                    }
                };
                tracing::info!(
                    address = draft.address,
                    "notification shown, waiting on its click"
                );
                let action = wait();
                if action == CLOSED {
                    tracing::debug!(
                        address = draft.address,
                        "notification closed without a click"
                    );
                } else {
                    tracing::info!(address = draft.address, action, "notification clicked");
                    events.bring_forward();
                    events.notification_clicked(draft.address.clone());
                }
                // Released after the emit, so a registry that reads as empty
                // has nothing left in flight (the tests lean on that order).
                drop(slot);
            });
        if let Err(error) = spawned {
            return Err(format!(
                "could not start the desktop notification thread: {error}"
            ));
        }
        on_screen
            .recv()
            .map_err(|_| "the desktop notification thread ended before showing".to_owned())??;
        Ok(delivery)
    }
}

/// The action id of a body click, on every backend.
pub const DEFAULT: &str = "default";

/// What a shown desktop notification reported, in words the wait decides on
/// whichever platform said it (#2529). Windows is the one backend that feeds
/// these today ([`platform`]'s toast); the rule they serve is here so it is
/// tested on every OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reported {
    /// The body was clicked.
    Body,
    /// A button with this id was clicked.
    Button(String),
    /// The banner timed out. On Windows that is `ToastDismissalReason::TimedOut`,
    /// and the toast has moved into Action Center, where it can still be
    /// clicked: it is not a clear.
    BannerExpired,
    /// The reader dismissed it, the app hid it, or the platform failed it:
    /// nothing more will come.
    Gone,
}

/// A Windows toast activation, by its `Arguments`: empty for the body (the
/// toast carries no `launch` arguments), a button's id otherwise.
#[must_use]
pub fn reported_activation(arguments: &str) -> Reported {
    if arguments.is_empty() {
        Reported::Body
    } else {
        Reported::Button(arguments.to_owned())
    }
}

/// A Windows toast dismissal: a timed-out banner went to Action Center; every
/// other reason (the reader, the app, an unknown one) ends it.
#[must_use]
pub fn reported_dismissal(timed_out: bool) -> Reported {
    if timed_out {
        Reported::BannerExpired
    } else {
        Reported::Gone
    }
}

/// Read a notification's reports until one decides it, and answer with the
/// action id: [`DEFAULT`] for the body, the button's id, or [`CLOSED`].
///
/// `next` answers `None` once the platform can report nothing more (its
/// senders dropped), which is a clear. A banner that expired into Action
/// Center is read past, so a click there still reaches the item -- the trap
/// in `notify-rust` 4.18.0's Windows handle, whose one `recv` answers the
/// expiry and drops the receiver the later click is sent to (#2529).
pub fn decide(mut next: impl FnMut() -> Option<Reported>) -> String {
    loop {
        match next() {
            Some(Reported::Body) => return DEFAULT.to_owned(),
            Some(Reported::Button(id)) => return id,
            Some(Reported::BannerExpired) => {}
            Some(Reported::Gone) | None => return CLOSED.to_owned(),
        }
    }
}

/// The toast document for a draft: the generic template, title then body as
/// text, escaped. No `launch` arguments, so a body click activates with empty
/// ones ([`reported_activation`]).
#[must_use]
pub fn toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\">\
         <text>{}</text><text>{}</text>\
         </binding></visual></toast>",
        xml_text(title),
        xml_text(body)
    )
}

fn xml_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The per-user registry values that give an AUMID a name and an icon in
/// toasts and Action Center, written by [`platform::register_aumid`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AumidKey {
    /// Under `HKEY_CURRENT_USER`.
    pub subkey: String,
    /// `(value name, REG_SZ value)`, in write order.
    pub values: Vec<(&'static str, String)>,
}

/// What to write so Windows knows `app_id` by `display_name` and `icon`
/// without a Start menu shortcut: `Software\Classes\AppUserModelId\<id>`,
/// the registration the Windows Community Toolkit and the Windows App SDK
/// make for an unpackaged app. An installed knobas has the shortcut too,
/// carrying the same id and name, so the two agree.
#[must_use]
pub fn aumid_registration(app_id: &str, display_name: &str, icon: &std::path::Path) -> AumidKey {
    AumidKey {
        subkey: format!("Software\\Classes\\AppUserModelId\\{app_id}"),
        values: vec![
            ("DisplayName", display_name.to_owned()),
            ("IconUri", icon.display().to_string()),
        ],
    }
}

/// The real backend: a WinRT toast on Windows, `notify-rust` elsewhere.
pub mod platform {
    use super::{Backend, NotificationDraft, Wait};

    /// The desktop notification backend, carrying the identity a Windows
    /// toast is sent under.
    pub struct Desktop {
        app_id: String,
    }

    impl Desktop {
        /// `app_id` is the bundle identifier (`tauri.conf.json`'s
        /// `identifier`): the AUMID Tauri's installer puts on the Start menu
        /// shortcut, and the one [`register_aumid`] registers. Read on
        /// Windows only; the other backends take the identity from the
        /// running bundle.
        #[must_use]
        pub fn new(app_id: &str) -> Self {
            Self {
                app_id: app_id.to_owned(),
            }
        }

        /// The identity toasts are sent under.
        #[must_use]
        pub fn app_id(&self) -> &str {
            &self.app_id
        }
    }

    impl Backend for Desktop {
        #[cfg(windows)]
        fn show(&self, draft: &NotificationDraft) -> Result<Wait, String> {
            toast::show(&self.app_id, draft)
        }

        #[cfg(not(windows))]
        fn show(&self, draft: &NotificationDraft) -> Result<Wait, String> {
            show(draft)
        }
    }

    /// Show `draft` with `notify-rust` and hand back its handle's wait.
    ///
    /// No application identity is set first. On the `UNUserNotificationCenter`
    /// backend the identity is the running bundle's, and a bare `tauri dev`
    /// binary has none: its send fails with *no bundle identifier* (recorded
    /// in `testenv/README.md`). #339 first carried over
    /// `tauri-plugin-notification`'s NS-backend rule -- `set_application`
    /// with the bundle identifier, or `com.apple.Terminal` under
    /// `tauri::is_dev()` -- and #380 took it out again: under
    /// `preview-macos-un`, `notify-rust` 4.18.0 exports that function as
    /// `deprecated(note = "these functions have no effect in this
    /// configuration")` (`src/macos/mod.rs`). Do not re-add it; it cannot
    /// make `tauri dev` show anything.
    ///
    /// # Errors
    ///
    /// `notify-rust`'s error, displayed.
    #[cfg(not(windows))]
    pub fn show(draft: &NotificationDraft) -> Result<Wait, String> {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&draft.title).body(&draft.body);
        // The freedesktop spec's body-click action: without it an XDG server
        // reports no click at all. On the UN backend the body click is
        // reported on its own, and a registered action would draw a button.
        #[cfg(all(unix, not(target_os = "macos")))]
        notification.action("default", "Open");

        let handle = notification.show().map_err(|error| error.to_string())?;
        Ok(Box::new(move || {
            let mut action = super::CLOSED.to_owned();
            handle.wait_for_action(|reported| action = reported.to_owned());
            action
        }))
    }

    /// Register `app_id` for the current user with knobas's name and icon
    /// ([`super::aumid_registration`]), the icon written as a PNG into
    /// `data_dir`. Without it a toast sent under an id that no shortcut
    /// carries (a dev or portable run) is not shown at all. Idempotent, so
    /// it runs on every start and follows a moved install.
    ///
    /// # Errors
    ///
    /// The icon write or the registry write that failed, as a sentence.
    #[cfg(windows)]
    pub fn register_aumid(
        app_id: &str,
        display_name: &str,
        data_dir: &std::path::Path,
    ) -> Result<(), String> {
        const ICON: &[u8] = include_bytes!("../icons/128x128.png");
        std::fs::create_dir_all(data_dir)
            .map_err(|error| format!("{}: {error}", data_dir.display()))?;
        let icon = data_dir.join("notification-icon.png");
        std::fs::write(&icon, ICON).map_err(|error| format!("{}: {error}", icon.display()))?;
        let key = super::aumid_registration(app_id, display_name, &icon);
        for (name, value) in &key.values {
            crate::win_registry::set_string(&key.subkey, Some(name), value.as_ref())?;
        }
        Ok(())
    }

    /// The Windows toast, on WinRT directly rather than through
    /// `notify-rust`'s handle (#2529). That handle answers a body click as
    /// [`super::CLOSED`] through `wait_for_action`, and its one `recv`
    /// returns on the banner's timeout and drops the receiver a later click
    /// from Action Center is sent to; it also lets go of the
    /// `ToastNotification` once shown. Here the toast and its notifier are
    /// held for the whole wait, the handlers report into a channel the wait
    /// reads until [`super::decide`] decides, and the id is knobas's.
    #[cfg(windows)]
    mod toast {
        use std::sync::mpsc;

        use windows::Data::Xml::Dom::XmlDocument;
        use windows::Foundation::TypedEventHandler;
        use windows::UI::Notifications::{
            ToastActivatedEventArgs, ToastDismissalReason, ToastDismissedEventArgs,
            ToastFailedEventArgs, ToastNotification, ToastNotificationManager, ToastNotifier,
        };
        use windows::core::{HSTRING, IInspectable, Interface};

        use crate::notify::{
            NotificationDraft, Reported, Wait, decide, reported_activation, reported_dismissal,
            toast_xml,
        };

        fn said(error: windows::core::Error) -> String {
            format!("Windows toast: {error}")
        }

        pub fn show(app_id: &str, draft: &NotificationDraft) -> Result<Wait, String> {
            let xml = XmlDocument::new().map_err(said)?;
            xml.LoadXml(&HSTRING::from(toast_xml(&draft.title, &draft.body)))
                .map_err(said)?;
            let toast = ToastNotification::CreateToastNotification(&xml).map_err(said)?;
            let (report, reports) = mpsc::channel::<Reported>();

            let activated = report.clone();
            let on_activated = toast
                .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                    move |_, args| {
                        let arguments = args
                            .as_ref()
                            .and_then(|args| args.cast::<ToastActivatedEventArgs>().ok())
                            .and_then(|args| args.Arguments().ok())
                            .map(|arguments| arguments.to_string())
                            .unwrap_or_default();
                        let _ = activated.send(reported_activation(&arguments));
                        Ok(())
                    },
                ))
                .map_err(said)?;
            let dismissed = report.clone();
            let on_dismissed = toast
                .Dismissed(&TypedEventHandler::<
                    ToastNotification,
                    ToastDismissedEventArgs,
                >::new(move |_, args| {
                    let reason = args.as_ref().and_then(|args| args.Reason().ok());
                    let _ = dismissed.send(reported_dismissal(
                        reason == Some(ToastDismissalReason::TimedOut),
                    ));
                    Ok(())
                }))
                .map_err(said)?;
            let on_failed = toast
                .Failed(
                    &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(
                        move |_, args| {
                            let code = args.as_ref().and_then(|args| args.ErrorCode().ok());
                            tracing::warn!(?code, "the Windows toast failed");
                            let _ = report.send(Reported::Gone);
                            Ok(())
                        },
                    ),
                )
                .map_err(said)?;

            let notifier =
                ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
                    .map_err(said)?;
            let held = Held {
                toast,
                notifier,
                handlers: [on_activated, on_dismissed, on_failed],
            };
            held.notifier.Show(&held.toast).map_err(said)?;
            Ok(Box::new(move || {
                // Held until the wait ends: the handlers live on the toast,
                // and a toast nothing holds can lose them before a click from
                // Action Center arrives.
                let action = decide(|| reports.recv().ok());
                drop(held);
                action
            }))
        }

        /// A shown toast, its notifier and its three handler tokens. Dropped
        /// when the wait ends, or unrun when the notifier dropped the wait
        /// (a second banner for the address, or past the cap): either way the
        /// handlers come off the toast, so a toast Windows still holds for
        /// Action Center keeps no closure or sender of ours alive.
        struct Held {
            toast: ToastNotification,
            notifier: ToastNotifier,
            /// `Activated`, `Dismissed`, `Failed`, in that order.
            handlers: [i64; 3],
        }

        impl Drop for Held {
            fn drop(&mut self) {
                let [activated, dismissed, failed] = self.handlers;
                let _ = self.toast.RemoveActivated(activated);
                let _ = self.toast.RemoveDismissed(dismissed);
                let _ = self.toast.RemoveFailed(failed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Sender};
    use std::time::{Duration, Instant};

    use super::*;

    /// A backend that shows nothing and waits on a channel per address, so a
    /// test decides when each wait ends and how.
    #[derive(Default)]
    struct FakeBackend {
        /// The addresses whose wait actually started, in order.
        started: Mutex<Vec<String>>,
        shown: Mutex<Vec<String>>,
        /// The gate of each wait that is *running*, by address. A gate is
        /// filed when its wait starts, not when its banner is shown: a
        /// duplicate or over-cap send shows too, drops its wait on its own
        /// thread, and a gate filed at show time could be picked by `release`
        /// before that thread had dropped it -- the release would reach no
        /// one and the real wait would run on (#1762).
        gates: Mutex<HashMap<String, Sender<String>>>,
        refuse: Mutex<bool>,
    }

    impl Backend for Arc<FakeBackend> {
        fn show(&self, draft: &NotificationDraft) -> Result<Wait, String> {
            if *self.refuse.lock().unwrap() {
                return Err("the platform said no".to_owned());
            }
            self.shown.lock().unwrap().push(draft.address.clone());
            let (release, released) = mpsc::channel::<String>();
            let me = Arc::clone(self);
            let address = draft.address.clone();
            Ok(Box::new(move || {
                me.gates.lock().unwrap().insert(address.clone(), release);
                me.started.lock().unwrap().push(address);
                released.recv().unwrap_or_else(|_| CLOSED.to_owned())
            }))
        }
    }

    #[derive(Default)]
    struct FakeEvents {
        clicked: Mutex<Vec<String>>,
        /// Everything the click did to the app, in order: `forward` for the
        /// window brought up, `open <address>` for the address handed on.
        steps: Mutex<Vec<String>>,
        /// When set, the first emit records its address and then blocks
        /// until the test opens the gate -- so the test can look at the
        /// registry while a click is still leaving.
        hold: Mutex<Option<mpsc::Receiver<()>>>,
    }

    impl NotificationEvents for FakeEvents {
        fn bring_forward(&self) {
            self.steps.lock().unwrap().push("forward".to_owned());
        }

        fn notification_clicked(&self, address: String) {
            self.steps.lock().unwrap().push(format!("open {address}"));
            let gate = self.hold.lock().unwrap().take();
            self.clicked.lock().unwrap().push(address);
            if let Some(gate) = gate {
                let _ = gate.recv();
            }
        }
    }

    struct Bench {
        notifier: Notifier,
        backend: Arc<FakeBackend>,
        events: Arc<FakeEvents>,
    }

    impl Bench {
        fn new() -> Self {
            let backend = Arc::new(FakeBackend::default());
            let events = Arc::new(FakeEvents::default());
            let notifier = Notifier::new(
                Arc::new(Arc::clone(&backend)),
                Arc::clone(&events) as Arc<dyn NotificationEvents>,
            );
            Self {
                notifier,
                backend,
                events,
            }
        }

        fn send(&self, address: &str) -> Delivery {
            self.notifier
                .notify(draft(address))
                .expect("the fake backend shows")
        }

        /// End the wait on `address` with `action`, once that wait is running
        /// (its thread files the gate when it starts, a moment after the send
        /// returned).
        fn release(&self, address: &str, action: &str) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(gate) = self.backend.gates.lock().unwrap().remove(address) {
                    gate.send(action.to_owned())
                        .expect("the running wait holds its receiver");
                    return;
                }
                assert!(Instant::now() < deadline, "no wait started for {address}");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        /// Block until no wait is running -- every emit has happened by then.
        fn settled(&self) {
            self.settled_to(0);
        }

        /// Block until exactly `count` waits are running.
        fn settled_to(&self, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.notifier.waiting() != count {
                assert!(Instant::now() < deadline, "a wait never ended");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn clicked(&self) -> Vec<String> {
            self.events.clicked.lock().unwrap().clone()
        }

        fn steps(&self) -> Vec<String> {
            self.events.steps.lock().unwrap().clone()
        }

        fn shown(&self) -> Vec<String> {
            self.backend.shown.lock().unwrap().clone()
        }

        fn started(&self) -> Vec<String> {
            self.backend.started.lock().unwrap().clone()
        }
    }

    fn draft(address: &str) -> NotificationDraft {
        NotificationDraft {
            title: "Title".to_owned(),
            body: "Body".to_owned(),
            address: address.to_owned(),
        }
    }

    /// The whole path once: shown, waited on, and the click leaves as the
    /// event carrying the address -- while a clear leaves as nothing.
    ///
    /// The silence is asserted against the click in the same test, the rule
    /// `notify.test.ts` follows: "nothing was emitted" alone cannot
    /// tell a working `__closed` branch from a bench whose events port was
    /// never wired.
    #[test]
    fn a_click_reaches_the_events_with_its_address_and_a_clear_does_not() {
        let b = Bench::new();
        assert_eq!(b.send("#/inbox"), Delivery::Waiting);
        b.release("#/inbox", CLOSED);
        b.settled();
        assert_eq!(b.clicked(), Vec::<String>::new(), "a clear is not a click");

        assert_eq!(b.send("#/entity/x"), Delivery::Waiting);
        b.release("#/entity/x", "default");
        b.settled();
        assert_eq!(b.clicked(), vec!["#/entity/x"]);
    }

    /// A click brings the main window up -- unminimized, shown, in front --
    /// before the address leaves for the frontend to navigate to (#2529),
    /// and a clear brings up nothing.
    #[test]
    fn a_click_brings_the_window_forward_before_its_address_leaves() {
        let b = Bench::new();
        b.send("#/inbox");
        b.release("#/inbox", CLOSED);
        b.settled();
        assert_eq!(b.steps(), Vec::<String>::new(), "a clear moves no window");

        b.send("#/entity/x");
        b.release("#/entity/x", "default");
        b.settled();
        assert_eq!(b.steps(), vec!["forward", "open #/entity/x"]);
    }

    /// One waiter per address: the second banner is shown, and its click --
    /// on either banner -- lands on the wait that already exists.
    #[test]
    fn a_second_send_for_an_address_already_waiting_shows_but_starts_no_second_wait() {
        let b = Bench::new();
        assert_eq!(b.send("#/inbox"), Delivery::Waiting);
        assert_eq!(b.send("#/inbox"), Delivery::AlreadyWaiting);
        assert_eq!(b.shown(), vec!["#/inbox", "#/inbox"], "both were shown");
        assert_eq!(b.notifier.waiting(), 1);

        b.release("#/inbox", "default");
        b.settled();
        assert_eq!(b.started(), vec!["#/inbox"], "exactly one wait ran");
        assert_eq!(b.clicked(), vec!["#/inbox"]);
    }

    fn address(n: usize) -> String {
        format!("#/entity/{n}")
    }

    /// The seventeenth concurrent waiter is shown and not waited on.
    #[test]
    fn the_seventeenth_concurrent_waiter_shows_but_does_not_wait() {
        let b = Bench::new();
        for n in 0..WAITER_CAP {
            assert_eq!(b.send(&address(n)), Delivery::Waiting);
        }
        assert_eq!(b.send("#/inbox"), Delivery::OverCap);
        assert_eq!(b.shown().len(), WAITER_CAP + 1, "the seventeenth was shown");
        assert_eq!(b.notifier.waiting(), WAITER_CAP);

        for n in 0..WAITER_CAP {
            b.release(&address(n), CLOSED);
        }
        b.settled();
        assert_eq!(
            b.started().len(),
            WAITER_CAP,
            "sixteen waits ran, not seventeen"
        );
    }

    /// A completed wait frees its slot: after one of sixteen ends, the next
    /// send waits again -- and the address whose wait ended can be waited on
    /// again.
    #[test]
    fn a_completed_wait_frees_its_slot_for_the_next_send() {
        let b = Bench::new();
        for n in 0..WAITER_CAP {
            assert_eq!(b.send(&address(n)), Delivery::Waiting);
        }
        assert_eq!(b.send("#/inbox"), Delivery::OverCap);

        b.release(&address(0), "default");
        let deadline = Instant::now() + Duration::from_secs(5);
        while b.notifier.waiting() == WAITER_CAP {
            assert!(
                Instant::now() < deadline,
                "the finished wait never freed its slot"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            b.send("#/inbox"),
            Delivery::Waiting,
            "the freed slot is taken"
        );
        assert_eq!(
            b.send(&address(0)),
            Delivery::OverCap,
            "and it was the only one"
        );

        b.release("#/inbox", CLOSED);
        b.settled_to(WAITER_CAP - 1);
        assert_eq!(
            b.send(&address(0)),
            Delivery::Waiting,
            "an ended address can wait again"
        );

        for n in 1..WAITER_CAP {
            b.release(&address(n), CLOSED);
        }
        b.release(&address(0), CLOSED);
        b.settled();
    }

    /// The slot outlives the emit. While a click is still leaving, the
    /// registry still counts its wait, so a registry that reads as empty has
    /// nothing left in flight -- the order `settled` leans on above. Pinned
    /// here because every other test passes with the slot released *before*
    /// the emit: the gap is microseconds, and `settled` wins the race.
    #[test]
    fn the_slot_is_held_until_the_click_has_left() {
        let b = Bench::new();
        let (open, gate) = mpsc::channel::<()>();
        *b.events.hold.lock().unwrap() = Some(gate);
        assert_eq!(b.send("#/inbox"), Delivery::Waiting);
        b.release("#/inbox", "default");
        let deadline = Instant::now() + Duration::from_secs(5);
        while b.clicked().is_empty() {
            assert!(Instant::now() < deadline, "the click never began to leave");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            b.notifier.waiting(),
            1,
            "the slot is freed only once the click has left"
        );
        open.send(()).expect("the emit is waiting on the gate");
        b.settled();
        assert_eq!(b.clicked(), vec!["#/inbox"]);
    }

    /// Feed `decide` a fixed run of reports, and say how many it read.
    fn decided(reports: Vec<Reported>) -> (String, usize) {
        let mut reports = reports.into_iter();
        let mut read = 0;
        let action = decide(|| {
            read += 1;
            reports.next()
        });
        (action, read)
    }

    /// A Windows toast's body click comes as an activation with no
    /// arguments, and it is a click -- the `"default"` action -- not the
    /// clear `notify-rust`'s `wait_for_action` turned it into (#2529).
    #[test]
    fn a_body_click_is_the_default_action() {
        assert_eq!(reported_activation(""), Reported::Body);
        assert_eq!(decided(vec![Reported::Body]), ("default".to_owned(), 1));
    }

    /// A button's activation carries its id, which is the action.
    #[test]
    fn a_button_click_is_its_own_action() {
        assert_eq!(
            reported_activation("snooze"),
            Reported::Button("snooze".to_owned())
        );
        assert_eq!(
            decided(vec![Reported::Button("snooze".to_owned())]),
            ("snooze".to_owned(), 1)
        );
    }

    /// The banner timing out moves the toast into Action Center, where it
    /// can still be clicked: the wait reads past it and the later click
    /// still opens the item (#2529).
    #[test]
    fn a_banner_that_times_out_into_action_center_still_waits_for_its_click() {
        assert_eq!(reported_dismissal(true), Reported::BannerExpired);
        assert_eq!(
            decided(vec![Reported::BannerExpired, Reported::Body]),
            ("default".to_owned(), 2)
        );
    }

    /// A dismiss, the app hiding it or a failure ends the wait as a clear,
    /// after the banner expired as well as before.
    #[test]
    fn a_dismiss_ends_the_wait_as_a_clear() {
        assert_eq!(reported_dismissal(false), Reported::Gone);
        assert_eq!(decided(vec![Reported::Gone]), (CLOSED.to_owned(), 1));
        assert_eq!(
            decided(vec![
                Reported::BannerExpired,
                Reported::Gone,
                Reported::Body
            ]),
            (CLOSED.to_owned(), 2),
            "nothing after a dismiss is read"
        );
    }

    /// A notification the platform let go of (every sender dropped) is a
    /// clear, not a wait that never ends.
    #[test]
    fn a_notification_the_platform_dropped_ends_as_a_clear() {
        assert_eq!(
            decided(vec![Reported::BannerExpired]),
            (CLOSED.to_owned(), 2)
        );
    }

    /// The toast's XML carries the title and body as text, escaped, so an
    /// item called `<b>` or `a & b` shows as written and cannot break the
    /// document the toast is built from.
    #[test]
    fn the_toast_xml_carries_title_and_body_escaped() {
        assert_eq!(
            toast_xml("PAY-1 <b> & \"x\"", "it's 'done'"),
            "<toast><visual><binding template=\"ToastGeneric\">\
             <text>PAY-1 &lt;b&gt; &amp; &quot;x&quot;</text>\
             <text>it&apos;s &apos;done&apos;</text>\
             </binding></visual></toast>"
        );
    }

    /// The toast identity is the bundle identifier: the AUMID Tauri's
    /// installer stamps on the Start menu shortcut, so a toast from an
    /// installed knobas and its taskbar button are one app, and not
    /// PowerShell's fallback id (#2529).
    #[test]
    fn the_toast_identity_is_the_bundle_identifier() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let identifier = config["identifier"].as_str().expect("an identifier");
        assert_eq!(
            platform::Desktop::new(identifier).app_id(),
            "dev.knobas.desktop"
        );
    }

    /// The per-user registration that gives a toast knobas's name and icon
    /// without a shortcut (a dev or portable run): the key Windows looks the
    /// AUMID up under, the name, and the icon file.
    #[test]
    fn the_aumid_registration_names_knobas_and_its_icon() {
        let key = aumid_registration(
            "dev.knobas.desktop",
            "knobas",
            std::path::Path::new(
                "C:\\Users\\b\\AppData\\Roaming\\dev.knobas.desktop\\notification-icon.png",
            ),
        );
        assert_eq!(
            key.subkey,
            "Software\\Classes\\AppUserModelId\\dev.knobas.desktop"
        );
        assert_eq!(
            key.values,
            vec![
                ("DisplayName", "knobas".to_owned()),
                (
                    "IconUri",
                    "C:\\Users\\b\\AppData\\Roaming\\dev.knobas.desktop\\notification-icon.png"
                        .to_owned()
                ),
            ]
        );
    }

    /// A platform that refuses is reported, and keeps no slot: the next send
    /// for the same address is a fresh attempt rather than a duplicate. (The
    /// slot is held *while* the show runs -- `admitted` travels with the
    /// thread -- so a same-address send racing a show that then fails reads
    /// `AlreadyWaiting` and goes fire-and-forget; that window is the show's
    /// few milliseconds, and it is freed the moment the refusal is known.)
    #[test]
    fn a_refused_show_is_an_error_and_holds_no_slot() {
        let b = Bench::new();
        *b.backend.refuse.lock().unwrap() = true;
        let refused = b.notifier.notify(draft("#/inbox")).unwrap_err();
        assert_eq!(refused, "the platform said no");
        b.settled();

        *b.backend.refuse.lock().unwrap() = false;
        assert_eq!(b.send("#/inbox"), Delivery::Waiting);
        b.release("#/inbox", CLOSED);
        b.settled();
    }
}
