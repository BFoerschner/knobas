//! The one place a knobas credential is stored, and the seam that keeps it out
//! of `just check`.
//!
//! Spec §14: nothing secret ever reaches Postgres. `knobas.source_config` holds
//! the auth *method*, the base URL and non-secret config; the secret itself is
//! one OS-keychain item per source (interfaces §3).
//!
//! ```text
//! service = "dev.knobas.desktop"            release builds
//!         = "dev.knobas.desktop.dev"        cfg!(debug_assertions)
//!         = ….demo                          the --demo profile (P13)
//! account = "source:<source_id>"
//! value   = {"v":1,"kind":"pat","secret":"…"}
//! ```
//!
//! One item per source rather than per auth method, so changing PAT → password
//! rewrites in place instead of orphaning an item; the envelope's `v` and
//! `kind` leave room for OAuth (access + refresh + expiry) without a naming
//! change.
//!
//! # Who names the service
//!
//! **`knobas_app::Profile::keychain_service()`, and nothing else.** That table
//! above is documentation of what it produces, not a second implementation:
//! [`KeyringStore::new`] takes the finished string and keeps it verbatim. A
//! copy of the rule here would be a copy that can lose the `.demo` suffix
//! while still looking right, and a demo run that silently reads the real
//! credentials is precisely what P13 exists to prevent. `Profile` lives in
//! `knobas-app`, which depends on this crate, so it could not be imported here
//! even if that were wanted.
//!
//! # Why the trait
//!
//! CI runs on Linux, where a real keychain needs a live secret service over
//! D-Bus, and no automated test may ever prompt a developer's macOS keychain.
//! So the store is a trait, [`MemoryStore`] is what every test and every CI run
//! gets, and [`KeyringStore`] is exercised by one `#[ignore]`d macOS-local test.

pub mod keyring_store;
pub mod memory;
pub mod spawn;

pub use keyring_store::KeyringStore;
pub use memory::MemoryStore;

use knobas_source::AuthMethod;

/// The keychain account for one source.
#[must_use]
pub fn account_for(source_id: &str) -> String {
    format!("source:{source_id}")
}

/// One credential. `value` is the PAT, password or API token itself.
#[derive(Clone)]
pub struct Secret {
    pub kind: AuthMethod,
    pub value: String,
}

/// Hand-written, and the reason is the whole point of the type: a derived
/// `Debug` puts the credential into every `tracing` line and every panic
/// message that ever formats a struct containing one.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("kind", &self.kind)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Why a credential could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform store failed, or handed back something this version cannot
    /// read. Carries no secret material.
    #[error("keychain: {0}")]
    Backend(String),
    /// The store is there but refused access to it.
    ///
    /// **Not "the keychain is locked", however much it sounds like it.** This
    /// is exactly `keyring_core::Error::NoStorageAccess`, and on macOS the
    /// legacy-keychain store raises that for only six `OSStatus` values:
    /// `-61` (write permissions), `errSecInvalidOwnerEdit`,
    /// `errSecNotAvailable`, `errSecReadOnly`, `errSecNoSuchKeychain` and
    /// `errSecInvalidKeychain`.
    ///
    /// The three cases a *human* would call "locked" are **not** among them --
    /// `errSecInteractionNotAllowed` (-25308, locked with no UI available),
    /// `errSecAuthFailed` (-25293) and `errSecUserCanceled` (-128, the user
    /// clicked *Deny*) all fall through to
    /// [`Backend`](SecretError::Backend). So an *Unlock your keychain*
    /// affordance hung off this variant alone would be dead code on the
    /// platform it was written for; a caller that wants one has to treat
    /// `Backend` as possibly-lockable too, or downcast the platform error.
    #[error("the credential store refused access")]
    Unavailable,
    /// Asked for an item that is not there, from an operation that needed one.
    /// [`SecretStore::get`] reports absence as `Ok(None)` instead.
    #[error("no stored credential")]
    NotFound,
}

/// Where knobas keeps credentials.
///
/// Deliberately **synchronous**: the platform APIs are blocking, and pretending
/// otherwise would hide the fact that a locked keychain waits on a human. Async
/// callers go through [`spawn`], never straight through this trait.
pub trait SecretStore: Send + Sync {
    /// The credential for `source_id`, or `None` if there is none.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed -- absence is not a failure.
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError>;

    /// Store `secret` for `source_id`, replacing whatever was there.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed.
    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError>;

