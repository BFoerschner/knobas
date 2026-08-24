//! The real store: one OS-keychain item per source.

use std::sync::{Arc, OnceLock};

use keyring_core::{CredentialStore, Entry, Error as KeyringError};

use crate::{Secret, SecretError, SecretStore, account_for, decode, encode};

/// The OS keychain, under one service name.
///
/// Never exercised by `just check` -- see the crate docs. The `#[ignore]`d test
/// in `tests/keyring_local.rs` is what proves this half works, run by hand on
/// macOS.
///
/// The service name is **taken, not computed**: `new` is handed whatever
/// `knobas_app::Profile::keychain_service()` produced and keeps it verbatim.
/// Recomputing it here is how the `.demo` suffix stops separating anything
/// (ruling P13), so this module deliberately knows nothing about how the name
/// is built -- a property `the_service_name_is_never_rebuilt_in_this_crate`
/// pins.
pub struct KeyringStore {
    service: String,
    /// Opened on first use, not in `new`.
    ///
    /// Two reasons, and the first is a hard rule: `just check` must never touch
    /// a real keychain, and a test may legitimately want to build a store to
    /// check the service name it kept. The second is that a platform store can
    /// fail to open (no session keyring in a container), and a constructor that
    /// returned a `Result` for that would push the failure onto every caller
    /// long before anyone asked for a credential.
    store: OnceLock<Arc<CredentialStore>>,
}

impl KeyringStore {
    /// A store writing under `service`.
    ///
    /// `service` is `knobas_app::Profile::keychain_service()`'s output -- the
    /// bundle identifier, `.dev` under `debug_assertions`, `.demo` on top of
    /// either (interfaces §3) -- or the per-run test service the macOS-local
    /// test uses. Kept exactly as given, and deliberately not spelled out here:
    /// see the type's docs.
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            store: OnceLock::new(),
        }
    }

    /// The service name this store writes under. For diagnostics only -- it is
    /// not a secret, and printing it is how a "why did it re-prompt" question
    /// gets answered.
    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    /// The platform credential store, opened once.
    ///
    /// A failed open is not memoized: a keyring that was unavailable at first
    /// call may be available at the next, and caching the failure would make
    /// one bad moment permanent for the life of the process.
    fn store(&self) -> Result<&Arc<CredentialStore>, SecretError> {
        if let Some(store) = self.store.get() {
            return Ok(store);
        }
        let opened = platform_store()?;
        Ok(self.store.get_or_init(|| opened))
    }

    fn entry(&self, source_id: &str) -> Result<Entry, SecretError> {
        self.store()?
            .build(&self.service, &account_for(source_id), None)
            .map_err(map_error)
    }
}

/// The credential store for this platform.
///
/// Chosen per target rather than through `keyring`'s all-in-one facade: that
/// crate's only compiling feature sets turn on the secret-service/D-Bus backend
/// for every non-Apple unix, and the Linux CI build must pull neither. See the
/// manifest.
#[cfg(target_os = "macos")]
fn platform_store() -> Result<Arc<CredentialStore>, SecretError> {
    let store = apple_native_keyring_store::keychain::Store::new().map_err(map_error)?;
    Ok(store as Arc<CredentialStore>)
}

#[cfg(target_os = "windows")]
fn platform_store() -> Result<Arc<CredentialStore>, SecretError> {
    let store = windows_native_keyring_store::Store::new().map_err(map_error)?;
    Ok(store as Arc<CredentialStore>)
}

#[cfg(target_os = "linux")]
fn platform_store() -> Result<Arc<CredentialStore>, SecretError> {
    let store = linux_keyutils_keyring_store::Store::new().map_err(map_error)?;
    Ok(store as Arc<CredentialStore>)
}

/// Everything else. knobas is a macOS-first desktop app; the point of this arm
/// is that an unsupported platform says so at the moment a credential is asked
/// for, rather than failing to build.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_store() -> Result<Arc<CredentialStore>, SecretError> {
    Err(SecretError::Backend(
        "no OS credential store on this platform".to_owned(),
    ))
}

/// Translate a keyring failure without ever quoting the payload.
///
/// `keyring_core`'s own `Display` is careful about this -- `BadEncoding` and
/// `BadDataFormat` describe the problem and print no bytes -- which is why
/// deferring to it is safe here.
fn map_error(error: KeyringError) -> SecretError {
    match error {
        KeyringError::NoEntry => SecretError::NotFound,
        // The platform has the item but would not hand it over: on macOS this
        // is the locked-keychain / denied-prompt case, which is a thing the
        // user can fix and must be told about as such.
        KeyringError::NoStorageAccess(_) => SecretError::Locked,
        other => SecretError::Backend(other.to_string()),
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError> {
        match self.entry(source_id)?.get_password() {
            Ok(raw) => decode(&raw).map(Some),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(other) => Err(map_error(other)),
        }
    }

    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError> {
        self.entry(source_id)?
            .set_password(&encode(secret)?)
            .map_err(map_error)
    }

    fn delete(&self, source_id: &str) -> Result<(), SecretError> {
        // Absent is success: `delete_source` runs this after the config row is
        // gone, and a source whose secret was already removed by hand must
        // still delete cleanly (interfaces §3, Delete).
        match self.entry(source_id)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(other) => Err(map_error(other)),
        }
    }
}
