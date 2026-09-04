//! Desktop notifications with a click that arrives (#339, spec #272
//! "Notifications").
//!
//! #290 sent notifications through `tauri-plugin-notification`, whose desktop
//! `notify` hands the notification to `notify-rust` and drops the handle --
//! so nothing in the process could learn of a click, and the plugin's
//! `register_listener` exists on mobile only. This module owns the send
//! instead: `notify-rust` shows the notification, the handle's
//! `wait_for_action` runs on a thread of its own, and a click becomes the
//! `notification:clicked` event with the item's address, which the store on
//! the other side already knows how to navigate to. The decision to own the
//! click on `notify-rust` rather than on a second wrapper is
//! `docs/research/2026-09-04-notification-library-alternatives.md`; that the
//! wait fires inside a signed Tauri bundle from a worker thread was witnessed
//! by `prototype/notification-click` (the verdict table on #339).
//!
//! ## The wait is unbounded, so there is a registry
//!
//! The prototype showed `wait_for_action` returning only on a click or on the
//! user clearing the notification from Notification Center -- **never** on
//! the banner sliding away. Every wait is therefore a thread blocked for as
//! long as the reader ignores that banner, which could be the whole session,
//! and knobas neither invents a timeout (the OS gives none, and a made-up one
//! drops real clicks) nor lets the count grow without bound. Two rules:
//!
//! - **One waiter per address.** A second send for an address already waited
//!   on shows the notification and starts no second wait: the click on either
//!   banner lands on the one thread, and it goes to the same room.
//! - **At most [`WAITER_CAP`] concurrent waiters.** A send beyond it still
//!   shows -- the reader is told -- but drops the handle, which on every
//!   backend sends fire-and-forget, and says so at `debug`. A clicked banner
//!   past the cap opens nothing, which is the pre-#339 behaviour for that one
//!   notification rather than for all of them.
//!
//! A completed wait frees its slot, whichever way it completed. The registry
//! is tested with an injected backend so the tests touch no OS notification;
//! [`platform::show`] is the one real backend and is exercised by the signed
//! bundle check in `testenv/README.md`.
//!
//! ## Platforms
//!
//! macOS is the witnessed one, on the `UNUserNotificationCenter` backend
//! (`preview-macos-un`, Björn's ruling on the prototype's evidence: current
//! API, a clean banner, and a body click reported without a button). Linux
//! gets the freedesktop `default` action, which is what makes a body click
//! reportable at all there; Windows uses the handle API as it stands, with
//! one thing worth knowing: its `wait_for_action` answers [`CLOSED`] for a
//! failed activation as well as for a dismiss (`windows.rs`, `Err(_) =>
//! "__closed"`), so a broken click there reads as a clear. Both are written
//! from `notify-rust` 4.18.0's sources and **neither has been witnessed** --
//! the same disclosure `testenv/README.md` and the §10.8 entry carry.

use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

/// How many notifications may be waited on at once. See the module note.
pub const WAITER_CAP: usize = 16;

/// The action id `notify-rust` reports when a notification was cleared or
/// dismissed rather than clicked, on every backend.
pub const CLOSED: &str = "__closed";

/// What the frontend asks to have shown: the `notify` command's argument,
/// mirrored as `NotificationDraft` in `app/src/lib/ipc/entity.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDraft {
    pub title: String,
    pub body: String,
    /// Where the click goes -- the store's own `addressOf`, carried through
    /// unread so the two ends of the click path stay the frontend's.
    pub address: String,
}

/// The `notification:clicked` payload, mirrored as `NotificationClicked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationClicked {
    pub address: String,
}

/// The one thing a click does: leave the process as an event.
///
/// A trait rather than an `AppHandle` for the reason `knobas_sync`'s
/// `SyncEvents` is one: the registry's tests observe the emit without Tauri.
/// `sources::events::TauriEvents` is the real implementation.
pub trait NotificationEvents: Send + Sync + 'static {
    fn notification_clicked(&self, address: String);
}

