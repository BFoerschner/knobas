//! The store every test and every CI run gets.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use knobas_source::AuthMethod;

use crate::{Secret, SecretError, SecretStore};

/// An in-process credential store.
///
/// Not a stub: it is the store `just check` runs against, so it honours the
/// same contract as [`KeyringStore`](crate::KeyringStore) -- including "delete
/// what is not there is success".
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<String, (AuthMethod, String)>>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn items(&self) -> MutexGuard<'_, HashMap<String, (AuthMethod, String)>> {
        // A panic while holding this lock leaves a plain map with no invariant
        // to violate, so poisoning would only turn one test failure into a
        // second, less informative one.
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self.items().get(source_id).map(|(kind, value)| Secret {
            kind: *kind,
            value: value.clone(),
        }))
    }

    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError> {
        self.items()
            .insert(source_id.to_owned(), (secret.kind, secret.value.clone()));
        Ok(())
    }

    fn delete(&self, source_id: &str) -> Result<(), SecretError> {
        self.items().remove(source_id);
        Ok(())
    }
}
