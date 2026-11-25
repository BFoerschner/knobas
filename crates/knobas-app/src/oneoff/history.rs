//! The list of runs (section 20): the last 200, without what they printed,
//! in `<profile>/command-runs/runs.json`, written when a run ends. A file of
//! knobas's own, never a table, so no migration; a backup dumps the database
//! and never carries it.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::run::{Ended, Target};

/// The most runs the list keeps.
pub const KEPT: usize = 200;

/// One run, as the list keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffRunRow")]
pub struct RunRow {
    pub id: String,
    pub command: String,
    pub name: String,
    pub entity_id: String,
    pub target: Target,
    /// The words it was run with.
    pub argv: Vec<String>,
    /// The folder it started in.
    pub cwd: String,
    /// The checkout's branch and commit when it started.
    #[ts(optional = nullable)]
    pub branch: Option<String>,
    #[ts(optional = nullable)]
    pub head: Option<String>,
    /// Unix milliseconds.
    pub started_at: i64,
    /// `None` while it runs.
    #[ts(optional = nullable)]
    pub duration_ms: Option<u64>,
    #[ts(optional = nullable)]
    pub ended: Option<Ended>,
    pub lines: u32,
    pub findings: u32,
    /// Whether what it printed is still in memory.
    #[serde(default)]
    pub kept: bool,
}

/// The list as it stands in memory, read from its file once.
pub struct History {
    file: PathBuf,
    rows: Mutex<Option<Vec<RunRow>>>,
}

impl History {
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            file: dir.join("runs.json"),
            rows: Mutex::new(None),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Vec<RunRow>) -> T) -> T {
        let mut rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
        let rows = rows.get_or_insert_with(|| read(&self.file));
        f(rows)
    }

    /// Every run, newest first.
    #[must_use]
    pub fn rows(&self) -> Vec<RunRow> {
        self.with(|rows| rows.iter().rev().cloned().collect())
    }

    /// A run ended: kept at the end, the oldest past [`KEPT`] dropped, the
    /// file written.
    pub fn ended(&self, row: RunRow) {
        let all = self.with(|rows| {
            rows.retain(|r| r.id != row.id);
            rows.push(RunRow { kept: false, ..row });
            let over = rows.len().saturating_sub(KEPT);
            rows.drain(..over);
            rows.clone()
        });
        write(&self.file, &all);
    }

    /// Forgets one run, or every run.
    pub fn forget(&self, id: Option<&str>) {
        let all = self.with(|rows| {
            match id {
                Some(id) => rows.retain(|r| r.id != id),
                None => rows.clear(),
            }
            rows.clone()
        });
        write(&self.file, &all);
    }
}

/// The file's rows; a file that is missing or does not parse is no runs.
fn read(file: &Path) -> Vec<RunRow> {
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the rows beside the file and moves them over it.
fn write(file: &Path, rows: &[RunRow]) {
    let Some(dir) = file.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(rows) else {
        return;
    };
    let staged = file.with_extension("json.new");
    if std::fs::write(&staged, bytes).is_ok() {
        let _ = std::fs::rename(&staged, file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> RunRow {
        RunRow {
            id: id.to_owned(),
            command: "eslint".to_owned(),
            name: "ESLint".to_owned(),
            entity_id: "gitea:a/b".to_owned(),
            target: Target::Repository,
            argv: vec!["eslint".to_owned()],
            cwd: "/x".to_owned(),
            branch: None,
            head: None,
            started_at: 1,
            duration_ms: Some(2),
            ended: Some(Ended::Exit { code: 1 }),
            lines: 3,
            findings: 1,
            kept: true,
        }
    }

    #[test]
    fn two_hundred_and_one_runs_keep_two_hundred_written_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::new(dir.path());
        for i in 0..201 {
            history.ended(row(&format!("r{i}")));
        }
        let rows = History::new(dir.path()).rows();
        assert_eq!(rows.len(), KEPT);
        assert_eq!(rows[0].id, "r200");
        assert_eq!(rows[KEPT - 1].id, "r1");
        assert!(rows.iter().all(|r| !r.kept));
    }

    #[test]
    fn a_file_that_does_not_parse_is_no_runs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("runs.json"), "{not json").unwrap();
        assert!(History::new(dir.path()).rows().is_empty());
    }

    #[test]
    fn forgetting_one_or_every_run() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::new(dir.path());
        history.ended(row("a"));
        history.ended(row("b"));
        history.forget(Some("a"));
        assert_eq!(History::new(dir.path()).rows().len(), 1);
        history.forget(None);
        assert!(History::new(dir.path()).rows().is_empty());
    }
}
