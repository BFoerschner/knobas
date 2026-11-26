//! A command on a generated folder (ADR-0046; ADR-0060, amended 2026-11-25,
//! Fable's ruling): the folder knobas kept stands in the checkout's place, as
//! it does for *Open in* (`checkout::open::made_argv`).
//!
//! - **Paths.** `{file}`, `{dir}`, `{files}` and `{root}` are the kept folder's
//!   own paths on disk; `{relfile}` and `{reldir}` count from its top. Each is
//!   checked as a kept file is read (`folder_render::keep`): the manifest
//!   holds it, every folder on the way is plain, nothing is a link.
//! - **Where it runs.** The kept folder's top, or the folder it runs on
//!   (*Starts in*). Never the checkout, cloned or not.
//! - **No git names.** A kept folder is no repository: `{sha}`, `{base}`,
//!   `{head}` and `{branch}` are refused by name.
//! - **Places** are the manifest's files only.
//! - **Writes.** The kept folder is knobas's copy of what the renderer made.
//!   A run that changed one of its files has it dropped once no run holds it,
//!   so it is rendered again; a file a program added beside them is no change.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_core::checkout::oneoff::OneOff;

use super::run::{Placed, Target};
use crate::IpcError;
use crate::folder_render::keep::{self, Made, Stamp};
use crate::folder_render::store;

/// The most changed files a run names; the count past it is not kept.
pub const CHANGED_SHOWN: usize = 500;
/// How long a changed kept folder waits with no run in it before it goes: a
/// batch starts its next file as one ends, and is checked once, after its
/// last.
pub const DROP_AFTER: Duration = Duration::from_secs(2);

/// Whether `target` is a generated folder's.
#[must_use]
pub fn is_made(target: &Target) -> bool {
    matches!(target, Target::Made { .. } | Target::MadeFiles { .. })
}

/// The git placeholder a command on a generated folder may not hold.
#[must_use]
pub fn git_name(command: &OneOff) -> Option<&'static str> {
    ["{sha}", "{base}", "{head}", "{branch}"]
        .into_iter()
        .find(|name| command.command.contains(name))
}

/// The path a page named, from the generated folder's top, with `/`.
fn rel(path: &str) -> String {
    path.trim_start_matches("./").trim_matches('/').to_owned()
}

fn not_held(path: &str, folder: &str, renderer: &str) -> IpcError {
    IpcError::invalid(format!(
        "{path} is not in {} as {renderer} made it",
        if folder.is_empty() {
            "the repository"
        } else {
            folder
        }
    ))
}

/// The target found in the kept folder `made`.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a path the kept folder
/// does not hold as a plain file or folder.
pub fn placed(made: &Made, target: &Target) -> Result<Placed, IpcError> {
    let mut placed = Placed {
        root: made.dir.clone(),
        file: None,
        files: Vec::new(),
        dir: None,
        lines: None,
        sha: None,
        base: None,
        head: None,
        branch: None,
        commit: None,
    };
    match target {
        Target::Made {
            renderer,
            folder,
            path,
            dir,
            lines,
        } => {
            let path = rel(path);
            if *dir {
                keep::checked_folder(made, &path).map_err(|_| not_held(&path, folder, renderer))?;
                placed.dir = Some(path);
            } else {
                keep::checked_file(made, &path).map_err(|_| not_held(&path, folder, renderer))?;
                placed.dir = Some(path.rsplit_once('/').map_or("", |(d, _)| d).to_owned());
                placed.file = Some(path);
                placed.lines = lines.map(|(a, b)| (a.max(1), b.max(a).max(1)));
            }
        }
        Target::MadeFiles {
            renderer,
            folder,
            paths,
        } => {
            for path in paths {
                let path = rel(path);
                keep::checked_file(made, &path).map_err(|_| not_held(&path, folder, renderer))?;
                placed.files.push(path);
            }
        }
        _ => {
            return Err(IpcError::invalid(
                "that is not a generated folder's".to_owned(),
            ));
        }
    }
    Ok(placed)
}

