//! Run… presets (#852, spec #808 *Builds*, R-9.7): the knobas-owned
//! `knobas.run_preset` rows (migration `0037`) behind two commands on
//! `commands::entity`, `run_presets`, `save_run_preset` and (#996)
//! `delete_run_preset`.
//!
//! A preset is what a reader started a run with, kept under a name per
//! pipeline: the branch, the parameter overrides, the two run options. It is
//! never written to the source. A name is unique per pipeline ignoring case
//! and spaces at either end, and saving a name the pipeline has replaces
//! that preset (the dialog's *Save these settings as a preset named …* has
//! no separate rename).

use knobas_core::entity::EntityRef;
use knobas_source::BuildParameter;
use sqlx::{PgPool, Row};

use crate::IpcError;

/// One preset, as `save_run_preset` takes it and `run_presets` answers it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunPreset {
    /// The pipeline's entity id; never checked against the mirror.
    pub pipeline_id: String,
    /// As the reader typed it, trimmed.
    pub name: String,
    /// `None` is the pipeline's default branch.
    pub branch: Option<String>,
    /// The overrides only, in the order the reader saw them.
    pub parameters: Vec<BuildParameter>,
    pub clean_checkout: bool,
    pub first_in_queue: bool,
}

fn row(stored: &sqlx::postgres::PgRow) -> Result<RunPreset, IpcError> {
    Ok(RunPreset {
        pipeline_id: stored.get("pipeline_id"),
        name: stored.get("name"),
        branch: stored.get("branch"),
        parameters: serde_json::from_value(stored.get("parameters")).map_err(IpcError::internal)?,
        clean_checkout: stored.get("clean_checkout"),
        first_in_queue: stored.get("first_in_queue"),
    })
}

/// The presets of `pipeline_id`, in the order they were first saved.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn list(pool: &PgPool, pipeline_id: &str) -> Result<Vec<RunPreset>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let rows = sqlx::query(
        "select pipeline_id, name, branch, parameters, clean_checkout, first_in_queue \
         from knobas.run_preset where pipeline_id = $1 \
         order by created_at, lower(btrim(name))",
    )
    .bind(entity.to_string())
    .fetch_all(pool)
    .await?;
    rows.iter().map(row).collect()
}

/// Remove the preset of `pipeline_id` named `name`, matched as the save
/// matches it: ignoring case and whitespace at either end. Other pipelines'
/// presets of the same name stay.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a pipeline that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) for a name the
/// pipeline holds no preset of; [`Internal`](crate::IpcErrorCode::Internal) if
/// the write fails.
pub async fn delete(pool: &PgPool, pipeline_id: &str, name: &str) -> Result<(), IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let removed = sqlx::query(
        "delete from knobas.run_preset \
         where pipeline_id = $1 and lower(btrim(name)) = lower(btrim($2))",
    )
    .bind(entity.to_string())
    .bind(name.trim())
    .execute(pool)
    .await?
    .rows_affected();
    if removed == 0 {
        return Err(IpcError::not_found(format!(
            "the pipeline has no preset named {:?}",
            name.trim()
        )));
    }
    Ok(())
}

/// Store `preset`, replacing the pipeline's preset of the same name, and
/// answer it as stored. The name is trimmed; a blank branch is the default
/// branch.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a pipeline that is not an
/// entity id, a blank name, or a parameter without a name;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save(pool: &PgPool, preset: RunPreset) -> Result<RunPreset, IpcError> {
    let entity = EntityRef::parse(&preset.pipeline_id).map_err(IpcError::invalid)?;
    let name = preset.name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid("a preset needs a name"));
    }
    if preset.parameters.iter().any(|p| p.name.trim().is_empty()) {
        return Err(IpcError::invalid("a parameter of the preset has no name"));
    }
    let branch = preset
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let parameters = serde_json::to_value(&preset.parameters).map_err(IpcError::internal)?;
    let stored = sqlx::query(
        "insert into knobas.run_preset as p \
           (pipeline_id, name, branch, parameters, clean_checkout, first_in_queue) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (pipeline_id, lower(btrim(name))) do update set \
           name = excluded.name, branch = excluded.branch, parameters = excluded.parameters, \
           clean_checkout = excluded.clean_checkout, first_in_queue = excluded.first_in_queue, \
           updated_at = now() \
         returning pipeline_id, name, branch, parameters, clean_checkout, first_in_queue",
    )
    .bind(entity.to_string())
    .bind(name)
    .bind(branch)
    .bind(parameters)
    .bind(preset.clean_checkout)
    .bind(preset.first_in_queue)
    .fetch_one(pool)
    .await?;
    row(&stored)
}
