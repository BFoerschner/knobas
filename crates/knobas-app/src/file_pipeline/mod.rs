//! Pipelines (#2264, spec #2234 step 5): the ordered steps between a file's
//! bytes and its renderer, run in the backend. Each step takes bytes (text
//! is bytes that are UTF-8) and gives bytes; the last step's output is drawn
//! by the renderer its [`PipeRun::Done`] name picks (`events.json.gz` through
//! `gunzip` is `events.json`).
//!
//! # Limits
//!
//! - **Size.** Every step's output is capped at [`Limits::cap`] (200 MiB),
//!   checked as it is written ([`steps::Capped`]): a decompressor is stopped
//!   at the byte that passes it, so a gzip bomb never expands in memory.
//! - **Time.** Every step has [`Limits::timeout`]. A built-in step is Rust
//!   with no process to kill, so it runs on a worker thread of its own and
//!   the pipeline stops waiting at the deadline. The thread is then
//!   *abandoned, not killed*: its stop flag is set, which every write it makes
//!   checks, so a decompressor or a jq filter that keeps producing output
//!   ends at its next write. A filter that loops without writing (`last(range(infinite))`)
//!   runs on until the process exits; [`WORKERS`] threads at most are
//!   ever held that way, because a worker keeps its permit until its thread
//!   ends, and a run that cannot get a permit by its deadline times out.
//! - **A failure** is a [`PipeFailure`]: the step's place and name, what it
//!   said, and the head of the input it was given.
//!
//! # Kept
//!
//! A run over a file read at a whole sha (the adapter's `bytes_standing` is
//! *Fixed*, ADR-0032) is kept beside the bytes in [`LiveSources`], keyed by
//! (sha, path, pipeline): the same pipeline over the same file costs no work
//! the second time. A timed-out run is not kept; it may finish next time.
//!
//! # User command steps (#2265)
//!
//! A command step joins [`PipeStep`] as one more variant, run by
//! `crate::renderers::run_argv` under ADR-0033's rules rather than on a
//! worker: [`run_step`] is the one place that picks how a step runs.

mod jq;
mod steps;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::Semaphore;

use knobas_secrets::SecretStore;
use knobas_source::view::{BYTES_CAP, FileBytes};
use knobas_sync::scheduler::AdapterRegistry;

use crate::sources::SourcesError;
use crate::sources::live::LiveSources;
use steps::Failure;

/// The most any one step may write: about 200 MB, the spec's cap.
pub const SIZE_CAP: u64 = 200 * 1024 * 1024;
/// How long any one built-in step may take.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Built-in steps running at once, abandoned ones included.
pub const WORKERS: usize = 4;
/// A worker's stack: deep enough for a jq filter's honest recursion.
const WORKER_STACK: usize = 256 * 1024 * 1024;
/// How much of a failed step's input its failure shows.
const INPUT_HEAD: usize = 4 * 1024;

static PERMITS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(WORKERS)));

/// A data format the pretty-print and convert steps read and write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipeFormat {
    Json,
    Yaml,
    Toml,
}

impl PipeFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
        }
    }
}

/// One step of a pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "step", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipeStep {
    Base64Decode,
    Base64Encode,
    Gunzip,
    Unzstd,
    Bunzip2,
    Unxz,
    /// A jq filter, run by `jaq`.
    Jq {
        filter: String,
    },
    /// The input laid out again in its own format.
    Pretty {
        format: PipeFormat,
    },
    /// The input read as `from`, written as `to`.
    Convert {
        from: PipeFormat,
        to: PipeFormat,
    },
    HexDump,
}

impl PipeStep {
    /// What a failure and the editor call it.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Base64Decode => "base64 decode".to_owned(),
            Self::Base64Encode => "base64 encode".to_owned(),
            Self::Gunzip => "gunzip".to_owned(),
            Self::Unzstd => "zstd decompress".to_owned(),
            Self::Bunzip2 => "bzip2 decompress".to_owned(),
            Self::Unxz => "xz decompress".to_owned(),
            Self::Jq { .. } => "jq".to_owned(),
            Self::Pretty { format } => format!("pretty-print {}", format.name()),
            Self::Convert { from, to } => format!("{} to {}", from.name(), to.name()),
            Self::HexDump => "hex dump".to_owned(),
        }
    }

    /// The name the output goes by, given the input's: an unwrap takes its
    /// extension off, a step that writes a format puts that format's on.
    fn rename(&self, name: &str) -> String {
        let strip = |extensions: &[&str]| {
            extensions
                .iter()
                .find_map(|ext| strip_extension(name, ext))
                .unwrap_or_else(|| name.to_owned())
        };
        match self {
            Self::Base64Decode => strip(&["b64", "base64"]),
            Self::Base64Encode => format!("{name}.b64"),
            Self::Gunzip => {
                if let Some(stem) = strip_extension(name, "tgz") {
                    format!("{stem}.tar")
                } else {
                    strip(&["gz", "gzip"])
                }
            }
            Self::Unzstd => strip(&["zst", "zstd"]),
            Self::Bunzip2 => strip(&["bz2", "bz"]),
            Self::Unxz => strip(&["xz"]),
            Self::Jq { .. } => with_extension(name, "json"),
            Self::Pretty { format } | Self::Convert { to: format, .. } => {
                if extension(name).is_some_and(|ext| format_of(ext) == Some(*format)) {
                    name.to_owned()
                } else {
                    with_extension(name, format.extension())
                }
            }
            Self::HexDump => format!("{name}.hexdump"),
        }
    }
}