/// What a run on a kept folder holds while it runs: the folder against
/// removal, and its files as they were when it started.
pub struct Watch {
    made: Arc<Made>,
    before: Stamp,
    held: Arc<HashSet<String>>,
    hold: Option<store::Hold>,
    /// Called once no run holds the folder, when this run changed it.
    on_change: Option<Box<dyn FnOnce() + Send>>,
    after: Duration,
}

impl Watch {
    /// Holds `made` and stamps its files; `on_change` drops it, after
    /// [`DROP_AFTER`] with no run in it, where the run changes it.
    #[must_use]
    pub fn new(made: Arc<Made>, on_change: Box<dyn FnOnce() + Send>) -> Self {
        Self::after(made, on_change, DROP_AFTER)
    }

    /// [`Watch::new`] with the wait given (tests).
    #[must_use]
    pub fn after(made: Arc<Made>, on_change: Box<dyn FnOnce() + Send>, after: Duration) -> Self {
        let hold = store::hold(&made.dir);
        let before = keep::stamp(&made);
        let held = Arc::new(made.manifest.files.iter().map(|f| f.path.clone()).collect());
        Self {
            made,
            before,
            held,
            hold: Some(hold),
            on_change: Some(on_change),
            after,
        }
    }

    /// The kept folder's files: the only places of the run.
    #[must_use]
    pub fn held(&self) -> Arc<HashSet<String>> {
        Arc::clone(&self.held)
    }

    /// The run ended: the files it changed, the first [`CHANGED_SHOWN`]. A
    /// change has the folder dropped once no run holds it.
    #[must_use]
    pub fn finish(mut self) -> Vec<String> {
        let mut changed = keep::changed_since(&self.made, &self.before);
        let dir = self.made.dir.clone();
        drop(self.hold.take());
        if !changed.is_empty()
            && let Some(on_change) = self.on_change.take()
        {
            drop_when_free(dir, on_change, self.after);
        }
        changed.truncate(CHANGED_SHOWN);
        changed
    }
}

