//! The responsiveness probe's saved databases (#781).
//!
//! Building the 22,000-item mirror is minutes; reading it is seconds. The
//! probe therefore keeps the built database -- the whole server root, `data/`
//! and `.pgpass` -- as a snapshot under `$TMPDIR/knobas-probe-snapshots/`, two
//! copies per key (never analyzed, analyzed), and starts a run from a clone
//! of one. The clone is `cp -c` (an APFS copy-on-write clone), so it costs
//! seconds whatever the size.
//!
//! A snapshot's name is `m<migrations>-p<seed>-<state>`: `<migrations>` hashes
//! every file in the migrations directory, `<seed>` the probe's seed
//! parameters (sizes, `RP_SYNTHETIC`, whether Gitea was synced). A changed key
//! names a snapshot that does not exist, so it is rebuilt; the old one is
//! stale the moment its `<migrations>` part differs from the current one, or
//! it is older than [`MAX_AGE`], and the next run deletes it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// A snapshot older than this is deleted by the next run.
pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Written last into a finished snapshot; its mtime is the snapshot's age.
const DONE: &str = ".snapshot-done";

/// The two statistics states a snapshot is kept in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NeverAnalyzed,
    Analyzed,
}

impl State {
    fn suffix(self) -> &'static str {
        match self {
            State::NeverAnalyzed => "never",
            State::Analyzed => "analyzed",
        }
    }
}

fn hex(hasher: &impl Hasher) -> String {
    format!("{:016x}", hasher.finish())
}

/// A hash of every file in `dir` -- names and bytes, in name order -- so a
/// changed, added or removed migration changes it.
pub fn migrations_hash(dir: &Path) -> String {
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    names.sort();
    let mut hasher = DefaultHasher::new();
    for path in names {
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .hash(&mut hasher);
        std::fs::read(&path).unwrap().hash(&mut hasher);
    }
    hex(&hasher)
}

/// A hash of the suggestion pass's code: the file `suggest` and every
/// migration (`migrations`, as [`migrations_hash`] gives it), so a change to
/// either names an after-pass snapshot that does not exist.
pub fn pass_code_hash(suggest: &Path, migrations: &str) -> String {
    let mut hasher = DefaultHasher::new();
    std::fs::read(suggest)
        .unwrap_or_else(|error| panic!("read {}: {error}", suggest.display()))
        .hash(&mut hasher);
    migrations.hash(&mut hasher);
    hex(&hasher)
}

/// A hash of the probe's seed parameters, as `name=value` text.
pub fn seed_hash(params: &[(&str, String)]) -> String {
    let mut hasher = DefaultHasher::new();
    for (name, value) in params {
        name.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    hex(&hasher)
}

/// The snapshot's directory name for `state`.
pub fn name(migrations: &str, seed: &str, state: State) -> String {
    format!("m{migrations}-p{seed}-{}", state.suffix())
}

/// The after-pass snapshot's directory name: the seed key plus the pass code
/// `pass` (see [`pass_code_hash`]), kept per statistics state.
pub fn passed_name(migrations: &str, seed: &str, pass: &str, state: State) -> String {
    format!("m{migrations}-p{seed}-c{pass}-passed-{}", state.suffix())
}

/// Whether the snapshot directory `name` belongs to the migrations `current`.
fn of_migrations(name: &str, current: &str) -> bool {
    name.strip_prefix('m')
        .and_then(|rest| rest.split_once("-p"))
        .is_some_and(|(migrations, _)| migrations == current)
}

/// The directory snapshots live in: `$TMPDIR/knobas-probe-snapshots`.
pub fn root() -> PathBuf {
    std::env::temp_dir().join("knobas-probe-snapshots")
}

/// How old `path` is by its finished-marker (or itself, when it has none).
pub fn age(path: &Path, now: SystemTime) -> Duration {
    let marker = path.join(DONE);
    let stamped = std::fs::metadata(&marker).or_else(|_| std::fs::metadata(path));
    stamped
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| now.duration_since(t).ok())
        .unwrap_or_default()
}