    /// Remove the credential for `source_id`. **Absent is success**: deleting a
    /// source must not fail because its secret was already gone.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed for any other reason.
    fn delete(&self, source_id: &str) -> Result<(), SecretError>;
}

/// Current envelope version. Bumped only when the shape changes; a reader that
/// meets a higher one refuses rather than guessing.
const ENVELOPE_VERSION: u8 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope<'a> {
    v: u8,
    kind: &'a str,
    secret: &'a str,
}

/// The envelope name for an auth method.
///
/// **No wildcard arm, deliberately** -- the same reason `WriteOp::identifier`
/// has none: adding an `AuthMethod` variant must stop this module compiling
/// until the variant is given a name here, rather than silently storing every
/// new method under a name that round-trips to the wrong one.
fn envelope_kind(kind: AuthMethod) -> &'static str {
    match kind {
        AuthMethod::UserPassword => "user_password",
        AuthMethod::Pat => "pat",
        AuthMethod::ApiToken => "api_token",
        AuthMethod::OAuth => "oauth",
    }
}

fn kind_from_envelope(name: &str) -> Option<AuthMethod> {
    match name {
        "user_password" => Some(AuthMethod::UserPassword),
        "pat" => Some(AuthMethod::Pat),
        "api_token" => Some(AuthMethod::ApiToken),
        "oauth" => Some(AuthMethod::OAuth),
        _ => None,
    }
}

/// Serialize a secret into the stored envelope.
fn encode(secret: &Secret) -> Result<String, SecretError> {
    serde_json::to_string(&Envelope {
        v: ENVELOPE_VERSION,
        kind: envelope_kind(secret.kind),
        secret: &secret.value,
    })
    .map_err(|e| SecretError::Backend(e.to_string()))
}

