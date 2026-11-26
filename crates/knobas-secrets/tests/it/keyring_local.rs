//! The one test that touches a real keychain. **Never run by `just check`.**
//!
//! Run it by hand on macOS with:
//!     cargo test -p knobas-secrets --test it keyring_local -- --ignored
//! It writes under a per-run service name and deletes what it wrote, so it
//! cannot collide with the credentials a real knobas stored.

use knobas_secrets::{KeychainAccount, KeyringStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_source::instance::Account;

#[test]
#[ignore = "touches the real OS keychain; macOS-local, never in CI"]
fn the_keyring_store_honours_the_store_contract() {
    // A service nothing else uses, so a failed run leaves no litter behind a
    // name a real profile would read. This is interfaces §3's
    // `dev.knobas.desktop.test.<run>`, and it is spelled out here rather than
    // taken from a helper for the same reason the store never builds one: the
    // only producer of a *real* service name is
    // `knobas_app::Profile::keychain_service()`.
    let service = format!("dev.knobas.desktop.test.{}", std::process::id());
    let store = KeyringStore::new(&service);
    assert_eq!(store.service(), service);
    let id = &KeychainAccount::source("keyring-local");

    store
        .delete(id)
        .expect("deleting an absent item is success");
    assert!(store.get(id).unwrap().is_none());

    store
        .put(id, &Secret::just(AuthMethod::Pat, "one"))
        .unwrap();
    let got = store.get(id).unwrap().unwrap();
    assert_eq!((got.kind, got.value.as_str()), (AuthMethod::Pat, "one"));

    // Re-entering rewrites the one item rather than adding a second.
    store
        .put(id, &Secret::just(AuthMethod::UserPassword, "two"))
        .unwrap();
    let got = store.get(id).unwrap().unwrap();
    assert_eq!(
        (got.kind, got.value.as_str()),
        (AuthMethod::UserPassword, "two")
    );
    assert!(
        got.account.is_none(),
        "a credential stored without an account must not grow one"
    );

    // The v2 envelope through the **real** platform store (issue #452): the
    // API key and the account come back out of one keychain item. The
    // in-memory store honours the same contract, but only this one witnesses
    // that the account survives a round trip through the OS.
    store
        .put(
            id,
            &Secret {
                kind: AuthMethod::ApiToken,
                value: "uk1_metrics".into(),
                account: Some(Account {
                    username: "knobas".into(),
                    password: "knobas-dev".into(),
                }),
                web: None,
            },
        )
        .unwrap();
    let got = store.get(id).unwrap().unwrap();
    assert_eq!(got.value, "uk1_metrics");
    let account = got
        .account
        .expect("the account came back out of the keychain");
    assert_eq!(account.username, "knobas");
    assert_eq!(account.password, "knobas-dev");

    store.delete(id).unwrap();
    assert!(store.get(id).unwrap().is_none());
}

/// The Passwords role's code keys (ADR-0056) through the real keychain: a
/// whole set written in chunks under one write id, read back whole, and
/// deleted, on whatever store this OS gives (the Mac's keychain, Windows'
/// credential manager, Linux's kernel keyutils).
#[test]
#[ignore = "touches the real OS keychain; run by hand"]
fn the_code_keys_round_trip_through_the_real_keychain() {
    use knobas_source::passwords::{OtpAlgorithm, OtpKey, SecretText};
    let service = format!("dev.knobas.desktop.test.otp.{}", std::process::id());
    let store = KeyringStore::new(&service);
    let key = |s: &str| OtpKey {
        secret: SecretText::new(s),
        digits: 6,
        period: 30,
        algorithm: OtpAlgorithm::Sha1,
    };
    let keys: knobas_secrets::otp::OtpKeys = (0..40)
        .map(|i| {
            (
                format!("pw:{i:036}"),
                key("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
            )
        })
        .collect();
    knobas_secrets::otp::replace(&store, "pw", &keys).unwrap();
    let back = knobas_secrets::otp::read(&store, "pw").unwrap();
    assert_eq!(
        back.len(),
        keys.len(),
        "the keys read back are not the keys written"
    );
    knobas_secrets::otp::delete(&store, "pw").unwrap();
    assert!(knobas_secrets::otp::read(&store, "pw").unwrap().is_empty());
}