fn extension(name: &str) -> Option<&str> {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext)
}

fn format_of(ext: &str) -> Option<PipeFormat> {
    match ext.to_ascii_lowercase().as_str() {
        "json" => Some(PipeFormat::Json),
        "yaml" | "yml" => Some(PipeFormat::Yaml),
        "toml" => Some(PipeFormat::Toml),
        _ => None,
    }
}

fn strip_extension(name: &str, ext: &str) -> Option<String> {
    extension(name)
        .filter(|found| found.eq_ignore_ascii_case(ext))
        .map(|found| name[..name.len() - found.len() - 1].to_owned())
}

fn with_extension(name: &str, ext: &str) -> String {
    match extension(name) {
        Some(found) => format!("{}.{ext}", &name[..name.len() - found.len() - 1]),
        None => format!("{name}.{ext}"),
    }
}

/// How far any one step may go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most bytes a step may write.
    pub cap: u64,
    /// How long a step may take.
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            cap: SIZE_CAP,
            timeout: STEP_TIMEOUT,
        }
    }
}

/// A pipeline's output: text where it is UTF-8, else bytes in base64.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipeOutput {
    Text { text: String },
    Bytes { base64: String, size: u64 },
}

/// Why a step stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipeWhy {
    /// The step's own error.
    Error,
    /// Its output passed the size cap.
    TooLarge,
    /// It ran past its timeout.
    TimedOut,
}

/// The head of the input a failed step was given.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct PipeInput {
    /// Its whole size in bytes.
    pub size: u64,
    /// Its first 4 KiB as text, or, when it is not text, its first 256
    /// bytes as a hex dump.
    pub head: String,
    /// The head is a hex dump.
    pub binary: bool,
}

/// A step that gave no output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct PipeFailure {
    /// Its place in the pipeline, from 0.
    pub index: u32,
    pub name: String,
    pub why: PipeWhy,
    /// What it said, in one line.
    pub error: String,
    pub input: PipeInput,
}

/// What a pipeline answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipeRun {
    /// Every step ran; `name` is what the output goes by (it picks the
    /// renderer).
    Done {
        name: String,
        output: PipeOutput,
    },
    Failed {
        failure: PipeFailure,
    },
}

impl PipeRun {
    /// What it weighs in the read cache.
    pub(crate) fn weight(&self) -> usize {
        match self {
            Self::Done { output, name } => {
                name.len()
                    + match output {
                        PipeOutput::Text { text } => text.len(),
                        PipeOutput::Bytes { base64, .. } => base64.len(),
                    }
            }
            Self::Failed { failure } => failure.error.len() + failure.input.head.len(),
        }
    }

    fn timed_out(&self) -> bool {
        matches!(self, Self::Failed { failure } if failure.why == PipeWhy::TimedOut)
    }
}

/// Bytes between steps: the file's own, shared with the read cache, or a
/// step's output.
#[derive(Clone)]
enum Carried {
    File(Arc<FileBytes>),
    PipeStep(Arc<Vec<u8>>),
}

impl Carried {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::File(file) => match file.as_ref() {
                FileBytes::Read(bytes) => bytes,
                FileBytes::TooLarge { .. } => &[],
            },
            Self::PipeStep(bytes) => bytes,
        }
    }
}

/// `steps` over `bytes`, the file `name`.
pub async fn run_bytes(steps: &[PipeStep], bytes: Vec<u8>, name: &str, limits: Limits) -> PipeRun {
    run(steps, Carried::PipeStep(Arc::new(bytes)), name, limits).await
}

async fn run(steps: &[PipeStep], mut carried: Carried, name: &str, limits: Limits) -> PipeRun {
    let mut name = name.to_owned();
    for (index, step) in steps.iter().enumerate() {
        match run_step(step, carried.clone(), limits).await {
            Ok(output) => carried = Carried::PipeStep(Arc::new(output)),
            Err((why, error)) => {
                return PipeRun::Failed {
                    failure: PipeFailure {
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                        name: step.name(),
                        why,
                        error,
                        input: shown(carried.bytes()),
                    },
                };
            }
        }
        name = step.rename(&name);
    }
    let bytes = carried.bytes();
    let output = match std::str::from_utf8(bytes) {
        Ok(text) => PipeOutput::Text {
            text: text.to_owned(),
        },
        Err(_) => PipeOutput::Bytes {
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            size: bytes.len() as u64,
        },
    };
    PipeRun::Done { name, output }
}

