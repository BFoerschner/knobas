//! What the person saves for pipelines (#2265): their command steps and their
//! named pipelines. Each list is one `knobas.setting` row, as the command
//! renderers are (`crate::renderers`): a short list a person types once is
//! not a relation, and a row that does not parse reads as an empty list.
//!
//! - A **command step** ([`CommandStep`], [`STEPS_KEY`]) is a program run
//!   under ADR-0033's rules: the words the person typed, no shell, the input
//!   on stdin, its stdout the output, killed at its timeout, refused past the
//!   pipeline's size cap. A pipeline names it by id
//!   ([`PipeStep::Command`](super::PipeStep::Command)); the webview never
//!   sends a program.
//! - A **named pipeline** ([`NamedPipeline`], [`PIPELINES_KEY`]) is a list
//!   of steps with a name. A default per file pattern names it by id
//!   (`crate::renderers::RendererDefault::pipeline`), so editing it changes
//!   every pattern that uses it.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::PipeStep;
use crate::IpcError;
use crate::renderers::{DEFAULT_TIMEOUT_SECS, MAX_TIMEOUT_SECS};
use crate::settings::{read_value, write_value};

/// The `knobas.setting` key holding the command steps.
pub const STEPS_KEY: &str = "render.steps";
/// The `knobas.setting` key holding the named pipelines.
pub const PIPELINES_KEY: &str = "render.pipelines";

/// One saved command step, Settings › Renderers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct CommandStep {
    /// Made from the name when first saved, kept through a rename.
    pub id: String,
    /// What the pipeline bar and a failure call it.
    pub name: String,
    /// The program and its arguments, as typed: quotes group words, no shell.
    pub command: String,
    /// Killed after this many seconds.
    pub timeout_secs: u32,
}

/// A command step as Settings sends it: a new one has no id yet, and no
/// timeout is the default.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct CommandStepDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

/// One named pipeline: steps a default per file pattern names by id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct NamedPipeline {
    /// Made from the name when first saved, kept through a rename.
    pub id: String,
    pub name: String,
    pub steps: Vec<PipeStep>,
}

/// A named pipeline as the viewer or Settings sends it: a new one has no id.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct NamedPipelineDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub steps: Vec<PipeStep>,
}

/// `given` when it is a free slug, else one made from `name`, `-2` and on
/// for a clash.
fn settled_id(given: Option<&str>, name: &str, fallback: &str, taken: &[&str]) -> String {
    let given = given.map(str::trim).unwrap_or_default();
    if crate::slug::is_slug(given) && !taken.contains(&given) {
        return given.to_owned();
    }
    let base = crate::slug::slug(name, fallback);
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id.as_str()) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// The saved command steps, in the person's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn command_steps(pool: &PgPool) -> Result<Vec<CommandStep>, IpcError> {
    stored_command_steps(pool).await.map_err(IpcError::internal)
}

/// [`command_steps`] as a read error, for the pipeline over a file.
pub(super) async fn stored_command_steps(pool: &PgPool) -> Result<Vec<CommandStep>, sqlx::Error> {
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(STEPS_KEY)
            .fetch_optional(pool)
            .await?;
    Ok(value
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Save the whole list of command steps, checked and trimmed, and answer it
/// as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a step with no name or a
/// command that cannot be run, naming it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save_command_steps(
    pool: &PgPool,
    drafts: &[CommandStepDraft],
) -> Result<Vec<CommandStep>, IpcError> {
    let mut out: Vec<CommandStep> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(IpcError::invalid(format!(
                "command step {} has no name",
                index + 1
            )));
        }
        let command = draft.command.trim();
        let words = knobas_core::checkout::open::words(command).map_err(|error| {
            IpcError::invalid(format!("{name}'s command cannot be run: {error}"))
        })?;
        if words.is_empty() {
            return Err(IpcError::invalid(format!("{name} has no command")));
        }
        let taken: Vec<&str> = out.iter().map(|step| step.id.as_str()).collect();
        let id = settled_id(draft.id.as_deref(), name, "step", &taken);
        out.push(CommandStep {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            timeout_secs: draft
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .clamp(1, MAX_TIMEOUT_SECS),
        });
    }
    let value = serde_json::to_value(&out).map_err(IpcError::internal)?;
    write_value(pool, STEPS_KEY, &value).await?;
    Ok(out)
}

/// The named pipelines, in the person's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn named_pipelines(pool: &PgPool) -> Result<Vec<NamedPipeline>, IpcError> {
    Ok(read_value(pool, PIPELINES_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Save the whole list of named pipelines and answer it as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a pipeline with no name or
/// no steps, or a name used twice;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save_named_pipelines(
    pool: &PgPool,
    drafts: &[NamedPipelineDraft],
) -> Result<Vec<NamedPipeline>, IpcError> {
    let mut out: Vec<NamedPipeline> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(IpcError::invalid(format!(
                "pipeline {} has no name",
                index + 1
            )));
        }
        if draft.steps.is_empty() {
            return Err(IpcError::invalid(format!("{name} has no steps")));
        }
        if out
            .iter()
            .any(|pipeline| pipeline.name.to_lowercase() == name.to_lowercase())
        {
            return Err(IpcError::invalid(format!(
                "there is a pipeline called {name} already"
            )));
        }
        let taken: Vec<&str> = out.iter().map(|pipeline| pipeline.id.as_str()).collect();
        let id = settled_id(draft.id.as_deref(), name, "pipeline", &taken);
        out.push(NamedPipeline {
            id,
            name: name.to_owned(),
            steps: draft.steps.clone(),
        });
    }
    let value = serde_json::to_value(&out).map_err(IpcError::internal)?;
    write_value(pool, PIPELINES_KEY, &value).await?;
    Ok(out)
}
