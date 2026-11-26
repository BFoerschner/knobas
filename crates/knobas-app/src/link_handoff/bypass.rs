//! Short-lived one-shot *Open in browser* bypass store (ADR-0061).
//!
//! When knobas runs *Open in browser* for an external URL, it records one
//! short-lived pass here before handing the URL to the OS opener. When the
//! Firefox extension sees that navigation arrive, it asks knobas whether a
//! one-shot bypass is waiting and consumes it so the page opens in Firefox
//! instead of bouncing straight back into knobas.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::link_handoff::normalize_web_url;

/// How long a recorded *Open in browser* bypass stays valid.
///
/// Long enough for a cold browser launch and a first redirect hop, short enough
/// that clicking the same link later in chat still hands off to knobas.
pub const BYPASS_TTL: Duration = Duration::from_secs(15);

/// Maximum live bypass entries held in memory across all URLs.
pub const MAX_ENTRIES: usize = 128;

/// In-memory store of short-lived one-shot bypasses, keyed by normalized URL.
#[derive(Debug, Default)]
pub struct BypassStore {
    entries: Mutex<HashMap<String, Vec<Instant>>>,
}

impl BypassStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a one-shot bypass for `url` using the default [`BYPASS_TTL`].
    ///
    /// Returns `true` when `url` is a valid `http`/`https` URL and was
    /// recorded, `false` otherwise.
    pub fn record(&self, url: &str) -> bool {
        self.record_at(url, Instant::now(), BYPASS_TTL)
    }

    /// Record a one-shot bypass at an explicit `now` and `ttl` (for tests).
    pub fn record_at(&self, url: &str, now: Instant, ttl: Duration) -> bool {
        let Some(key) = normalize_web_url(url) else {
            return false;
        };
        let expires_at = now + ttl;
        let mut map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut map, now);
        let total: usize = map.values().map(Vec::len).sum();
        if total >= MAX_ENTRIES {
            evict_oldest(&mut map);
        }
        map.entry(key).or_default().push(expires_at);
        true
    }

    /// Check whether `url` has a live bypass; if `consume` is `true`, remove
    /// one matching entry so subsequent navigations are not bypassed.
    #[must_use]
    pub fn check(&self, url: &str, consume: bool) -> bool {
        self.check_at(url, consume, Instant::now())
    }

    /// Check (and optionally consume) at an explicit `now` (for tests).
    #[must_use]
    pub fn check_at(&self, url: &str, consume: bool, now: Instant) -> bool {
        let Some(key) = normalize_web_url(url) else {
            return false;
        };
        let mut map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut map, now);
        let Some(times) = map.get_mut(&key) else {
            return false;
        };
        if times.is_empty() {
            map.remove(&key);
            return false;
        }
        if consume {
            times.remove(0);
            if times.is_empty() {
                map.remove(&key);
            }
        }
        true
    }
}

fn prune(map: &mut HashMap<String, Vec<Instant>>, now: Instant) {
    map.retain(|_, times| {
        times.retain(|&expires_at| expires_at > now);
        !times.is_empty()
    });
}

fn evict_oldest(map: &mut HashMap<String, Vec<Instant>>) {
    let oldest_key = map
        .iter()
        .filter_map(|(k, times)| times.first().map(|&t| (k.clone(), t)))
        .min_by_key(|(_, t)| *t)
        .map(|(k, _)| k);
    if let Some(key) = oldest_key
        && let Some(times) = map.get_mut(&key)
    {
        times.remove(0);
        if times.is_empty() {
            map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_is_one_shot_and_normalizes_fragment_case_and_trailing_slash() {
        let store = BypassStore::new();
        let t0 = Instant::now();
        assert!(store.record_at(
            "https://JIRA.Example/browse/PAY-231/#comment-4",
            t0,
            BYPASS_TTL
        ));

        // Peek does not consume.
        assert!(store.check_at("https://jira.example/browse/PAY-231", false, t0));
        // First consume matches.
        assert!(store.check_at(
            "HTTPS://jira.example/browse/PAY-231#other",
            true,
            t0 + Duration::from_secs(1)
        ));
        // Second consume misses: one-shot.
        assert!(!store.check_at(
            "https://jira.example/browse/PAY-231",
            true,
            t0 + Duration::from_secs(2)
        ));
    }

    #[test]
    fn bypass_expires_after_ttl_and_keeps_query_distinct() {
        let store = BypassStore::new();
        let t0 = Instant::now();
        assert!(store.record_at(
            "https://wiki.example/pages/viewpage.action?pageId=42",
            t0,
            Duration::from_secs(5)
        ));
        // Different query string must not match.
        assert!(!store.check_at(
            "https://wiki.example/pages/viewpage.action?pageId=99",
            true,
            t0 + Duration::from_secs(1)
        ));
        // Expired at t0 + 5s.
        assert!(!store.check_at(
            "https://wiki.example/pages/viewpage.action?pageId=42",
            true,
            t0 + Duration::from_secs(5)
        ));
    }

    #[test]
    fn bypass_rejects_non_http_urls() {
        let store = BypassStore::new();
        assert!(!store.record("file:///etc/passwd"));
        assert!(!store.record("javascript:alert(1)"));
        assert!(!store.record("knobas://open?url=https://jira.example"));
    }
}