/// Calls `drop` once the kept folder at `dir` has had no run in it for
/// `after`, on a thread of its own; gives up after an hour of runs.
fn drop_when_free(dir: PathBuf, drop: Box<dyn FnOnce() + Send>, after: Duration) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3600);
        let mut free_since: Option<Instant> = None;
        while Instant::now() < deadline {
            if store::is_held(&dir) {
                free_since = None;
            } else if free_since.get_or_insert_with(Instant::now).elapsed() >= after {
                drop();
                return;
            }
            std::thread::sleep(
                Duration::from_millis(100).min(after.max(Duration::from_millis(10))),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder_render::keep::{MadeFile, Manifest};

    fn kept(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<Made>) {
        let dir = tempfile::tempdir().expect("temp");
        let mut list = Vec::new();
        for (path, text) in files {
            let at = dir.path().join(path);
            std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
            std::fs::write(&at, text).expect("write");
            list.push(MadeFile {
                path: (*path).to_owned(),
                size: text.len() as u64,
            });
        }
        let manifest = Manifest {
            files: list,
            bytes: 0,
            left_out: Vec::new(),
            left_out_total: 0,
        };
        let made = Arc::new(Made::new(dir.path().to_path_buf(), manifest));
        (dir, made)
    }

    fn file(path: &str) -> Target {
        Target::Made {
            renderer: "site".to_owned(),
            folder: "docs".to_owned(),
            path: path.to_owned(),
            dir: false,
            lines: Some((3, 2)),
        }
    }

    #[test]
    fn a_generated_file_is_placed_in_the_kept_folder() {
        let (_dir, made) = kept(&[("out/a.xml", "<a/>"), ("index.html", "")]);
        let placed = placed(&made, &file("out/a.xml")).expect("placed");
        assert_eq!(placed.root, made.dir);
        assert_eq!(placed.file.as_deref(), Some("out/a.xml"));
        assert_eq!(placed.dir.as_deref(), Some("out"));
        assert_eq!(placed.lines, Some((3, 3)));
        assert_eq!(placed.branch, None);
    }

    #[test]
    fn a_generated_folder_is_one_the_manifest_holds_a_file_in() {
        let (_dir, made) = kept(&[("out/a.xml", "<a/>")]);
        let folder = |path: &str| Target::Made {
            renderer: "site".to_owned(),
            folder: "docs".to_owned(),
            path: path.to_owned(),
            dir: true,
            lines: None,
        };
        assert_eq!(
            placed(&made, &folder("out")).expect("out").dir.as_deref(),
            Some("out")
        );
        assert_eq!(
            placed(&made, &folder("")).expect("top").dir.as_deref(),
            Some("")
        );
        std::fs::create_dir(made.dir.join("empty")).expect("dir");
        assert!(
            placed(&made, &folder("empty")).is_err(),
            "a folder it holds no file in"
        );
    }

    #[test]
    fn a_path_the_manifest_does_not_hold_is_refused() {
        let (_dir, made) = kept(&[("a.xml", "<a/>")]);
        std::fs::write(made.dir.join("added.txt"), "x").expect("write");
        let Err(error) = placed(&made, &file("added.txt")) else {
            panic!("a file the manifest does not hold was placed");
        };
        assert!(
            error.message.contains("not in docs as site made it"),
            "{}",
            error.message
        );
        assert!(placed(&made, &file("../a.xml")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_swapped_for_a_link_is_refused() {
        let (_dir, made) = kept(&[("out/a.xml", "<a/>")]);
        let elsewhere = tempfile::tempdir().expect("temp");
        std::fs::write(elsewhere.path().join("a.xml"), "<b/>").expect("write");
        std::fs::remove_dir_all(made.dir.join("out")).expect("rm");
        std::os::unix::fs::symlink(elsewhere.path(), made.dir.join("out")).expect("link");
        assert!(placed(&made, &file("out/a.xml")).is_err());
    }

    #[test]
    fn a_changed_file_is_named_and_the_folder_dropped_once_free() {
        let (_dir, made) = kept(&[("a.xml", "<a/>"), ("b.xml", "<b/>")]);
        let (tx, rx) = std::sync::mpsc::channel();
        let watch = Watch::after(
            Arc::clone(&made),
            Box::new(move || tx.send(()).expect("send")),
            Duration::from_millis(20),
        );
        assert!(store::is_held(&made.dir));
        std::fs::write(made.dir.join("b.xml"), "<b>changed</b>").expect("write");
        assert_eq!(watch.finish(), vec!["b.xml".to_owned()]);
        assert!(!store::is_held(&made.dir));
        rx.recv_timeout(Duration::from_secs(5)).expect("dropped");
    }

    #[test]
    fn a_file_added_beside_them_drops_nothing() {
        let (_dir, made) = kept(&[("a.xml", "<a/>")]);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let watch = Watch::after(
            Arc::clone(&made),
            Box::new(move || tx.send(()).expect("send")),
            Duration::from_millis(10),
        );
        std::fs::write(made.dir.join(".eslintcache"), "{}").expect("write");
        assert!(watch.finish().is_empty());
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn a_folder_with_a_run_in_it_waits_for_the_last() {
        let (_dir, made) = kept(&[("a.xml", "<a/>")]);
        let (tx, rx) = std::sync::mpsc::channel();
        let other = store::hold(&made.dir);
        let watch = Watch::after(
            Arc::clone(&made),
            Box::new(move || tx.send(()).expect("send")),
            Duration::from_millis(20),
        );
        std::fs::write(made.dir.join("a.xml"), "<a>changed</a>").expect("write");
        assert_eq!(watch.finish(), vec!["a.xml".to_owned()]);
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "another run holds it"
        );
        drop(other);
        rx.recv_timeout(Duration::from_secs(5))
            .expect("dropped after the last");
    }

    #[test]
    fn git_names_are_named() {
        let mut command: OneOff = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "x", "command": "lint {file} --rev {branch}"
        }))
        .expect("command");
        assert_eq!(git_name(&command), Some("{branch}"));
        command.command = "lint {file}".to_owned();
        assert_eq!(git_name(&command), None);
    }
}