/// One step's output, or why it has none and what to say.
async fn run_step(
    step: &PipeStep,
    input: Carried,
    limits: Limits,
) -> Result<Vec<u8>, (PipeWhy, String)> {
    let name = step.name();
    let size = input.bytes().len() as u64;
    on_worker(step.clone(), input, limits)
        .await
        .map_err(|failure| match failure {
            Failure::Error(error) => (PipeWhy::Error, error),
            Failure::TooLarge => (
                PipeWhy::TooLarge,
                format!(
                    "{name} was stopped at the size cap: its {} of input expands past {}",
                    sized(size),
                    sized(limits.cap)
                ),
            ),
            Failure::Stopped => (
                PipeWhy::TimedOut,
                format!("{name} did not finish in {}", seconds(limits.timeout)),
            ),
            Failure::Busy => (
                PipeWhy::TimedOut,
                format!(
                    "{name} did not start in {}: every pipeline worker is busy with an earlier step",
                    seconds(limits.timeout)
                ),
            ),
        })
}

/// A built-in step on a worker thread of its own, waited for until the
/// step's deadline (see the module's *Limits*).
async fn on_worker(step: PipeStep, input: Carried, limits: Limits) -> Result<Vec<u8>, Failure> {
    let deadline = tokio::time::Instant::now() + limits.timeout;
    let Ok(Ok(permit)) = tokio::time::timeout_at(deadline, PERMITS.clone().acquire_owned()).await
    else {
        return Err(Failure::Busy);
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (answer, answered) = tokio::sync::oneshot::channel();
    let told = stop.clone();
    std::thread::Builder::new()
        .name("knobas-pipeline-step".to_owned())
        .stack_size(WORKER_STACK)
        .spawn(move || {
            let _permit = permit;
            let _ = answer.send(steps::run(&step, input.bytes(), limits.cap, &told));
        })
        .map_err(|error| Failure::Error(format!("no worker thread: {error}")))?;
    match tokio::time::timeout_at(deadline, answered).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => Err(Failure::Error(
            "the step stopped without an answer".to_owned(),
        )),
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err(Failure::Stopped)
        }
    }
}

/// The head of `bytes`, as [`PipeInput`] says.
fn shown(bytes: &[u8]) -> PipeInput {
    let head = &bytes[..bytes.len().min(INPUT_HEAD)];
    let text = match std::str::from_utf8(head) {
        Ok(text) => Some(text),
        // Cut inside a character at the end: the rest is text.
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&head[..error.valid_up_to()]).ok()
        }
        Err(_) => None,
    };
    let (head, binary) = text.map_or_else(
        || {
            let stop = AtomicBool::new(false);
            let dump = steps::run(
                &PipeStep::HexDump,
                &bytes[..bytes.len().min(256)],
                u64::MAX,
                &stop,
            )
            .unwrap_or_default();
            (String::from_utf8_lossy(&dump).into_owned(), true)
        },
        |text| (text.to_owned(), false),
    );
    PipeInput {
        size: bytes.len() as u64,
        head,
        binary,
    }
}

/// `bytes` in the unit a person reads it in.
fn sized(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let at = |unit: f64| bytes as f64 / unit;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", at(1024.0))
    } else {
        let mib = at(1024.0 * 1024.0);
        if mib.fract() == 0.0 {
            format!("{mib:.0} MiB")
        } else {
            format!("{mib:.1} MiB")
        }
    }
}

fn seconds(timeout: Duration) -> String {
    if timeout.subsec_millis() == 0 {
        format!("{} s", timeout.as_secs())
    } else {
        format!("{} ms", timeout.as_millis())
    }
}

/// What a pipeline over a file came to.
#[derive(Debug)]
pub enum Piped {
    Ran(Arc<PipeRun>),
    /// The file is over the byte read's cap.
    TooLarge {
        size: u64,
    },
    /// The source reads no bytes.
    NoBytes,
}

/// `steps` over the bytes of the file at `path` in `entity_id`'s tree, at
/// `commit` (a file block's, handed back): read as `crud::file_bytes` reads
/// them, and kept with them where the read is at a whole sha.
///
/// # Errors
///
/// As `crud::file_bytes`. A step's failure is an answer, not an error.
#[allow(clippy::too_many_arguments)]
pub async fn run_on_file(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    path: &str,
    commit: Option<&str>,
    steps: &[PipeStep],
    limits: Limits,
) -> Result<Piped, SourcesError> {
    let lent = live.source(pool, secrets, registry, entity_id).await?;
    let pipeline = format!(
        "{}:{}:{}",
        limits.cap,
        limits.timeout.as_millis(),
        serde_json::to_string(steps).unwrap_or_default()
    );
    if let Some(kept) = live.kept_run(&lent, entity_id, path, commit, &pipeline) {
        return Ok(Piped::Ran(kept));
    }
    let Some(file) = live
        .bytes(&lent, entity_id, path, commit, BYTES_CAP)
        .await?
    else {
        return Ok(Piped::NoBytes);
    };
    if let FileBytes::TooLarge { size } = file.as_ref() {
        return Ok(Piped::TooLarge { size: *size });
    }
    let ran = Arc::new(run(steps, Carried::File(file), path, limits).await);
    if !ran.timed_out() {
        live.keep_run(&lent, entity_id, path, commit, &pipeline, ran.clone());
    }
    Ok(Piped::Ran(ran))
}
