//! The Builds frame's live reads of a run (ADR-0022, issues #854, #855, #856).
//!
//! ADR-0022 names four reads a CI adapter answers for a run: a range of log
//! lines, its tests, the artifact tree with a download stream, and its
//! changes. Changes are the run's source view (#853); this module is the
//! other three, each answered in a shape the shell draws without knowing the
//! source, and none stored:
//!
//! * **The log** ([`RunLog`]): a range of the whole log's lines, numbered from
//!   1, each with its time and tone, and the run's steps as line ranges over
//!   the whole log. The adapter streams the log and keeps only the range, so a
//!   log of any length costs at most [`LOG_LINES_MAX`] lines.
//! * **The tests** ([`RunTests`]) and one test's outcome in the runs before
//!   ([`TestOutcome`]).
//! * **The artifacts**: one folder's entries at a time ([`ArtifactEntry`]), a
//!   text file's head ([`ArtifactPreview`]), and [`DownloadTarget`]s streamed
//!   into a [`DownloadSink`] with the stored credential. A download reads the
//!   source and writes nothing to it, so it is not a `WriteOp` (D8).
//!
//! **Frozen once landed.** A field is a §10.8 entry, as a declared path is.
//!
//! Every string is **source text**: the shell interpolates it and never
//! renders it as markup.

use serde::{Deserialize, Serialize};

use crate::{SourceError, Tone};

/// The most log lines one read answers: the cap on what the Log tab holds.
pub const LOG_LINES_MAX: u32 = 20_000;

/// The longest a log line is answered, in characters; the rest is cut and the
/// line ends in `…`. A build that prints a megabyte on one line is drawn, not
/// held.
pub const LOG_LINE_CHARS_MAX: usize = 4_000;

/// The most tests one read answers.
pub const TESTS_MAX: u32 = 5_000;

/// How many runs a test's history looks back over (R-9.11: "the last 10 runs").
pub const TEST_HISTORY_RUNS: u32 = 10;

/// The most of a text artifact a preview reads, in bytes.
pub const PREVIEW_BYTES_MAX: usize = 256 * 1024;

/// A range of a run's log (#854).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RunLog {
    /// The lines asked for, in order.
    pub lines: Vec<LogLine>,
    /// The run's steps over the **whole** log, in order, whether or not their
    /// lines are in [`lines`](Self::lines).
    pub steps: Vec<LogStep>,
    /// How many lines the whole log has.
    pub total: u64,
}

/// One line of a log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LogLine {
    /// Its place in the whole log, from 1.
    pub number: u64,
    /// The time the source stamped it with, as the source wrote it (`10:06:12`).
    pub time: Option<String>,
    /// The line, without the source's stamp and status marks.
    pub text: String,
    /// `err` or `warn` where the source marked the line so; `None` otherwise.
    pub tone: Option<Tone>,
    /// The index into [`RunLog::steps`] of the step the line belongs to;
    /// `None` for a line before, between or after the steps.
    pub step: Option<u32>,
}

/// One step of a run, as a range of its log's lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LogStep {
    /// The step's name as the source gave it.
    pub name: String,
    /// The step's first line (its heading) and its last, both in the whole log.
    pub first: u64,
    pub last: u64,
    /// How long it took, in seconds, where the log's stamps say.
    pub seconds: Option<u64>,
    /// `err` when a line in it is an error, else `warn` when one is a warning.
    pub tone: Option<Tone>,
}

/// A run's tests (#855).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RunTests {
    /// The run's tests, failed ones first, at most [`TESTS_MAX`].
    pub tests: Vec<RunTest>,
    /// The run has more tests than [`tests`](Self::tests) holds.
    pub truncated: bool,
}

/// One test of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RunTest {
    /// The source's id for the test across runs: what [`TestOutcome`]s are
    /// asked by. `None` where the source has none.
    pub id: Option<String>,
    /// The test's full name as the source gave it.
    pub name: String,
    /// The module or class it is in: its name up to the last `::` or `.`.
    pub module: Option<String>,
    pub status: TestStatus,
    /// The source mutes its failure.
    pub muted: bool,
    /// It passed in the run before and fails in this one.
    pub new_failure: bool,
    pub duration_ms: Option<u64>,
    /// What the source says about a failure: the assertion, the stack.
    pub details: Option<String>,
}