/// Whether `path` is a finished snapshot.
pub fn exists(path: &Path) -> bool {
    path.join(DONE).is_file()
}

/// Delete every entry of `root` that is stale: another migrations hash, older
/// than `max_age`, or an unfinished copy older than an hour. Returns what it
/// deleted.
pub fn sweep(root: &Path, current: &str, now: SystemTime, max_age: Duration) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let file = entry.file_name().to_string_lossy().into_owned();
        let unfinished = !exists(&path);
        let stale = if unfinished {
            age(&path, now) > Duration::from_secs(3600)
        } else {
            !of_migrations(&file, current) || age(&path, now) > max_age
        };
        if stale && std::fs::remove_dir_all(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// Copy the directory `from` to the new path `to`, as an APFS clone where the
/// filesystem has them and as a plain copy where it does not.
pub fn clone_dir(from: &Path, to: &Path) {
    let _ = std::fs::remove_dir_all(to);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let cloned = Command::new("cp")
        .arg("-cR")
        .arg(from)
        .arg(to)
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !cloned {
        let _ = std::fs::remove_dir_all(to);
        let status = Command::new("cp")
            .arg("-R")
            .arg(from)
            .arg(to)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "copy {} to {}",
            from.display(),
            to.display()
        );
    }
}

/// Save the stopped server root `from` as the snapshot `to`: copied beside it,
/// marked finished, then renamed, so a killed run leaves nothing a later run
/// would take for a snapshot.
pub fn save(from: &Path, to: &Path) {
    let partial = to.with_extension("partial");
    clone_dir(from, &partial);
    let _ = std::fs::remove_file(partial.join(".owner.lock"));
    let _ = std::fs::remove_file(partial.join("data").join("postmaster.pid"));
    std::fs::write(partial.join(DONE), b"").unwrap();
    let _ = std::fs::remove_dir_all(to);
    std::fs::rename(&partial, to).unwrap();
}

/// Clone the snapshot `from` to the run's own root `to`, taking the
/// finished-marker off the clone so the run's directory is never mistaken for
/// a snapshot.
pub fn restore(from: &Path, to: &Path) {
    clone_dir(from, to);
    let _ = std::fs::remove_file(to.join(DONE));
}

