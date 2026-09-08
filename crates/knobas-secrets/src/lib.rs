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
//! value   = {"v":2,"kind":"pat","secret":"…"}
//!         = {"v":2,"kind":"api_token","secret":"…",
//!            "account":{"username":"…","password":"…"}}
//! ```
//!
//! One item per source rather than per auth method, so changing PAT → password
//! rewrites in place instead of orphaning an item; the envelope's `v` and
//! `kind` leave room for OAuth (access + refresh + expiry) without a naming
//! change.
//!
//! # Version 2: a second credential in the same item (M4.1, issue #452)
//!
//! `account` is an **optional** username and password stored beside the
//! source's ordinary secret, for the one source that needs two credentials at
//! once: Uptime Kuma's API key opens `/metrics` and nothing else, and pausing
//! a monitor is a socket.io login an API key cannot make
//! ([`knobas_source::instance::Account`] argues the shape).
//!
//! **In the same item and not a second one.** The service+account pair above
//! is the keychain convention (interfaces §3), and a `source:<id>:account`
//! beside it would be a second item to keep in step with the first: a
//! `delete_source` that removed one and not the other leaves a credential
//! behind for a source that no longer exists, and a re-enter that rewrote one
//! and not the other leaves the pair disagreeing. One item has one lifetime,
//! and [`SecretStore`]'s three methods already give it exactly one.
//!
//! **A v1 envelope still reads**, as the source it describes: one credential
//! and no account. That is what makes this migration nothing -- every
//! credential already in a developer's keychain keeps working, and the item is
//! rewritten as v2 the next time it is stored. The refusal
//! [`decode`] makes is of a version this build has *not heard of*, which is
//! still the rule: a reader that guessed at a newer shape would hand an
//! adapter half a credential.
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
use knobas_source::instance::Account;

/// The keychain **account** one item is stored under -- the second half of the
/// service+account pair (interfaces §3), and the whole of what separates one
/// stored credential from another under one service.
///
/// # Two namespaces, and why this is a type
///
/// `source:<source_id>` is every configured [source](knobas_source)'s
/// credential. `importer:<producer_id>` is an **importer**'s (issue #509,
/// ADR-0015): an importer produces an estate file from a live system and is
/// *not* a source -- it mirrors nothing, syncs nothing, and appears nowhere
/// sources do -- so its token must not be reachable by a `delete_source`, a
/// credential-health sweep or a `list` that walks sources, and the way to make
/// that true of every reader at once is for the two to be different accounts
/// under the same service.
///
/// **A newtype rather than a `&str` and a naming function**, because the
/// namespace has to be applied exactly once. It used to be applied inside
/// [`KeyringStore`], which meant [`SecretStore`]'s argument was a *source id*
/// in one reading and an *account* in another; adding a second namespace to
/// that arrangement leaves every direct caller free to pass either, and the
/// failure is silent in the worst direction -- a test asserting a credential is
/// **absent** passes against a key nothing was ever written under. Here the
/// only two ways to make one are [`source`](Self::source) and
/// [`importer`](Self::importer), so the compiler asks every call site which it
/// meant.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeychainAccount(String);

impl KeychainAccount {
    /// The account one configured source's credential is stored under.
    #[must_use]
    pub fn source(source_id: &str) -> Self {
        Self(format!("source:{source_id}"))
    }

    /// The account one importer's credential is stored under (#509).
    ///
    /// `producer_id` is `knobas_app::assets::Producer::id` -- `hcloud` today --
    /// and not an adapter kind: importers and sources are different sets and
    /// nothing may be shared between the two but this crate.
    #[must_use]
    pub fn importer(producer_id: &str) -> Self {
        Self(format!("importer:{producer_id}"))
    }

    /// The account as the platform store spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for KeychainAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One credential. `value` is the PAT, password or API token itself.
#[derive(Clone)]
pub struct Secret {
    pub kind: AuthMethod,
    pub value: String,
    /// The optional second credential of envelope version 2 (issue #452):
    /// the account whose name and password open a channel [`value`](Self::value)
    /// cannot. `None` for every source that has one credential.
    ///
    /// The type is the SPI's, not a copy of it, because this is exactly what
    /// reaches an adapter as [`SourceInstance::account`]; a private twin here
    /// would be a shape to keep in step with that one.
    ///
    /// [`SourceInstance::account`]: knobas_source::instance::SourceInstance::account
    pub account: Option<Account>,
}

impl Secret {
    /// A credential with no account beside it -- the shape every source but
    /// Uptime Kuma has, and the one every caller wrote before issue #452.
    #[must_use]
    pub fn just(kind: AuthMethod, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
            account: None,
        }
    }
}