/// How a test came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum TestStatus {
    Failed,
    Passed,
    Ignored,
}

/// How one test came out in one run: its history, newest first (#855).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TestOutcome {
    /// The run, as an entity id.
    pub run: String,
    pub number: Option<String>,
    pub branch: Option<String>,
    pub status: TestStatus,
}

/// One entry of a run's artifact folder (#856).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ArtifactEntry {
    /// From the artifacts' root, `/`-separated, no leading `/`.
    pub path: String,
    pub name: String,
    /// In bytes; `None` for a folder.
    pub size: Option<u64>,
    pub folder: bool,
    /// How many entries a folder holds, where the source says.
    pub children: Option<u32>,
}

/// The head of a text artifact (#856).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ArtifactPreview {
    /// The text, at most [`PREVIEW_BYTES_MAX`] bytes of it; `None` for a file
    /// that is not text.
    pub text: Option<String>,
    /// The file is longer than [`text`](Self::text).
    pub truncated: bool,
}

/// What one download streams (#854's *Download log*, #856's downloads).
/// Tagged on `kind`.
///
/// - `log`: The run's whole log.
/// - `file`: One artifact file.
/// - `folder`: One artifact folder, as a zip.
/// - `all`: Every artifact, as a zip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DownloadTarget {
    Log,
    File { path: String },
    Folder { path: String },
    All,
}

/// Where a download's bytes go. The app's sink writes a new file in the
/// system's downloads folder.
#[async_trait::async_trait]
pub trait DownloadSink: Send {
    /// Called once, before the first byte: the file name the source suggests.
    /// The sink must not trust it as a path.
    async fn start(&mut self, file_name: &str) -> Result<(), SourceError>;
    /// The next bytes, in order.
    async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError>;
}

/// A test's module: its name up to the last `::` or `.`, `None` when it has
/// neither (#855). `sepa::retry::gives_up` is in `sepa::retry`,
/// `com.acme.PayoutTest.retries` in `com.acme.PayoutTest`.
#[must_use]
pub fn module_of(name: &str) -> Option<String> {
    let at = match (name.rfind("::"), name.rfind('.')) {
        (Some(r), Some(d)) => r.max(d),
        (Some(at), None) | (None, Some(at)) => at,
        (None, None) => return None,
    };
    let module = name[..at].trim();
    (!module.is_empty()).then(|| module.to_owned())
}

/// Whether an artifact path is one a read may send on: relative, `/`
/// separated, with no empty, `.` or `..` segment (#856). The empty path is
/// the root.
#[must_use]
pub fn is_artifact_path(path: &str) -> bool {
    path.is_empty()
        || (!path.contains('\\')
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != ".."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tests_module_is_its_name_up_to_the_last_separator() {
        assert_eq!(
            module_of("sepa::retry::gives_up_after_max_attempts").as_deref(),
            Some("sepa::retry")
        );
        assert_eq!(
            module_of("com.acme.PayoutTest.retries").as_deref(),
            Some("com.acme.PayoutTest")
        );
        assert_eq!(module_of("smoke"), None);
        assert_eq!(module_of("::odd"), None);
    }

    #[test]
    fn an_artifact_path_never_climbs_out_of_the_artifacts() {
        for good in ["", "logs", "test-report/junit.xml", "a b/c.txt"] {
            assert!(is_artifact_path(good), "{good:?}");
        }
        for bad in ["/etc/passwd", "../x", "a/../b", "a//b", "a/", "./a", "a\\b"] {
            assert!(!is_artifact_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_download_target_is_tagged_on_kind() {
        let wire = serde_json::to_value(DownloadTarget::Folder {
            path: "logs".into(),
        })
        .unwrap();
        assert_eq!(wire, serde_json::json!({"kind": "folder", "path": "logs"}));
        let all: DownloadTarget =
            serde_json::from_value(serde_json::json!({"kind": "all"})).unwrap();
        assert_eq!(all, DownloadTarget::All);
    }
}
