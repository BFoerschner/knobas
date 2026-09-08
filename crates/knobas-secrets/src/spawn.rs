//! `spawn_blocking` wrappers -- the only way async knobas code touches a store.
//!
//! [`SecretStore`] is synchronous because the platform APIs are, and because a
//! locked keychain waits on a human clicking *Allow*. Calling it straight from
//! a tokio worker parks that worker for the length of a dialog, which on a
//! multi-thread runtime with a busy sync wave is how the UI stops answering.

use std::sync::Arc;

use crate::{KeychainAccount, Secret, SecretError, SecretStore};

fn joined<T>(
    result: Result<Result<T, SecretError>, tokio::task::JoinError>,
) -> Result<T, SecretError> {
    match result {
        Ok(inner) => inner,
        Err(join) => Err(SecretError::Backend(format!(
            "keychain task failed: {join}"
        ))),
    }
}

/// # Errors
/// Whatever the store reported, or [`SecretError::Backend`] if the blocking
/// task itself failed.
pub async fn get(
    store: &Arc<dyn SecretStore>,
    account: &KeychainAccount,
) -> Result<Option<Secret>, SecretError> {
    let store = Arc::clone(store);
    let account = account.clone();
    joined(tokio::task::spawn_blocking(move || store.get(&account)).await)
}

/// # Errors
/// As [`get`].
pub async fn put(
    store: &Arc<dyn SecretStore>,
    account: &KeychainAccount,
    secret: Secret,
) -> Result<(), SecretError> {
    let store = Arc::clone(store);
    let account = account.clone();
    joined(tokio::task::spawn_blocking(move || store.put(&account, &secret)).await)
}

/// # Errors
/// As [`get`].
pub async fn delete(
    store: &Arc<dyn SecretStore>,
    account: &KeychainAccount,
) -> Result<(), SecretError> {
    let store = Arc::clone(store);
    let account = account.clone();
    joined(tokio::task::spawn_blocking(move || store.delete(&account)).await)
}
