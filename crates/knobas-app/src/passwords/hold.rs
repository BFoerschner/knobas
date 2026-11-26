//! Where a shown secret lives (ADR-0056): in the app process, for at most
//! [`HOLD_SECS`](super::HOLD_SECS), and nowhere else.
//!
//! A shown value is purged at its deadline, on *Hide*, when the page is left,
//! when its source is rebuilt and when the app quits. The deadline is checked
//! against both the monotonic and the wall clock, because the monotonic one
//! stands still while a Mac sleeps. A copy of a value that is shown takes it
//! from here and asks the source nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use knobas_source::passwords::{SecretRef, SecretText};

struct Shown {
    value: SecretText,
    deadline: Instant,
    wall: SystemTime,
}

impl Shown {
    fn live(&self, now: Instant, wall: SystemTime) -> bool {
        now < self.deadline && wall < self.wall
    }
}

/// The hold: shown secrets by entry and which secret.
#[derive(Clone)]
pub struct Hold {
    inner: Arc<Mutex<HashMap<(String, SecretRef), Shown>>>,
    hold: Duration,
}

impl Hold {
    #[must_use]
    pub fn new(hold: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            hold,
        }
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<(String, SecretRef), Shown>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keep `value` as shown until the hold runs out, and start the purge.
    /// Answers when it hides itself.
    pub fn show(&self, entity: &str, which: &SecretRef, value: SecretText) -> SystemTime {
        let wall = SystemTime::now() + self.hold;
        self.map().insert(
            (entity.to_owned(), which.clone()),
            Shown {
                value,
                deadline: Instant::now() + self.hold,
                wall,
            },
        );
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let me = self.clone();
            runtime.spawn(async move {
                loop {
                    me.purge();
                    if me.map().is_empty() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            });
        }
        wall
    }

    /// The shown value of `which`, while it is shown.
    #[must_use]
    pub fn get(&self, entity: &str, which: &SecretRef) -> Option<SecretText> {
        self.purge();
        self.map()
            .get(&(entity.to_owned(), which.clone()))
            .map(|s| s.value.clone())
    }

    /// Hide one.
    pub fn hide(&self, entity: &str, which: &SecretRef) {
        self.map().remove(&(entity.to_owned(), which.clone()));
    }

    /// Hide every secret of a source: its adapter was rebuilt.
    pub fn hide_source(&self, source: &str) {
        let prefix = format!("{source}:");
        self.map()
            .retain(|(entity, _), _| !entity.starts_with(&prefix));
    }

    /// Hide everything: the page was left, or the app quits.
    pub fn hide_all(&self) {
        self.map().clear();
    }

    /// Drop what is past its deadline on either clock.
    pub fn purge(&self) {
        let (now, wall) = (Instant::now(), SystemTime::now());
        self.map().retain(|_, shown| shown.live(now, wall));
    }

    /// How many secrets are shown now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.purge();
        self.map().len()
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_shown_secret_is_gone_after_the_hold() {
        let hold = Hold::new(Duration::from_millis(80));
        hold.show("pw:e1", &SecretRef::Password, SecretText::new("hunter2"));
        assert_eq!(
            hold.get("pw:e1", &SecretRef::Password)
                .map(|v| v.expose().to_owned()),
            Some("hunter2".to_owned())
        );
        assert!(
            hold.get("pw:e1", &SecretRef::Field { name: "PIN".into() })
                .is_none()
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            hold.get("pw:e1", &SecretRef::Password).is_none(),
            "the hold outlived its deadline"
        );
        // The purge ran on its own, not only at a read.
        assert_eq!(hold.inner.lock().unwrap().len(), 0);
    }

    #[test]
    fn hide_hide_source_and_hide_all() {
        let hold = Hold::new(Duration::from_secs(30));
        hold.show("pw:e1", &SecretRef::Password, SecretText::new("a"));
        hold.show("pw:e2", &SecretRef::Password, SecretText::new("b"));
        hold.show("pw2:e1", &SecretRef::Password, SecretText::new("c"));
        assert!(!hold.is_empty());
        hold.hide("pw:e1", &SecretRef::Password);
        assert_eq!(hold.len(), 2);
        hold.hide_source("pw");
        assert_eq!(hold.len(), 1);
        assert!(hold.get("pw2:e1", &SecretRef::Password).is_some());
        hold.hide_all();
        assert!(hold.is_empty());
    }

    #[test]
    fn either_clock_ends_the_hold() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let shown = Shown {
            value: SecretText::new("x"),
            deadline: now + Duration::from_secs(30),
            wall: wall + Duration::from_secs(30),
        };
        assert!(shown.live(now, wall));
        // The Mac slept: the monotonic clock stood still, the wall clock did not.
        assert!(!shown.live(now, wall + Duration::from_secs(31)));
        assert!(!shown.live(now + Duration::from_secs(31), wall));
        // At its deadline on either clock it is gone.
        assert!(!shown.live(now + Duration::from_secs(30), wall));
        assert!(!shown.live(now, wall + Duration::from_secs(30)));
    }
}
