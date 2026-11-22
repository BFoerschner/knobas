//! The clipboard a copied secret goes to (ADR-0056).
//!
//! **The Rust side puts it there and takes it away.** A copy never sends the
//! secret to the window. It is set with the platform's "keep out of history"
//! marks, and a Rust timer (a hidden webview throttles its own) clears it
//! after [`HOLD_SECS`](super::HOLD_SECS) **only if the clipboard still holds
//! what knobas put there**: a later copy by the person, from anywhere, is
//! left alone. The same check runs on quit.
//!
//! The keeper remembers a hash of what it set, never the value, so nothing
//! of the secret stays in the process for the 30 seconds.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};

/// What the clipboard is: a small trait, so the tests use a fake.
pub trait ClipboardPort: Send {
    /// Put `text` on the clipboard, marked as a secret where the platform
    /// has a mark.
    ///
    /// # Errors
    /// The platform's refusal, in words.
    fn set_secret(&mut self, text: &str) -> Result<(), String>;
    /// What the clipboard holds as text, `None` when it holds none.
    ///
    /// # Errors
    /// The platform's refusal, in words.
    fn text(&mut self) -> Result<Option<String>, String>;
    /// Empty it.
    ///
    /// # Errors
    /// The platform's refusal, in words.
    fn clear(&mut self) -> Result<(), String>;
}

/// The system clipboard through `arboard`, opened on first use and kept, so
/// on X11 its thread serves the selection while knobas runs.
#[derive(Default)]
pub struct SystemClipboard {
    open: Option<arboard::Clipboard>,
}

impl SystemClipboard {
    fn board(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.open.is_none() {
            self.open = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        Ok(self.open.as_mut().expect("opened above"))
    }
}

impl ClipboardPort for SystemClipboard {
    fn set_secret(&mut self, text: &str) -> Result<(), String> {
        let board = self.board()?;
        let set = board.set();
        // macOS: `org.nspasteboard.ConcealedType`, which clipboard managers
        // honour. Windows: no Win+V history, no cloud clipboard, no
        // monitoring. Linux: KDE's password-manager hint.
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple as _;
            set.exclude_from_history()
        };
        #[cfg(target_os = "windows")]
        let set = {
            use arboard::SetExtWindows as _;
            set.exclude_from_history()
                .exclude_from_cloud()
                .exclude_from_monitoring()
        };
        #[cfg(target_os = "linux")]
        let set = {
            use arboard::SetExtLinux as _;
            set.exclude_from_history()
        };
        set.text(text.to_owned()).map_err(|e| e.to_string())
    }

    fn text(&mut self) -> Result<Option<String>, String> {
        match self.board()?.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(other) => Err(other.to_string()),
        }
    }

    fn clear(&mut self) -> Result<(), String> {
        self.board()?.clear().map_err(|e| e.to_string())
    }
}

/// What the clipboard holds for knobas now, for the status bar: what it is,
/// whose, and until when. `None` fields when it holds nothing of knobas's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct ClipState {
    /// *password*, *one-time code*, or a field's name.
    pub what: Option<String>,
    /// The entry it came from.
    pub entity_id: Option<String>,
    /// When it leaves the clipboard, RFC 3339.
    pub until: Option<String>,
}

impl ClipState {
    fn empty() -> Self {
        Self {
            what: None,
            entity_id: None,
            until: None,
        }
    }
}

/// One thing knobas put on the clipboard.
struct Held {
    hash: [u8; 32],
    what: String,
    entity_id: Option<String>,
    deadline: Instant,
    wall: SystemTime,
    generation: u64,
}

