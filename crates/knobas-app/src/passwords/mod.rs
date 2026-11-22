//! The Passwords role in the app (ADR-0056): codes made on this machine, the
//! hold a shown secret lives in, the clipboard a copied one goes to, and the
//! page's reads.
//!
//! **House rule:** a secret ([`knobas_source::passwords::SecretText`]) is
//! named in this module, in `commands/passwords.rs` and in the adapters, and
//! nowhere else in the app. It is never put in a table, a file, a log line, a
//! `tracing` field, the read cache (ADR-0032) or the held reads (ADR-0042).

pub mod clipboard;
pub mod hold;
pub mod otp;
pub mod reads;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use knobas_secrets::otp::OtpKeys;

/// How long a shown secret stays shown, and a copied one stays on the
/// clipboard. Mirrored in the window's `tf/passwords/hold.ts`.
pub const HOLD_SECS: u64 = knobas_source::passwords::HOLD_SECS;

/// What the app keeps for the Passwords page: one per process.
pub struct PasswordsState {
    pub hold: hold::Hold,
    pub clipboard: clipboard::Keeper,
    /// Sources a live read just found down, and when: until a call succeeds.
    down: Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>,
    /// Each source's code keys as read from the keychain, and the run they
    /// were read after: read once a session and again after each run.
    keys: Mutex<HashMap<String, KeysRead>>,
}

/// A source's code keys and the run they were read after.
type KeysRead = (Arc<OtpKeys>, Option<chrono::DateTime<chrono::Utc>>);

impl PasswordsState {
    /// The state over `clipboard`, telling `tell` what it holds.
    #[must_use]
    pub fn new(port: Box<dyn clipboard::ClipboardPort>, tell: clipboard::Tell) -> Self {
        let hold = Duration::from_secs(HOLD_SECS);
        Self {
            hold: hold::Hold::new(hold),
            clipboard: clipboard::Keeper::new(port, hold, tell),
            down: Mutex::new(HashMap::new()),
            keys: Mutex::new(HashMap::new()),
        }
    }

    /// The sources found down, for the page's read.
    #[must_use]
    pub fn down(&self) -> HashMap<String, chrono::DateTime<chrono::Utc>> {
        self.down
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A live read of `source` did not reach it.
    pub fn mark_down(&self, source: &str) {
        self.down
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(source.to_owned(), chrono::Utc::now());
    }

    /// A call to `source` succeeded.
    pub fn mark_up(&self, source: &str) {
        self.down
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(source);
    }

    /// The keys kept for `source`, if they were read after `run`.
    #[must_use]
    pub fn cached_keys(
        &self,
        source: &str,
        run: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Option<Arc<OtpKeys>> {
        let keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.get(source)
            .filter(|(_, after)| *after == run)
            .map(|(k, _)| Arc::clone(k))
    }

    /// Keep `keys`, read after `run`.
    pub fn keep_keys(
        &self,
        source: &str,
        run: Option<chrono::DateTime<chrono::Utc>>,
        keys: OtpKeys,
    ) -> Arc<OtpKeys> {
        let keys = Arc::new(keys);
        self.keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(source.to_owned(), (Arc::clone(&keys), run));
        keys
    }

    /// Forget everything kept of `source`: it was removed or rebuilt.
    pub fn forget_source(&self, source: &str) {
        self.hold.hide_source(source);
        self.keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(source);
        self.mark_up(source);
    }

    /// The quit: hide what is shown and take back what is on the clipboard,
    /// if it is still knobas's.
    pub fn quit(&self) {
        self.hold.hide_all();
        self.clipboard.clear_if_ours(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The house rule above, held: no module of the app outside this one,
    /// `commands/passwords.rs` and the IPC mirror names a secret's type.
    #[test]
    fn no_other_module_names_a_secret() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        let allowed = |p: &std::path::Path| {
            let rel = p
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            rel.starts_with("passwords/")
                || rel == "commands/passwords.rs"
                || rel == "ipc_mirror.rs"
        };
        let offenders: Vec<String> = files
            .iter()
            .filter(|p| !allowed(p))
            .filter(|p| std::fs::read_to_string(p).unwrap().contains("SecretText"))
            .map(|p| p.display().to_string())
            .collect();
        assert!(
            offenders.is_empty(),
            "a secret is named outside the passwords modules: {offenders:?}"
        );
    }

    #[test]
    fn keys_are_read_again_after_a_newer_run() {
        let state = PasswordsState::new(
            Box::new(clipboard::SystemClipboard::default()),
            Arc::new(|_| {}),
        );
        let first = Some(chrono::Utc::now());
        assert!(state.cached_keys("pw", first).is_none());
        state.keep_keys("pw", first, OtpKeys::new());
        assert!(state.cached_keys("pw", first).is_some());
        let later = first.map(|t| t + chrono::Duration::seconds(5));
        assert!(
            state.cached_keys("pw", later).is_none(),
            "a newer run reads the keys again"
        );
        state.mark_down("pw");
        assert!(state.down().contains_key("pw"));
        state.forget_source("pw");
        assert!(state.down().is_empty());
        assert!(state.cached_keys("pw", first).is_none());
    }
}