/// Total bytes of the files under `path`.
pub fn size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(m) if m.is_dir() => size(&entry.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (file, text) in files {
            std::fs::write(dir.path().join(file), text).unwrap();
        }
        dir
    }

    #[test]
    fn a_changed_added_or_removed_migration_changes_the_hash() {
        let base = migrations_hash(dir_with(&[("0001.sql", "a"), ("0002.sql", "b")]).path());
        assert_eq!(
            base,
            migrations_hash(dir_with(&[("0002.sql", "b"), ("0001.sql", "a")]).path())
        );
        assert_ne!(
            base,
            migrations_hash(dir_with(&[("0001.sql", "a"), ("0002.sql", "c")]).path())
        );
        assert_ne!(base, migrations_hash(dir_with(&[("0001.sql", "a")]).path()));
        assert_ne!(
            base,
            migrations_hash(
                dir_with(&[("0001.sql", "a"), ("0002.sql", "b"), ("0003.sql", "")]).path()
            )
        );
    }

    #[test]
    fn every_seed_parameter_is_in_the_key() {
        let p = |tickets: &str, synthetic: &str| {
            seed_hash(&[("tickets", tickets.into()), ("synthetic", synthetic.into())])
        };
        assert_eq!(p("5000", "4000"), p("5000", "4000"));
        assert_ne!(p("5000", "4000"), p("500", "4000"));
        assert_ne!(p("5000", "4000"), p("5000", "0"));
    }

    #[test]
    fn a_change_to_the_pass_code_or_the_migrations_names_another_after_pass_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("suggest.rs");
        std::fs::write(&file, "fn pass() {}\n").unwrap();
        let before = pass_code_hash(&file, "m1");
        assert_eq!(before, pass_code_hash(&file, "m1"));
        assert_ne!(before, pass_code_hash(&file, "m2"));
        std::fs::write(&file, "fn pass() {} \n").unwrap(); // whitespace only
        let after = pass_code_hash(&file, "m1");
        assert_ne!(before, after);
        std::fs::write(&file, "fn mass() {}\n").unwrap(); // same length
        assert_ne!(before, pass_code_hash(&file, "m1"));
        let name = |pass: &str| passed_name("m", "s", pass, State::Analyzed);
        assert_ne!(name(&before), name(&after));
        assert_ne!(
            passed_name("m", "s", &before, State::Analyzed),
            passed_name("m", "s", &before, State::NeverAnalyzed)
        );
        assert_ne!(name(&before), name_of_base());
        assert!(of_migrations(&name(&before), "m"));
    }

    fn name_of_base() -> String {
        name("m", "s", State::Analyzed)
    }

    #[test]
    fn the_two_states_are_two_snapshots() {
        assert_ne!(
            name("m", "s", State::NeverAnalyzed),
            name("m", "s", State::Analyzed)
        );
    }

    fn snapshot(root: &Path, name: &str, done: bool) {
        let path = root.join(name);
        std::fs::create_dir_all(path.join("data")).unwrap();
        if done {
            std::fs::write(path.join(DONE), b"").unwrap();
        }
    }

    #[test]
    fn the_sweep_removes_a_stale_key_and_an_old_age_and_spares_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let current = "aaaa";
        let keep = name(current, "s1", State::Analyzed);
        let other_key = name("bbbb", "s1", State::Analyzed);
        let old = name(current, "s2", State::NeverAnalyzed);
        snapshot(root.path(), &keep, true);
        snapshot(root.path(), &other_key, true);
        snapshot(root.path(), &old, true);

        // Eight days on: every finished snapshot is older than a week...
        let later = SystemTime::now() + Duration::from_secs(8 * 24 * 3600);
        assert_eq!(sweep(root.path(), current, later, MAX_AGE).len(), 3);

        // ...and now: only the other migrations' snapshot goes.
        snapshot(root.path(), &keep, true);
        snapshot(root.path(), &other_key, true);
        let removed = sweep(root.path(), current, SystemTime::now(), MAX_AGE);
        assert_eq!(removed, vec![root.path().join(&other_key)]);
        assert!(exists(&root.path().join(&keep)));
    }

    #[test]
    fn an_unfinished_copy_is_not_a_snapshot_and_goes_once_it_is_old() {
        let root = tempfile::tempdir().unwrap();
        snapshot(root.path(), "m1-p2-never.partial", false);
        assert!(!exists(&root.path().join("m1-p2-never.partial")));
        assert!(sweep(root.path(), "1", SystemTime::now(), MAX_AGE).is_empty());
        let later = SystemTime::now() + Duration::from_secs(2 * 3600);
        assert_eq!(sweep(root.path(), "1", later, MAX_AGE).len(), 1);
    }

    #[test]
    fn a_saved_snapshot_is_a_finished_copy_without_the_servers_locks() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("run");
        std::fs::create_dir_all(from.join("data")).unwrap();
        std::fs::write(from.join("data").join("f"), b"12345").unwrap();
        std::fs::write(from.join("data").join("postmaster.pid"), b"1").unwrap();
        std::fs::write(from.join(".owner.lock"), b"").unwrap();
        let to = root.path().join("snap");
        save(&from, &to);
        assert!(exists(&to));
        assert!(to.join("data").join("f").is_file());
        assert!(!to.join("data").join("postmaster.pid").exists());
        assert!(!to.join(".owner.lock").exists());
        assert_eq!(size(&to), 5);
    }
}
