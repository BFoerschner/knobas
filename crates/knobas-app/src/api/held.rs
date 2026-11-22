//! Secrets typed on the explorer's page (ux31; Björn: "I'd just type it into
//! the openapi viewer page itself. not using stored credentials"): held in the
//! app process until knobas quits, keyed by the origin they were typed for and
//! the security scheme they answer, and never answered to the web view.
//!
//! A [`Held`] prints `<redacted>` and zeroes its bytes when dropped; the map
//! prints its origins and schemes only.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

/// A secret and, for HTTP Basic, the user name beside it.
#[derive(Clone)]
pub struct Held {
    secret: String,
    user: Option<String>,
}

impl Held {
    #[must_use]
    pub fn new(secret: String, user: Option<String>) -> Self {
        Self { secret, user }
    }

    /// The secret, for the one place that uses it: the request's header or query.
    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// The user name, for HTTP Basic.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
}

impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.secret);
        if let Some(user) = &mut self.user {
            zeroize::Zeroize::zeroize(user);
        }
    }
}

/// The secrets held, by origin (`https://api.example.com`) and scheme name.
#[derive(Default)]
pub struct HeldSecrets {
    map: HashMap<(String, String), Held>,
}

impl std::fmt::Debug for HeldSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut keys: Vec<_> = self
            .map
            .keys()
            .map(|(origin, scheme)| format!("{origin} {scheme}"))
            .collect();
        keys.sort();
        f.debug_struct("HeldSecrets").field("held", &keys).finish()
    }
}

impl HeldSecrets {
    /// Holds `held` for `origin` and `scheme`, in place of any held before.
    pub fn hold(&mut self, origin: &str, scheme: &str, held: Held) {
        self.map
            .insert((origin.to_owned(), scheme.to_owned()), held);
    }

    /// The secret held for `origin` and `scheme`.
    #[must_use]
    pub fn get(&self, origin: &str, scheme: &str) -> Option<&Held> {
        self.map.get(&(origin.to_owned(), scheme.to_owned()))
    }

    /// Which schemes `origin` holds a secret for.
    #[must_use]
    pub fn schemes(&self, origin: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .map
            .keys()
            .filter(|(o, _)| o == origin)
            .map(|(_, s)| s.clone())
            .collect();
        out.sort();
        out
    }

    /// Forgets every secret held for `origin`.
    pub fn forget(&mut self, origin: &str) {
        self.map.retain(|(o, _), _| o != origin);
    }
}

/// The app's held secrets, for as long as it runs.
pub static HELD: LazyLock<Mutex<HeldSecrets>> = LazyLock::new(Mutex::default);

/// The held secrets, locked; a poisoned lock is still the map.
pub fn held() -> std::sync::MutexGuard<'static, HeldSecrets> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{Held, HeldSecrets};

    #[test]
    fn a_secret_is_held_by_origin_and_scheme_and_never_printed() {
        let mut map = HeldSecrets::default();
        map.hold(
            "https://api.example.com",
            "bearer",
            Held::new("s3cret-token".into(), None),
        );
        map.hold(
            "https://api.example.com",
            "basic",
            Held::new("pw".into(), Some("mara".into())),
        );
        assert_eq!(
            map.get("https://api.example.com", "bearer")
                .map(Held::secret),
            Some("s3cret-token")
        );
        assert!(map.get("https://api.example.com:8443", "bearer").is_none());
        assert!(map.get("http://api.example.com", "bearer").is_none());
        assert_eq!(map.schemes("https://api.example.com"), ["basic", "bearer"]);
        let printed = format!("{map:?} {:?}", map.get("https://api.example.com", "basic"));
        assert!(
            !printed.contains("s3cret") && !printed.contains("mara") && !printed.contains("pw\""),
            "{printed}"
        );
        map.forget("https://api.example.com");
        assert!(map.schemes("https://api.example.com").is_empty());
    }
}