/// A shown notification's wait: blocks until the reader acts and answers
/// with the action id (`"default"` for the body, [`CLOSED`] for a clear).
///
/// Owns the platform handle; dropping it unwaited is how a notification
/// beyond the cap is sent fire-and-forget.
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
/// said what it needed to at `debug` on the thread. The notification was shown
/// in every case; the variants say whether a click can reach anybody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Shown, and a thread is waiting on its click.
    Waiting,
    /// Shown; a wait for the same address was already running, so a click on
    /// either banner lands on it.
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
    /// its own. Returns once the notification is on screen (or refused), not
    /// when it is clicked.
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
                    events.notification_clicked(draft.address.clone());
                }
                // Released after the emit, so a registry that reads as empty
                // has nothing left in flight (the tests lean on that order).
                drop(slot);
            });
        if let Err(error) = spawned {
            return Err(format!("could not start the notification thread: {error}"));
        }
        on_screen
            .recv()
            .map_err(|_| "the notification thread ended before showing".to_owned())??;
        Ok(delivery)
    }
}

/// The real backend: `notify-rust`, on whichever OS this is.
pub mod platform {
    use super::{NotificationDraft, Wait};

    /// Show `draft` with `notify-rust` and hand back its handle's wait.
    ///
    /// On macOS the application identity is set once before the first send,
    /// the way `tauri-plugin-notification`'s desktop `notify` does it: the
    /// bundle identifier, or `com.apple.Terminal` under `tauri::is_dev()`.
    /// Under `preview-macos-un` `notify-rust` marks that call as having no
    /// effect -- the `UNUserNotificationCenter` backend takes its identity
    /// from the running bundle and refuses a bare binary outright -- so the
    /// call is kept for the rule and not for the effect, and a `tauri dev`
    /// send is expected to fail with *no bundle identifier* (recorded in
    /// `testenv/README.md`).
    ///
    /// # Errors
    ///
    /// `notify-rust`'s error, displayed.
    pub fn show(identifier: &str, draft: &NotificationDraft) -> Result<Wait, String> {
        #[cfg(target_os = "macos")]
        {
            static IDENTITY: std::sync::Once = std::sync::Once::new();
            IDENTITY.call_once(|| {
                let application = if tauri::is_dev() {
                    "com.apple.Terminal"
                } else {
                    identifier
                };
                #[allow(deprecated)]
                let outcome = notify_rust::set_application(application);
                tracing::debug!(application, ?outcome, "notification identity");
            });
        }
        #[cfg(not(target_os = "macos"))]
        let _ = identifier;

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
        /// One gate per *show*, oldest first: a duplicate send shows too,
        /// and its (never-run) wait must not steal the running one's gate.
        gates: Mutex<HashMap<String, Vec<Sender<String>>>>,
        refuse: Mutex<bool>,
    }

    impl Backend for Arc<FakeBackend> {
        fn show(&self, draft: &NotificationDraft) -> Result<Wait, String> {
            if *self.refuse.lock().unwrap() {
                return Err("the platform said no".to_owned());
            }
            self.shown.lock().unwrap().push(draft.address.clone());
            let (release, released) = mpsc::channel::<String>();
            self.gates
                .lock()
                .unwrap()
                .entry(draft.address.clone())
                .or_default()
                .push(release);
            let me = Arc::clone(self);
            let address = draft.address.clone();
            Ok(Box::new(move || {
                me.started.lock().unwrap().push(address);
                released.recv().unwrap_or_else(|_| CLOSED.to_owned())
            }))
        }
    }

    #[derive(Default)]
    struct FakeEvents {
        clicked: Mutex<Vec<String>>,
        /// When set, the first emit records its address and then blocks
        /// until the test opens the gate -- so the test can look at the
        /// registry while a click is still leaving.
        hold: Mutex<Option<mpsc::Receiver<()>>>,
    }

    impl NotificationEvents for FakeEvents {
        fn notification_clicked(&self, address: String) {
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

        /// End the wait on `address` with `action`.
        fn release(&self, address: &str, action: &str) {
            // A gate whose wait was never run (a duplicate or over-cap send
            // dropped it) has no receiver; skip to the one that is waiting.
            let mut gates = self.backend.gates.lock().unwrap();
            let queue = gates
                .get_mut(address)
                .expect("a notification was shown for this address");
            while !queue.is_empty() {
                if queue.remove(0).send(action.to_owned()).is_ok() {
                    return;
                }
            }
            panic!("no wait is running for {address}");
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
    /// `notify.test.svelte.ts` follows: "nothing was emitted" alone cannot
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
