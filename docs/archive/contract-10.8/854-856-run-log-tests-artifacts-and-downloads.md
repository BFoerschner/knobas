- **`crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issues
  #854, #855 and #856 (2026-10-16): the run page's live reads (ADR-0022) and the download (D8),
  one new SPI module, six `Source` methods and six commands.** Spec #808, milestone TF3,
  requirements R-9.11 (Log, Tests, Artifacts), D8. No migration, no event, no grammar key, no
  `WriteOp` (a download reads the source and writes nothing to it), no declared path, no new
  module in `commands/` or `ipc/`.

  **The SPI** (`crates/knobas-source/src/run.rs`, a new public module of `knobas_source`):

  ```rust
  pub const LOG_LINES_MAX: u32 = 20_000;
  pub const LOG_LINE_CHARS_MAX: usize = 4_000;
  pub const TESTS_MAX: u32 = 5_000;
  pub const TEST_HISTORY_RUNS: u32 = 10;
  pub const PREVIEW_BYTES_MAX: usize = 256 * 1024;
  pub struct RunLog { pub lines: Vec<LogLine>, pub steps: Vec<LogStep>, pub total: u64 }
  pub struct LogLine { pub number: u64, pub time: Option<String>, pub text: String,
      pub tone: Option<Tone>, pub step: Option<u32> }
  pub struct LogStep { pub name: String, pub first: u64, pub last: u64,
      pub seconds: Option<u64>, pub tone: Option<Tone> }
  pub struct RunTests { pub tests: Vec<RunTest>, pub truncated: bool }
  pub struct RunTest { pub id: Option<String>, pub name: String, pub module: Option<String>,
      pub status: TestStatus, pub muted: bool, pub new_failure: bool,
      pub duration_ms: Option<u64>, pub details: Option<String> }
  #[serde(rename_all = "snake_case")] pub enum TestStatus { Failed, Passed, Ignored }
  pub struct TestOutcome { pub run: String, pub number: Option<String>,
      pub branch: Option<String>, pub status: TestStatus }
  pub struct ArtifactEntry { pub path: String, pub name: String, pub size: Option<u64>,
      pub folder: bool, pub children: Option<u32> }
  pub struct ArtifactPreview { pub text: Option<String>, pub truncated: bool }
  #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum DownloadTarget { Log, File { path: String }, Folder { path: String }, All }
  #[async_trait] pub trait DownloadSink: Send {
      async fn start(&mut self, file_name: &str) -> Result<(), SourceError>;
      async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError>; }
  pub fn module_of(name: &str) -> Option<String>;
  pub fn is_artifact_path(path: &str) -> bool;
  ```

  `Source` gains six methods, each with a default body answering "this adapter keeps no runs"
  (`Ok(None)`, `Ok(false)` for `download`), so no adapter but a CI one changes:

  ```rust
  async fn run_log(&self, entity: &str, from: Option<u64>, count: u32)
      -> Result<Option<run::RunLog>, SourceError>;
  async fn run_tests(&self, entity: &str) -> Result<Option<run::RunTests>, SourceError>;
  async fn test_history(&self, entity: &str, test: &str)
      -> Result<Option<Vec<run::TestOutcome>>, SourceError>;
  async fn run_artifacts(&self, entity: &str, path: &str)
      -> Result<Option<Vec<run::ArtifactEntry>>, SourceError>;
  async fn artifact_preview(&self, entity: &str, path: &str)
      -> Result<Option<run::ArtifactPreview>, SourceError>;
  async fn download(&self, entity: &str, target: &run::DownloadTarget,
      sink: &mut dyn run::DownloadSink) -> Result<bool, SourceError>;
  ```

  `run_log` answers `count` lines from line `from` (from 1), or the last `count` when `from` is
  `None`, and every step over the whole log; the log is streamed and never held whole.
  `run_tests` answers failed first. `test_history` answers the last ten outcomes of test `test`
  (a `RunTest::id`) in the run's pipeline, newest first. `run_artifacts` answers one folder,
  `""` the root, each entry with its path from the root. `artifact_preview` answers the first
  `PREVIEW_BYTES_MAX` bytes, `text: None` for a file that is not text. `download` calls
  `sink.start` with a suggested file name and then `sink.write` with the bytes in order; the sink
  must not trust the name as a path.

  **TeamCity** answers all six, read-only (`src/run_reads.rs`, `src/log.rs`):
  `GET /downloadBuildLog.html?buildId=N` streamed through a line reader that keeps the range and
  finds the top-level `Step N/M: <name>` blocks; `GET /app/rest/testOccurrences` with
  `build:(id:N),count:5001` (and, past the cap, `build:(id:N),status:FAILURE,count:5000`) and
  `fields=nextHref,testOccurrence(name,status,ignored,muted,duration,details,newFailure,test(id))`;
  the history with `test:(id:T),buildType:(id:B),count:10` and
  `fields=testOccurrence(status,ignored,build(id,number,branchName))`, the configuration read by
  `buildType(id)`; `GET /app/rest/builds/id:N/artifacts` and `/artifacts/children/<path>` with
  `ARTIFACT_FIELDS`; `/artifacts/files/<path>` for a preview and a file; `/artifacts/archived/<path>`
  for a folder; `GET /downloadArtifacts.html?buildId=N` for all. A path is percent-encoded segment
  by segment and refused when it leaves the artifacts (`is_artifact_path`); a test id that is not
  a number is refused before any request. A download's name is `<config>-<number>.log`,
  `<config>-<number>-artifacts.zip`, `<folder>.zip` or the file's name.

  **The six commands**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[tauri::command] pub async fn run_log<R>(app, run_id: String, from: Option<u64>,
      count: Option<u32>) -> Result<Option<knobas_source::run::RunLog>, IpcError>;
  #[tauri::command] pub async fn run_tests<R>(app, run_id: String)
      -> Result<Option<knobas_source::run::RunTests>, IpcError>;
  #[tauri::command] pub async fn test_history<R>(app, run_id: String, test_id: String)
      -> Result<Option<Vec<knobas_source::run::TestOutcome>>, IpcError>;
  #[tauri::command] pub async fn run_artifacts<R>(app, run_id: String, path: Option<String>)
      -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, IpcError>;
  #[tauri::command] pub async fn artifact_preview<R>(app, run_id: String, path: String)
      -> Result<Option<knobas_source::run::ArtifactPreview>, IpcError>;
  #[tauri::command] pub async fn download_run_file<R>(app, run_id: String,
      target: knobas_source::run::DownloadTarget) -> Result<Option<String>, IpcError>;
  ```

  Each routes by the run id's namespace with no mirror check (`sources::crud::run_source`, as
  `entity_view` routes), refuses `invalid` for a non-id or a path that leaves the artifacts and
  `not_found` for a source that is not configured, and maps the source's own failure through
  `sources::to_ipc`. `run_log`'s `count` defaults to and is clamped at `LOG_LINES_MAX`.
  `download_run_file` writes a new file in the system's downloads folder
  (`knobas_app::downloads::FileSink`): the suggested name cut to its last segment and stripped of
  what a file system reads as more than a name (`safe_name`), ` (n)` before the extension where
  the name is taken, the file removed when the download fails; it answers the file's path.
  Mirrored in `app/src/lib/ipc/entity.ts` (`runLog`, `runTests`, `testHistory`, `runArtifacts`,
  `artifactPreview`, `downloadRunFile` and the types), held by `entity_mirror.rs`'s
  `the_run_read_shapes_match_their_typescript_mirror`, answered by the fake IPC
  (`fake/run-reads.ts`), witnessed by `knobas-source-teamcity`'s `tests/it/run_reads.rs`
  (wiremock), `knobas-app`'s `tests/it/run_reads_ipc.rs` and, on the seeded TeamCity, by
  `live_teamcity_seeded.rs`'s `a_seeded_runs_log_tests_and_artifacts_read_whole`.

  **Not here, by ruling** (`docs/archive/2026-10-unattended-rulings.md`, #854 to #856): a run's
  steps as a declared run field (the steps bar, #1003), and a streaming send with a stall timeout
  for large downloads (#1012).
