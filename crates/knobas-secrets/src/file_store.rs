//! A file of credentials, for the demo profile of a debug build only (#2384).
//!
//! **Why it exists.** The knobas-dev signing certificate has no Apple team ID,
//! so macOS partitions every login-keychain item a dev build writes by the
//! writer's cdhash (`partition_id: cdhash:…` in `security dump-keychain -a`).
//! Every rebuild is a new cdhash, so every rebuilt debug binary or bundle asked
//! for the login password once per item, and "Always Allow" only added that one
//! build. The demo profile holds testenv credentials alone, so a debug build
//! keeps them here instead and the keychain is never asked.
//!
//! **Compiled out of release builds**: this module exists only under
//! `debug_assertions`, so a release binary cannot name the type, let alone
//! write a credential to disk. `knobas_app::sources` decides when a debug build
//! uses it (demo profile, keychain requested) and pins that decision by test.
//!
//! The file is one JSON object, account → the same envelope a keychain item
//! holds, mode 0600. Every call re-reads it, and a write replaces it through a
//! temporary file renamed into place under an exclusive lock, because a demo
//! app and `knobas-app live-sources` (or two agents' bundles) may share it.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::{KeychainAccount, Secret, SecretError, SecretStore, decode, encode};

/// Credentials in one 0600 file. See the module docs for when it is used.
pub struct FileStore {
    path: PathBuf,
}

type Items = BTreeMap<String, String>;

impl FileStore {
    /// A store over the file at `path`, created on the first `put`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file this store reads and writes. For diagnostics.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    /// Hold the sidecar lock for one read-modify-write.
    fn lock(&self) -> Result<File, SecretError> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(io_error)?;
        }
        let file = private(OpenOptions::new().create(true).truncate(false).write(true))
            .open(self.lock_path())
            .map_err(io_error)?;
        file.lock().map_err(io_error)?;
        Ok(file)
    }

    fn read(&self) -> Result<Items, SecretError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|_| SecretError::Backend("unreadable secret file".into())),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Items::new()),
            Err(e) => Err(io_error(e)),
        }
    }

    fn write(&self, items: &Items) -> Result<(), SecretError> {
        let text =
            serde_json::to_string_pretty(items).map_err(|e| SecretError::Backend(e.to_string()))?;
        let tmp = self.path.with_extension("tmp");
        let mut file = private(OpenOptions::new().create(true).truncate(true).write(true))
            .open(&tmp)
            .map_err(io_error)?;
        file.write_all(text.as_bytes()).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(&tmp, &self.path).map_err(io_error)
    }

    fn update(
        &self,
        change: impl FnOnce(&mut Items) -> Result<(), SecretError>,
    ) -> Result<(), SecretError> {
        let _lock = self.lock()?;
        let mut items = self.read()?;
        change(&mut items)?;
        self.write(&items)
    }
}

/// Owner read/write only, set at creation so the file is never wider.
fn private(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
}

/// An I/O failure, by kind and message: neither carries file contents.
fn io_error(error: std::io::Error) -> SecretError {
    SecretError::Backend(format!("secret file: {error}"))
}

impl SecretStore for FileStore {
    fn get(&self, account: &KeychainAccount) -> Result<Option<Secret>, SecretError> {
        self.read()?
            .get(account.as_str())
            .map(|raw| decode(raw))
            .transpose()
    }

    fn put(&self, account: &KeychainAccount, secret: &Secret) -> Result<(), SecretError> {
        let raw = encode(secret)?;
        self.update(|items| {
            items.insert(account.as_str().to_owned(), raw);
            Ok(())
        })
    }

    fn delete(&self, account: &KeychainAccount) -> Result<(), SecretError> {
        // Absent is success, as for every store; no file is not created for it.
        if !self.path.exists() {
            return Ok(());
        }
        self.update(|items| {
            items.remove(account.as_str());
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use knobas_source::instance::Account;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("knobas-file-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("secrets.json")
    }

    #[test]
    fn a_put_secret_is_read_back_by_a_second_store_over_the_same_file() {
        let path = scratch("reopen");
        let id = KeychainAccount::source("live-kuma");
        FileStore::new(&path)
            .put(
                &id,
                &Secret {
                    kind: AuthMethod::ApiToken,
                    value: "tok\"en".into(),
                    account: Some(Account {
                        username: "admin".into(),
                        password: "pw".into(),
                    }),
                },
            )
            .unwrap();
        FileStore::new(&path)
            .put(
                &KeychainAccount::source("live-jira"),
                &Secret::just(AuthMethod::Pat, "j"),
            )
            .unwrap();

        let got = FileStore::new(&path)
            .get(&id)
            .unwrap()
            .expect("kept on disk");
        assert_eq!(
            (got.kind, got.value.as_str()),
            (AuthMethod::ApiToken, "tok\"en")
        );
        assert_eq!(got.account.expect("account kept").username, "admin");
    }

    #[test]
    fn an_absent_account_is_none_and_deleting_it_succeeds() {
        let path = scratch("absent");
        let store = FileStore::new(&path);
        let id = KeychainAccount::source("nope");
        assert!(store.get(&id).unwrap().is_none());
        store.delete(&id).unwrap();
        assert!(!path.exists(), "a delete of nothing writes no file");
    }

    #[test]
    fn a_deleted_secret_is_gone_and_its_neighbour_stays() {
        let path = scratch("delete");
        let store = FileStore::new(&path);
        let (a, b) = (KeychainAccount::source("a"), KeychainAccount::source("b"));
        store.put(&a, &Secret::just(AuthMethod::Pat, "1")).unwrap();
        store.put(&b, &Secret::just(AuthMethod::Pat, "2")).unwrap();
        store.delete(&a).unwrap();
        assert!(store.get(&a).unwrap().is_none());
        assert_eq!(store.get(&b).unwrap().unwrap().value, "2");
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let path = scratch("mode");
        FileStore::new(&path)
            .put(
                &KeychainAccount::source("a"),
                &Secret::just(AuthMethod::Pat, "1"),
            )
            .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "mode {mode:o}");
    }
}