/// Hand-written, and the reason is the whole point of the type: a derived
/// `Debug` puts the credential into every `tracing` line and every panic
/// message that ever formats a struct containing one.
///
/// The account is printed through [`Account`]'s own `Debug`, which redacts the
/// password and keeps the username: whose login was refused is the question a
/// log line is being read for.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("kind", &self.kind)
            .field("value", &"<redacted>")
            .field("account", &self.account)
            .finish()
    }
}

/// Why a credential could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform store failed, or handed back something this version cannot
    /// read. Carries no secret material.
    ///
    /// **Also the bucket a locked macOS keychain lands in.** A caller deciding
    /// what to render must not read this as "an internal error, nothing the
    /// user can do": `errSecInteractionNotAllowed`, `errSecAuthFailed` and
    /// `errSecUserCanceled` all arrive here rather than in
    /// [`Unavailable`](SecretError::Unavailable) -- see that variant's docs for
    /// why, and for what an *Unlock your keychain* affordance has to key off
    /// instead. Pinned by `the_macos_codes_land_where_the_docs_say_they_do`.
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
    /// The credential under `account`, or `None` if there is none.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed -- absence is not a failure.
    fn get(&self, account: &KeychainAccount) -> Result<Option<Secret>, SecretError>;

    /// Store `secret` under `account`, replacing whatever was there.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed.
    fn put(&self, account: &KeychainAccount, secret: &Secret) -> Result<(), SecretError>;

    /// Remove the credential under `account`. **Absent is success**: deleting a
    /// source must not fail because its secret was already gone.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed for any other reason.
    fn delete(&self, account: &KeychainAccount) -> Result<(), SecretError>;
}

/// Current envelope version. Bumped only when the shape changes; a reader that
/// meets a higher one refuses rather than guessing.
///
/// **2 since issue #452**, when the envelope grew an optional `account`. Every
/// version this build knows is listed in [`READABLE_VERSIONS`], and a stored
/// item is upgraded by being written, never by a migration: there is nothing
/// to migrate *to* -- a v1 item decodes as the credential it always was.
const ENVELOPE_VERSION: u8 = 2;

/// The envelope versions this build can read.
///
/// A list rather than `<= ENVELOPE_VERSION`, so that dropping support for a
/// shape is a deletion here and reads as one, and so the refusal below can
/// name what it does understand.
const READABLE_VERSIONS: &[u8] = &[1, 2];

/// The stored account half. Its own type, `Deserialize`d by value, because
/// [`Envelope`] borrows from the raw string and a borrowed nested struct would
/// refuse any item whose JSON needed unescaping.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredAccount {
    username: String,
    password: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope<'a> {
    v: u8,
    kind: &'a str,
    secret: &'a str,
    /// Absent in v1, and absent in a v2 item for a source with one credential
    /// -- `skip_serializing_if` so the common envelope is byte-for-byte what
    /// it always was but for its `v`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<StoredAccount>,
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
///
/// Always at [`ENVELOPE_VERSION`], account or no account: the version names
/// the *shape a reader must understand*, and a v1 item that grew an `account`
/// would be one an older build silently read as a Kuma with no write half.
fn encode(secret: &Secret) -> Result<String, SecretError> {
    serde_json::to_string(&Envelope {
        v: ENVELOPE_VERSION,
        kind: envelope_kind(secret.kind),
        secret: &secret.value,
        account: secret.account.as_ref().map(|a| StoredAccount {
            username: a.username.clone(),
            password: a.password.clone(),
        }),
    })
    .map_err(|e| SecretError::Backend(e.to_string()))
}