fn hash(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn rfc3339(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Told when what the clipboard holds for knobas changes.
pub type Tell = Arc<dyn Fn(ClipState) + Send + Sync>;

/// The keeper: one per app.
#[derive(Clone)]
pub struct Keeper {
    inner: Arc<Inner>,
}

struct Inner {
    port: Mutex<Box<dyn ClipboardPort>>,
    held: Mutex<Option<Held>>,
    generation: std::sync::atomic::AtomicU64,
    hold: Duration,
    tell: Tell,
}

impl Keeper {
    /// A keeper over `port`, holding a copy for `hold`, telling `tell`.
    #[must_use]
    pub fn new(port: Box<dyn ClipboardPort>, hold: Duration, tell: Tell) -> Self {
        Self {
            inner: Arc::new(Inner {
                port: Mutex::new(port),
                held: Mutex::new(None),
                generation: std::sync::atomic::AtomicU64::new(0),
                hold,
                tell,
            }),
        }
    }

    /// Put a secret on the clipboard until the hold runs out, and start the
    /// timer that clears it. Answers when it leaves. A second copy moves the
    /// deadline: the first copy's timer then does nothing.
    ///
    /// # Errors
    /// The platform's refusal, in words.
    pub fn copy(
        &self,
        text: &str,
        what: &str,
        entity_id: Option<&str>,
    ) -> Result<SystemTime, String> {
        self.inner
            .port
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_secret(text)?;
        let generation = self
            .inner
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let wall = SystemTime::now() + self.inner.hold;
        let deadline = Instant::now() + self.inner.hold;
        *self
            .inner
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Held {
            hash: hash(text),
            what: what.to_owned(),
            entity_id: entity_id.map(str::to_owned),
            deadline,
            wall,
            generation,
        });
        (self.inner.tell)(ClipState {
            what: Some(what.to_owned()),
            entity_id: entity_id.map(str::to_owned),
            until: Some(rfc3339(wall)),
        });
        let keeper = self.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { keeper.clear_at_deadline(generation).await });
        }
        Ok(wall)
    }

    /// Wait out the hold, checking both clocks: the monotonic one stands
    /// still while a Mac sleeps, the wall clock does not.
    async fn clear_at_deadline(&self, generation: u64) {
        loop {
            let (deadline, wall) = {
                let held = self
                    .inner
                    .held
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                match held.as_ref() {
                    Some(h) if h.generation == generation => (h.deadline, h.wall),
                    // Copied again, or cleared: this timer is done.
                    _ => return,
                }
            };
            if Instant::now() >= deadline || SystemTime::now() >= wall {
                self.clear_if_ours(Some(generation));
                return;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            tokio::time::sleep(left.min(Duration::from_secs(1))).await;
        }
    }

    /// Clear the clipboard if it still holds what knobas put there; forget it
    /// either way. `generation` limits it to one copy; `None` is any (quit,
    /// *Clear now*). Answers whether it cleared.
    pub fn clear_if_ours(&self, generation: Option<u64>) -> bool {
        let mut held = self
            .inner
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(h) = held.as_ref() else {
            return false;
        };
        if generation.is_some_and(|g| g != h.generation) {
            return false;
        }
        let ours = h.hash;
        *held = None;
        drop(held);
        let mut port = self
            .inner
            .port
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let still = matches!(port.text(), Ok(Some(text)) if hash(&text) == ours);
        let cleared = still && port.clear().is_ok();
        drop(port);
        (self.inner.tell)(ClipState::empty());
        cleared
    }

    /// The person copied something in the window (a username, an address):
    /// whatever knobas put there is gone, so its deadline is dropped and the
    /// clipboard is not touched.
    pub fn forget(&self) {
        let had = self
            .inner
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .is_some();
        if had {
            (self.inner.tell)(ClipState::empty());
        }
    }

    /// What the clipboard holds for knobas now.
    #[must_use]
    pub fn state(&self) -> ClipState {
        let held = self
            .inner
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        held.as_ref().map_or_else(ClipState::empty, |h| ClipState {
            what: Some(h.what.clone()),
            entity_id: h.entity_id.clone(),
            until: Some(rfc3339(h.wall)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clipboard in memory that any "other app" can write to.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Option<String>>>);

    impl ClipboardPort for Fake {
        fn set_secret(&mut self, text: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
        fn text(&mut self) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn clear(&mut self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    fn keeper(hold: Duration) -> (Keeper, Fake, Arc<Mutex<Vec<ClipState>>>) {
        let fake = Fake::default();
        let told = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&told);
        let keeper = Keeper::new(
            Box::new(fake.clone()),
            hold,
            Arc::new(move |state| sink.lock().unwrap().push(state)),
        );
        (keeper, fake, told)
    }

    #[tokio::test]
    async fn a_copy_is_cleared_after_the_hold() {
        let (keeper, fake, told) = keeper(Duration::from_millis(60));
        keeper.copy("hunter2", "password", Some("pw:e1")).unwrap();
        assert_eq!(fake.0.lock().unwrap().as_deref(), Some("hunter2"));
        assert_eq!(keeper.state().what.as_deref(), Some("password"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fake.0.lock().unwrap().as_deref(), None, "the secret stayed");
        assert_eq!(keeper.state(), ClipState::empty());
        let told = told.lock().unwrap();
        assert_eq!(told.first().unwrap().entity_id.as_deref(), Some("pw:e1"));
        assert_eq!(*told.last().unwrap(), ClipState::empty());
    }

    #[tokio::test]
    async fn a_clipboard_the_person_changed_is_left_alone() {
        let (keeper, fake, _) = keeper(Duration::from_millis(60));
        keeper.copy("hunter2", "password", None).unwrap();
        *fake.0.lock().unwrap() = Some("something they copied".into());
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            fake.0.lock().unwrap().as_deref(),
            Some("something they copied")
        );
        assert_eq!(keeper.state(), ClipState::empty());
    }

    #[tokio::test]
    async fn a_second_copy_moves_the_deadline() {
        let (keeper, fake, _) = keeper(Duration::from_millis(150));
        keeper.copy("first", "password", None).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        keeper.copy("second", "one-time code", None).unwrap();
        // The first copy's deadline has passed; the second's has not.
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(fake.0.lock().unwrap().as_deref(), Some("second"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fake.0.lock().unwrap().as_deref(), None);
    }

    #[test]
    fn clear_now_and_quit_clear_only_what_is_ours() {
        let (keeper, fake, _) = keeper(Duration::from_secs(30));
        keeper.copy("hunter2", "password", None).unwrap();
        assert!(keeper.clear_if_ours(None));
        assert_eq!(fake.0.lock().unwrap().as_deref(), None);
        assert!(!keeper.clear_if_ours(None), "nothing left to clear");
        keeper.copy("hunter2", "password", None).unwrap();
        *fake.0.lock().unwrap() = Some("theirs".into());
        assert!(!keeper.clear_if_ours(None));
        assert_eq!(fake.0.lock().unwrap().as_deref(), Some("theirs"));
        // A stale timer's generation clears nothing.
        keeper.copy("again", "password", None).unwrap();
        assert!(!keeper.clear_if_ours(Some(1)));
        assert_eq!(fake.0.lock().unwrap().as_deref(), Some("again"));
        // A plain copy in the window drops the deadline, and leaves the clipboard.
        keeper.forget();
        assert_eq!(keeper.state(), ClipState::empty());
        assert_eq!(fake.0.lock().unwrap().as_deref(), Some("again"));
    }
}
