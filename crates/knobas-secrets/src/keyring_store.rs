//! The real store: one OS-keychain item per source.

use std::sync::{Arc, OnceLock};

use keyring_core::{CredentialStore, Entry, Error as KeyringError};

use crate::{KeychainAccount, Secret, SecretError, SecretStore, decode, encode};

/// The OS keychain, under one service name.
///
/// Its platform store is never opened by `just check` -- see the crate docs.
/// The `#[ignore]`d test in `tests/it/keyring_local.rs` is what proves that
/// half works, run by hand on macOS; the envelope path runs in `just check`
/// over `keyring_core::mock::Store` (the tests' `KeyringStore::over`).
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

    /// A store over a credential store the caller supplies, for tests.
    ///
    /// The tests hand it `keyring_core::mock::Store`, an in-memory store that
    /// keeps what an entry was given, so `put` and `get` run their real
    /// envelope path -- [`encode`] into the item, [`decode`] out of it --
    /// without opening any keychain. `just check` must never touch one.
    #[cfg(test)]
    fn over(service: &str, store: Arc<CredentialStore>) -> Self {
        Self {
            service: service.to_owned(),
            store: OnceLock::from(store),
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

    /// The keychain entry for one account.
    ///
    /// The account arrives **already namespaced** ([`KeychainAccount`]): this
    /// module knows how to reach an item and not which namespaces there are,
    /// the same separation that keeps it from rebuilding the service name.
    fn entry(&self, account: &KeychainAccount) -> Result<Entry, SecretError> {
        self.store()?
            .build(&self.service, account.as_str(), None)
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
        // The store refused access. See `SecretError::Unavailable` for what
        // this does and does not cover -- notably it is *not* the
        // locked-keychain / denied-prompt case on macOS, which arrives as
        // `PlatformFailure` and therefore as `Backend`.
        KeyringError::NoStorageAccess(_) => SecretError::Unavailable,
        other => SecretError::Backend(other.to_string()),
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &KeychainAccount) -> Result<Option<Secret>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(raw) => decode(&raw).map(Some),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(other) => Err(map_error(other)),
        }
    }

    fn put(&self, account: &KeychainAccount, secret: &Secret) -> Result<(), SecretError> {
        self.entry(account)?
            .set_password(&encode(secret)?)
            .map_err(map_error)
    }

    fn delete(&self, account: &KeychainAccount) -> Result<(), SecretError> {
        // Absent is success: `delete_source` runs this after the config row is
        // gone, and a source whose secret was already removed by hand must
        // still delete cleanly (interfaces §3, Delete).
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(other) => Err(map_error(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use knobas_source::instance::Account;

    /// A stand-in for whatever the platform layer boxes up. Its `Display`
    /// carries a marker so the assertions below can tell "the cause was
    /// forwarded" from "a generic message was invented".
    #[derive(Debug)]
    struct Boxed(&'static str);

    impl std::fmt::Display for Boxed {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for Boxed {}

    /// The classification the whole crate's error surface rests on, and until
    /// now the only part of it with no test.
    ///
    /// `NoStorageAccess` is deliberately **not** "locked": see
    /// [`SecretError::Unavailable`]. Pinning it here is what stops a future
    /// edit from quietly folding it into `Backend` and taking a caller's only
    /// signal with it.
    #[test]
    fn a_platform_failure_is_classified_by_what_the_store_said() {
        assert!(matches!(
            map_error(KeyringError::NoEntry),
            SecretError::NotFound
        ));
        assert!(matches!(
            map_error(KeyringError::NoStorageAccess(Box::new(Boxed("refused")))),
            SecretError::Unavailable
        ));
        assert!(matches!(
            map_error(KeyringError::PlatformFailure(Box::new(Boxed("boom")))),
            SecretError::Backend(_)
        ));
    }

    /// **The macOS gap, driven rather than described.**
    ///
    /// Every code below is pushed through `apple-native-keyring-store`'s own
    /// `decode_error`, so this asserts what the platform layer *does*, not what
    /// a comment says it does. The previous version of this test built a
    /// `PlatformFailure(Boxed("errSecInteractionNotAllowed"))` and checked it
    /// came back as `Backend` -- but that string is an inert `Display` payload
    /// that `map_error` never inspects, so the assertion was a duplicate of the
    /// `"boom"` case above and would have stayed green if `apple-native` had
    /// started mapping -25308 to `NoStorageAccess`. That is the exact shape of
    /// a test that fails green.
    ///
    /// If this goes red, the store changed its mapping and
    /// [`SecretError::Unavailable`]'s documentation is now wrong -- fix the doc,
    /// do not relax the test.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_codes_land_where_the_docs_say_they_do() {
        use apple_native_keyring_store::keychain::decode_error;
        use security_framework::base::Error as PlatformError;

        let classify = |code| map_error(decode_error(PlatformError::from_code(code)));

        // The six the legacy-keychain store maps to `NoStorageAccess`.
        for (code, name) in [
            (-61, "write permissions"),
            (-25244, "errSecInvalidOwnerEdit"),
            (-25291, "errSecNotAvailable"),
            (-25292, "errSecReadOnly"),
            (-25294, "errSecNoSuchKeychain"),
            (-25295, "errSecInvalidKeychain"),
        ] {
            assert!(
                matches!(classify(code), SecretError::Unavailable),
                "{name} ({code}) should reach Unavailable"
            );
        }

        assert!(
            matches!(classify(-25300), SecretError::NotFound),
            "errSecItemNotFound is absence, not failure"
        );

        // …and the three a human would call "locked", which do **not**. This is
        // the gap `SecretError::Unavailable`'s docs warn about, and the reason
        // an *Unlock your keychain* affordance cannot hang off that variant
        // alone.
        for (code, name) in [
            (-25308, "errSecInteractionNotAllowed"),
            (-25293, "errSecAuthFailed"),
            (-128, "errSecUserCanceled"),
        ] {
            assert!(
                matches!(classify(code), SecretError::Backend(_)),
                "{name} ({code}) reaches Backend today; if this fails, \
                 apple-native-keyring-store changed its mapping and \
                 SecretError::Unavailable's documentation must be updated"
            );
        }
    }

    /// The cause is forwarded through `Display`, never `Debug`: `Debug` on a
    /// platform error can print the buffer it was decoding.
    #[test]
    fn the_backend_message_forwards_the_cause_without_debug_formatting() {
        let SecretError::Backend(message) =
            map_error(KeyringError::PlatformFailure(Box::new(Boxed("plain-text"))))
        else {
            panic!("a platform failure is a Backend error");
        };
        // keyring-core's own `Display` prefixes the class ("Platform failure:
        // …"); what matters is that the cause reaches us through it.
        assert!(
            message.contains("plain-text"),
            "the cause was dropped: {message}"
        );
        assert!(
            !message.contains("Boxed"),
            "the cause was Debug-formatted, which can print payload bytes: {message}"
        );
    }

    /// What a reader can type into a credential field and the JSON writer
    /// has to escape: a quote, a backslash, a tab. The non-ASCII character
    /// is written unescaped, and is here so it stays that way.
    const NEEDS_ESCAPING: &str = "p@ss\"w0rd \\ back\\slash \t tab ümlaut";

    fn mock_backed() -> KeyringStore {
        KeyringStore::over(
            "dev.knobas.test.mock",
            keyring_core::mock::Store::new().expect("the mock store opens"),
        )
    }

    /// `raw` placed in an item as the bytes a keychain holds, then read back
    /// through `get` -- for items no current `put` would write.
    fn read_stored(raw: &str) -> Secret {
        let store = mock_backed();
        let id = KeychainAccount::source("jira");
        store.entry(&id).unwrap().set_password(raw).unwrap();
        store.get(&id).unwrap().expect("the item is there")
    }

    /// **#1639 (SPI-1).** A secret `put` stores must be one `get` can read.
    /// Through the store's own `put` and `get`, so the envelope is written
    /// and read the way a keychain item is: an escaped secret used to be
    /// stored and then refused on every read as "unreadable envelope".
    #[test]
    fn a_secret_that_needs_escaping_reads_back_after_put() {
        let store = mock_backed();
        let id = KeychainAccount::source("jira");
        store
            .put(&id, &Secret::just(AuthMethod::Pat, NEEDS_ESCAPING))
            .unwrap();
        let got = store.get(&id).unwrap().expect("the item was stored");
        assert_eq!(
            (got.kind, got.value.as_str()),
            (AuthMethod::Pat, NEEDS_ESCAPING)
        );
        assert!(got.account.is_none());
    }

    /// The account half (the `StoredAccount` path) and the secret beside it
    /// carry the same escapes through one item.
    #[test]
    fn an_account_that_needs_escaping_reads_back_after_put() {
        let store = mock_backed();
        let id = KeychainAccount::source("kuma");
        let user = "o\"brien\\ops\tümlaut";
        store
            .put(
                &id,
                &Secret {
                    kind: AuthMethod::ApiToken,
                    value: NEEDS_ESCAPING.into(),
                    account: Some(Account {
                        username: user.into(),
                        password: NEEDS_ESCAPING.into(),
                    }),
                },
            )
            .unwrap();
        let got = store.get(&id).unwrap().expect("the item was stored");
        assert_eq!(got.value, NEEDS_ESCAPING);
        let account = got.account.expect("the account survives the item");
        assert_eq!(
            (account.username.as_str(), account.password.as_str()),
            (user, NEEDS_ESCAPING)
        );
    }

    /// **An item written before the fix** is read by the fixed reader. The
    /// bytes are the literal a pre-fix `put` wrote for `p@ss"w0rd` -- v2,
    /// with the quote escaped -- placed in the store directly rather than
    /// re-encoded, because a re-encoding would only show that the new writer
    /// agrees with the new reader.
    #[test]
    fn a_pre_fix_item_with_an_escaped_quote_reads_as_its_secret() {
        let got = read_stored(r#"{"v":2,"kind":"pat","secret":"p@ss\"w0rd"}"#);
        assert_eq!(
            (got.kind, got.value.as_str()),
            (AuthMethod::Pat, "p@ss\"w0rd")
        );
    }

    /// `kind` is read like the secret: a kind spelled with a JSON escape is
    /// the kind it spells. knobas never writes one -- the names are fixed --
    /// so this is the reader's half of the claim, from a literal item.
    #[test]
    fn a_kind_spelled_with_an_escape_reads_as_that_kind() {
        let got = read_stored(r#"{"v":2,"kind":"api\u005ftoken","secret":"x"}"#);
        assert_eq!((got.kind, got.value.as_str()), (AuthMethod::ApiToken, "x"));
    }
}