/// Parse a stored envelope. The error text never quotes the payload.
///
/// Every version in [`READABLE_VERSIONS`] is read as the credential it
/// describes: a v1 item is a source with one credential, which is what it
/// always was. Anything else is refused rather than guessed at -- an envelope
/// from a newer knobas may carry a credential in a place this build would not
/// look, and handing an adapter the half it happened to recognise is worse
/// than saying so.
fn decode(raw: &str) -> Result<Secret, SecretError> {
    let envelope: Envelope<'_> = serde_json::from_str(raw)
        .map_err(|_| SecretError::Backend("unreadable envelope".into()))?;
    if !READABLE_VERSIONS.contains(&envelope.v) {
        return Err(SecretError::Backend(format!(
            "envelope version {} was written by a newer knobas; this build reads \
             {READABLE_VERSIONS:?}",
            envelope.v
        )));
    }
    let kind = kind_from_envelope(envelope.kind)
        .ok_or_else(|| SecretError::Backend(format!("unknown auth kind {:?}", envelope.kind)))?;
    Ok(Secret {
        kind,
        value: envelope.secret.to_owned(),
        account: envelope.account.map(|a| Account {
            username: a.username,
            password: a.password,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use knobas_source::instance::Account;

    /// `account` is half of the keychain convention (interfaces §3) and is
    /// load-bearing: changing it strands every stored credential.
    #[test]
    fn the_account_is_the_source_id_prefixed() {
        assert_eq!(KeychainAccount::source("jira-eu").as_str(), "source:jira-eu");
    }

    /// **The importer namespace** (#509, ADR-0015): an importer's token is not
    /// a source's credential and does not live where a source's does.
    ///
    /// Both halves, because the claim is that the two are *separated*: the
    /// prefix is what a stored item is found under, and no source id can be
    /// spelled in a way that reaches an importer's account -- a source called
    /// `importer:hcloud` still lands under `source:importer:hcloud`, which is
    /// what makes the namespaces disjoint rather than merely different.
    #[test]
    fn an_importers_token_lives_under_its_own_namespace() {
        assert_eq!(
            KeychainAccount::importer("hcloud").as_str(),
            "importer:hcloud"
        );
        assert_ne!(
            KeychainAccount::source("hcloud"),
            KeychainAccount::importer("hcloud")
        );
        assert_eq!(
            KeychainAccount::source("importer:hcloud").as_str(),
            "source:importer:hcloud"
        );
    }

    /// The envelope is versioned and carries the auth method, so PAT →
    /// password rewrites one item instead of orphaning another (§3), and OAuth
    /// can add fields later without a naming change.
    ///
    /// A source with one credential writes **no** `account` key at all -- the
    /// v2 envelope for the ordinary case is v1's with its version bumped, so
    /// the shape a reader has to understand is the shape they already knew.
    #[test]
    fn the_envelope_round_trips_and_names_its_version() {
        let secret = Secret::just(AuthMethod::Pat, "abc123");
        let json = encode(&secret).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({ "v": 2, "kind": "pat", "secret": "abc123" })
        );
        let back = decode(&json).unwrap();
        assert_eq!(back.kind, AuthMethod::Pat);
        assert_eq!(back.value, "abc123");
        assert!(back.account.is_none());
    }

    /// The v2 addition (issue #452): the API key and the account under **one**
    /// item, both surviving the hop.
    #[test]
    fn an_account_rides_beside_the_secret_in_the_same_item() {
        let secret = Secret {
            kind: AuthMethod::ApiToken,
            value: "uk1_metrics".into(),
            account: Some(Account {
                username: "knobas".into(),
                password: "knobas-dev".into(),
            }),
        };
        let json = encode(&secret).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({
                "v": 2,
                "kind": "api_token",
                "secret": "uk1_metrics",
                "account": { "username": "knobas", "password": "knobas-dev" },
            })
        );
        let back = decode(&json).unwrap();
        assert_eq!(back.value, "uk1_metrics");
        let account = back.account.expect("the account survives the envelope");
        assert_eq!(account.username, "knobas");
        assert_eq!(account.password, "knobas-dev");
    }

    /// **Nothing to migrate.** A v1 item -- every credential already in a
    /// developer's keychain -- decodes as the source it always described: one
    /// credential and no account. Asserted against the literal bytes v1 wrote,
    /// not against a re-encoding, because a re-encoding would be v2.
    #[test]
    fn a_version_one_item_still_reads_as_a_source_with_one_credential() {
        let back = decode(r#"{"v":1,"kind":"pat","secret":"abc123"}"#).unwrap();
        assert_eq!(back.kind, AuthMethod::Pat);
        assert_eq!(back.value, "abc123");
        assert!(
            back.account.is_none(),
            "a v1 item describes one credential and must not invent a second"
        );
    }

    #[test]
    fn every_auth_method_has_an_envelope_name_that_round_trips() {
        for kind in [
            AuthMethod::UserPassword,
            AuthMethod::Pat,
            AuthMethod::ApiToken,
            AuthMethod::OAuth,
        ] {
            let s = Secret::just(kind, "x");
            assert_eq!(decode(&encode(&s).unwrap()).unwrap().kind, kind);
        }
    }

    /// An envelope from a future knobas is not a secret we may guess at.
    ///
    /// The number moved with the version (issue #452): 2 is now a shape this
    /// build writes, so the probe is 3. What the test pins is the *rule* --
    /// a version outside [`READABLE_VERSIONS`] is refused -- and it is read
    /// off that list rather than written out, so bumping the version again
    /// cannot leave this asserting something the code no longer does.
    #[test]
    fn an_unknown_envelope_version_is_refused_rather_than_misread() {
        let unknown = READABLE_VERSIONS.iter().max().expect("a readable version") + 1;
        let err = decode(&format!(r#"{{"v":{unknown},"kind":"pat","secret":"x"}}"#)).unwrap_err();
        assert!(matches!(err, SecretError::Backend(_)), "got {err:?}");
        for readable in READABLE_VERSIONS {
            decode(&format!(r#"{{"v":{readable},"kind":"pat","secret":"x"}}"#))
                .unwrap_or_else(|e| panic!("version {readable} is declared readable: {e}"));
        }
    }

    /// A secret that prints itself is a secret in a log file.
    #[test]
    fn debug_never_prints_the_value() {
        let s = Secret {
            kind: AuthMethod::Pat,
            value: "hunter2".into(),
            account: Some(Account {
                username: "knobas".into(),
                password: "hunter3".into(),
            }),
        };
        let shown = format!("{s:?}");
        assert!(
            !shown.contains("hunter2"),
            "Debug leaked the secret: {shown}"
        );
        assert!(
            !shown.contains("hunter3"),
            "Debug leaked the account password: {shown}"
        );
        assert!(shown.contains("redacted"));
    }

    /// The store contract, exercised against the implementation every test and
    /// every CI run uses. [`KeyringStore`] runs the same assertions in the
    /// `#[ignore]`d macOS-local test.
    #[test]
    fn a_memory_store_honours_the_store_contract() {
        let store = MemoryStore::new();
        let jira = KeychainAccount::source("jira");
        let kuma = KeychainAccount::source("kuma");
        assert!(store.get(&jira).unwrap().is_none());
        // Deleting something absent is success, not an error: `delete_source`
        // must not fail because the secret was already gone.
        store.delete(&jira).unwrap();

        store
            .put(&jira, &Secret::just(AuthMethod::Pat, "one"))
            .unwrap();
        assert_eq!(store.get(&jira).unwrap().unwrap().value, "one");

        // Re-entering overwrites the one item rather than adding a second.
        store
            .put(&jira, &Secret::just(AuthMethod::UserPassword, "two"))
            .unwrap();
        let got = store.get(&jira).unwrap().unwrap();
        assert_eq!(
            (got.kind, got.value.as_str()),
            (AuthMethod::UserPassword, "two")
        );

        // The account is part of what a store keeps, not something `put`
        // takes and `get` forgets -- the whole of the write half turns on
        // this answer coming back.
        store
            .put(
                &kuma,
                &Secret {
                    kind: AuthMethod::ApiToken,
                    value: "uk1_metrics".into(),
                    account: Some(Account {
                        username: "knobas".into(),
                        password: "knobas-dev".into(),
                    }),
                },
            )
            .unwrap();
        assert_eq!(
            store
                .get(&kuma)
                .unwrap()
                .unwrap()
                .account
                .unwrap()
                .username,
            "knobas"
        );
        store.delete(&kuma).unwrap();

        store.delete(&jira).unwrap();
        assert!(store.get(&jira).unwrap().is_none());
    }

    /// Two sources are two items; deleting one leaves the other.
    #[test]
    fn sources_do_not_share_an_item() {
        let store = MemoryStore::new();
        let jira = KeychainAccount::source("jira");
        let eu = KeychainAccount::source("jira-eu");
        store
            .put(&jira, &Secret::just(AuthMethod::Pat, "a"))
            .unwrap();
        store
            .put(&eu, &Secret::just(AuthMethod::Pat, "b"))
            .unwrap();
        store.delete(&jira).unwrap();
        assert!(store.get(&jira).unwrap().is_none());
        assert_eq!(store.get(&eu).unwrap().unwrap().value, "b");
    }

    /// The async callers all go through `spawn_blocking`, because a locked
    /// keychain blocks on a user prompt for as long as the user takes.
    #[tokio::test]
    async fn the_spawn_helpers_reach_the_store() {
        let store: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
        let jira = KeychainAccount::source("jira");
        spawn::put(&store, &jira, Secret::just(AuthMethod::Pat, "z"))
            .await
            .unwrap();
        assert_eq!(
            spawn::get(&store, &jira).await.unwrap().unwrap().value,
            "z"
        );
        spawn::delete(&store, &jira).await.unwrap();
        assert!(spawn::get(&store, &jira).await.unwrap().is_none());
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