/// Parse a stored envelope. The error text never quotes the payload.
fn decode(raw: &str) -> Result<Secret, SecretError> {
    let envelope: Envelope<'_> = serde_json::from_str(raw)
        .map_err(|_| SecretError::Backend("unreadable envelope".into()))?;
    if envelope.v != ENVELOPE_VERSION {
        return Err(SecretError::Backend(format!(
            "envelope version {} was written by a newer knobas",
            envelope.v
        )));
    }
    let kind = kind_from_envelope(envelope.kind)
        .ok_or_else(|| SecretError::Backend(format!("unknown auth kind {:?}", envelope.kind)))?;
    Ok(Secret {
        kind,
        value: envelope.secret.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;

    /// `account` is half of the keychain convention (interfaces §3) and is
    /// load-bearing: changing it strands every stored credential.
    #[test]
    fn the_account_is_the_source_id_prefixed() {
        assert_eq!(account_for("jira-eu"), "source:jira-eu");
    }

    /// The envelope is versioned and carries the auth method, so PAT →
    /// password rewrites one item instead of orphaning another (§3), and OAuth
    /// can add fields later without a naming change.
    #[test]
    fn the_envelope_round_trips_and_names_its_version() {
        let secret = Secret {
            kind: AuthMethod::Pat,
            value: "abc123".into(),
        };
        let json = encode(&secret).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({ "v": 1, "kind": "pat", "secret": "abc123" })
        );
        let back = decode(&json).unwrap();
        assert_eq!(back.kind, AuthMethod::Pat);
        assert_eq!(back.value, "abc123");
    }

    #[test]
    fn every_auth_method_has_an_envelope_name_that_round_trips() {
        for kind in [
            AuthMethod::UserPassword,
            AuthMethod::Pat,
            AuthMethod::ApiToken,
            AuthMethod::OAuth,
        ] {
            let s = Secret {
                kind,
                value: "x".into(),
            };
            assert_eq!(decode(&encode(&s).unwrap()).unwrap().kind, kind);
        }
    }

    /// An envelope from a future knobas is not a secret we may guess at.
    #[test]
    fn an_unknown_envelope_version_is_refused_rather_than_misread() {
        let err = decode(r#"{"v":2,"kind":"pat","secret":"x"}"#).unwrap_err();
        assert!(matches!(err, SecretError::Backend(_)), "got {err:?}");
    }

    /// A secret that prints itself is a secret in a log file.
    #[test]
    fn debug_never_prints_the_value() {
        let s = Secret {
            kind: AuthMethod::Pat,
            value: "hunter2".into(),
        };
        let shown = format!("{s:?}");
        assert!(
            !shown.contains("hunter2"),
            "Debug leaked the secret: {shown}"
        );
        assert!(shown.contains("redacted"));
    }

    /// The store contract, exercised against the implementation every test and
    /// every CI run uses. [`KeyringStore`] runs the same assertions in the
    /// `#[ignore]`d macOS-local test.
    #[test]
    fn a_memory_store_honours_the_store_contract() {
        let store = MemoryStore::new();
        assert!(store.get("jira").unwrap().is_none());
        // Deleting something absent is success, not an error: `delete_source`
        // must not fail because the secret was already gone.
        store.delete("jira").unwrap();

        store
            .put(
                "jira",
                &Secret {
                    kind: AuthMethod::Pat,
                    value: "one".into(),
                },
            )
            .unwrap();
        assert_eq!(store.get("jira").unwrap().unwrap().value, "one");

        // Re-entering overwrites the one item rather than adding a second.
        store
            .put(
                "jira",
                &Secret {
                    kind: AuthMethod::UserPassword,
                    value: "two".into(),
                },
            )
            .unwrap();
        let got = store.get("jira").unwrap().unwrap();
        assert_eq!(
            (got.kind, got.value.as_str()),
            (AuthMethod::UserPassword, "two")
        );

        store.delete("jira").unwrap();
        assert!(store.get("jira").unwrap().is_none());
    }

    /// Two sources are two items; deleting one leaves the other.
    #[test]
    fn sources_do_not_share_an_item() {
        let store = MemoryStore::new();
        store
            .put(
                "jira",
                &Secret {
                    kind: AuthMethod::Pat,
                    value: "a".into(),
                },
            )
            .unwrap();
        store
            .put(
                "jira-eu",
                &Secret {
                    kind: AuthMethod::Pat,
                    value: "b".into(),
                },
            )
            .unwrap();
        store.delete("jira").unwrap();
        assert!(store.get("jira").unwrap().is_none());
        assert_eq!(store.get("jira-eu").unwrap().unwrap().value, "b");
    }

    /// The async callers all go through `spawn_blocking`, because a locked
    /// keychain blocks on a user prompt for as long as the user takes.
    #[tokio::test]
    async fn the_spawn_helpers_reach_the_store() {
        let store: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
        spawn::put(
            &store,
            "jira",
            Secret {
                kind: AuthMethod::Pat,
                value: "z".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            spawn::get(&store, "jira").await.unwrap().unwrap().value,
            "z"
        );
        spawn::delete(&store, "jira").await.unwrap();
        assert!(spawn::get(&store, "jira").await.unwrap().is_none());
    }

    /// **The `.demo` suffix must survive.** [`KeyringStore`] is handed the
    /// service name that `knobas_app::Profile::keychain_service()` produced and
    /// stores it *verbatim*; a constructor that recomputed it from a profile
    /// flag is how the demo suffix silently stops separating anything and a
    /// demo run reaches the real credentials (ruling P13).
    ///
    /// Safe inside `just check`: building a store touches no keychain — the
    /// platform store is opened lazily, on the first `get`/`put`/`delete`.
    #[test]
    fn a_keyring_store_keeps_the_service_name_it_was_given() {
        for service in [
            "dev.knobas.desktop",
            "dev.knobas.desktop.dev",
            "dev.knobas.desktop.dev.demo",
            "dev.knobas.desktop.test.4711",
        ] {
            assert_eq!(KeyringStore::new(service).service(), service);
        }
    }

    /// …and it must be *impossible* to rebuild it here.
    ///
    /// The other half of the pin above: this crate deliberately holds no
    /// `Profile`, no `service_name()` and no copy of the bundle identifier, so
    /// there is exactly one producer of the service name
    /// (`knobas_app::Profile::keychain_service()`) and no second one to drift
    /// from it. Asserted over the file that would host such a helper.
    ///
    /// The stem is assembled at run time so that this assertion is not
    /// defeated by its own source text.
    #[test]
    fn the_service_name_is_never_rebuilt_in_this_crate() {
        let stem = ["dev", "knobas", "desktop"].join(".");
        let store_source = include_str!("keyring_store.rs");
        assert!(
            !store_source.contains(&stem),
            "keyring_store.rs mentions {stem:?}: the service name has one \
             producer (knobas_app::Profile::keychain_service) and this crate \
             is not it"
        );
    }
}
