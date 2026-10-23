//! The Builds frame: shims over its mirror reads (`knobas_core::builds`, #1318), the run page's live reads, pins and *This job*, and Run… presets.

use super::*;

// -- the Builds frame (ADR-0022, #848) -------------------------------------

use knobas_core::builds::RawRun;
pub(super) use knobas_core::builds::{PipelineRow, RunRow};

/// Every pipeline the sources filling the CI role declare, for the Builds
/// list. The read is CI's by definition, so it narrows itself to the role's
/// fillers (`sources::crud::fillers`, #1295, ADR-0029 as amended on
/// 2026-10-19) with no argument: a source whose records missed a CI path,
/// one whose adapter claims no CI, one whose adapter is not installed and
/// the items a removed source left behind draw no pipeline here.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn pipelines_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Vec<PipelineRow>, IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    let identity = identity_of(pool).await?;
    let fillers = crate::sources::crud::fillers(pool, registry, knobas_source::role::CI).await?;
    let mut pipelines =
        knobas_core::builds::pipelines(pool, &declarations, &identity, &fillers).await?;
    // The list reads the mirror only: a sha two repositories share links
    // nothing here, rather than one live read per pipeline (#1144).
    let runs = knobas_core::builds::recent_runs(&mut pipelines);
    with_commits(pool, &declarations, None, runs).await?;
    Ok(pipelines)
}

/// One pipeline's runs, newest first, at most `limit` (100 when `None`).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when no live
/// pipeline carries it; [`Internal`](crate::IpcErrorCode::Internal) for a
/// query failure.
///
/// The mirror alone: a run whose sha two mirrored repositories share has no
/// `commit_id` here. [`pipeline_runs_live_inner`] is what the command answers.
pub async fn pipeline_runs_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    runs_of_pipeline(pool, registry, None, pipeline_id, limit).await
}

/// One pipeline's runs as [`pipeline_runs_inner`] reads them, with a sha two
/// mirrored repositories share linked to the commit in the repository the
/// pipeline builds, read from its settings live (#1144, [`with_commits`]).
/// A failed settings read is no failure here: those runs link nothing.
///
/// # Errors
///
/// As [`pipeline_runs_inner`].
pub async fn pipeline_runs_live_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    runs_of_pipeline(
        pool,
        registry.as_ref(),
        Some((secrets, registry)),
        pipeline_id,
        limit,
    )
    .await
}

/// What both `pipeline_runs` readings share; `live` is whether a shared sha
/// may be resolved through the pipeline's settings.
async fn runs_of_pipeline(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    live: Option<LiveReads<'_>>,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let declarations = declared_paths(pool, registry).await?;
    let identity = identity_of(pool).await?;
    let Some(mut runs) =
        knobas_core::builds::pipeline_runs(pool, &declarations, &identity, &entity, limit).await?
    else {
        return Err(IpcError::not_found(format!(
            "{entity} is not a pipeline in the mirror"
        )));
    };
    let pipeline = entity.to_string();
    let pairs = runs.iter_mut().map(|r| (pipeline.as_str(), r)).collect();
    with_commits(pool, &declarations, live, pairs).await?;
    Ok(runs)
}

/// The live reads a run's commit may need (#1144): the secrets and adapters
/// to read its pipeline's settings with.
type LiveReads<'a> = (
    &'a std::sync::Arc<dyn knobas_secrets::SecretStore>,
    &'a std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
);

/// Fills each run's [`RunRow::commit_id`] from its `revision` (#1004), with
/// one read for all of them, and none when no run names a revision. Each run
/// comes with the id of its pipeline.
///
/// The rule is [`knobas_core::builds::link_commits`]'s: a sha one mirrored
/// commit holds links it; a sha two hold links the one in the repository the
/// run's pipeline builds, read from its settings live
/// ([`crate::sources::crud::shared_commits`], within its budget) and only
/// with `live`. Without it, or where the settings cannot be read in time,
/// that run links nothing.
async fn with_commits<'a>(
    pool: &PgPool,
    declarations: &knobas_core::payload::Declarations,
    live: Option<LiveReads<'_>>,
    mut runs: Vec<(&'a str, &'a mut RunRow)>,
) -> Result<(), IpcError> {
    let shas: Vec<&str> = runs
        .iter()
        .filter_map(|(_, r)| r.revision.as_deref())
        .collect();
    if shas.is_empty() {
        return Ok(());
    }
    let held = knobas_core::builds::commits_of(pool, declarations, &shas).await?;
    let shared = knobas_core::builds::shared_shas(&runs, &held);
    let resolved = match live {
        Some((secrets, registry)) => {
            crate::sources::crud::shared_commits(pool, secrets, registry, shared).await
        }
        None => knobas_core::builds::ResolvedCommits::new(),
    };
    knobas_core::builds::link_commits(&mut runs, &held, &resolved);
    Ok(())
}

/// `not_found` unless `entity` is a live pipeline of `declarations`.
async fn live_pipeline(
    pool: &PgPool,
    declarations: &knobas_core::payload::Declarations,
    entity: &EntityRef,
) -> Result<(), IpcError> {
    if knobas_core::builds::is_pipeline(pool, declarations, entity).await? {
        Ok(())
    } else {
        Err(IpcError::not_found(format!(
            "{entity} is not a pipeline in the mirror"
        )))
    }
}

/// How many older runs one scroll asks for when the caller names no cap.
const OLDER_RUNS: u32 = 50;

/// One run the source answered, read through its kind's declarations as a
/// mirrored run is (`run_columns!`): `None` for an item of a kind the
/// declarations put outside the run frame.
fn run_of_item(
    declarations: &knobas_core::payload::Declarations,
    identity: &[String],
    item: &knobas_source::SyncItem,
) -> Option<RunRow> {
    use knobas_core::payload::{
        Frame, PayloadPath, at, resolve_list, resolve_state, resolve_string, resolve_trigger,
    };
    // A count as `declared_integer!` reads one in the mirror: the first
    // candidate that lands on a JSON integer.
    let count = |paths: &[PayloadPath]| {
        paths
            .iter()
            .find_map(|path| at(&item.payload, path)?.as_i64())
    };
    let paths = declarations.get(&item.entity.namespace, &item.kind)?;
    if paths.frame != Some(Frame::Run) {
        return None;
    }
    let payload = &item.payload;
    Some(RunRow::read(
        RawRun {
            entity_id: item.entity.to_string(),
            number: resolve_string(payload, &paths.number),
            state: resolve_state(payload, &paths.state),
            branch: resolve_string(payload, &paths.branch),
            started: resolve_string(payload, &paths.started),
            finished: resolve_string(payload, &paths.finished),
            updated_at: item.updated_at,
            web_url: item.web_url.clone(),
            trigger: resolve_trigger(payload, &paths.trigger),
            started_by: resolve_string(payload, &paths.started_by),
            agent: resolve_string(payload, &paths.agent),
            current_step: resolve_string(payload, &paths.current_step),
            tests_failed: count(&paths.tests_failed),
            tests_passed: count(&paths.tests_passed),
            tests_ignored: count(&paths.tests_ignored),
            revision: resolve_list(payload, &paths.revision).into_iter().next(),
        },
        identity,
    ))
}

/// A pipeline's runs older than the run `before`, asked of its source: the
/// history beyond the window the sync keeps (ADR-0022, #851), newest first, at
/// most `limit` (50 when `None`, never more than
/// [`knobas_source::pipeline::OLDER_RUNS_MAX`]). Each run is read through the
/// run kind's declared paths, as [`pipeline_runs_inner`] reads a mirrored one,
/// and nothing is stored. An empty answer is the start of the history.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when no live
/// pipeline carries it or its source is not configured; the source's own
/// failure as [`crate::sources::to_ipc`] maps it.
pub async fn older_pipeline_runs_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    pipeline_id: &str,
    before: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let run = EntityRef::parse(before).map_err(IpcError::invalid)?;
    if run.namespace != entity.namespace {
        return Err(IpcError::invalid(format!(
            "{run} is not a run of {entity}'s source"
        )));
    }
    let declarations = crate::sources::paths::declared_paths(pool, registry.as_ref()).await?;
    live_pipeline(pool, &declarations, &entity).await?;
    let identity = identity_of(pool).await?;
    let count = limit
        .unwrap_or(OLDER_RUNS)
        .clamp(1, knobas_source::pipeline::OLDER_RUNS_MAX);
    let items = crate::sources::crud::older_runs(
        pool,
        secrets,
        registry,
        &entity.to_string(),
        &run.to_string(),
        count,
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))?;
    let mut runs: Vec<RunRow> = items
        .iter()
        .filter(|item| item.entity != run)
        .filter_map(|item| run_of_item(&declarations, &identity, item))
        .take(count as usize)
        .collect();
    let pipeline = entity.to_string();
    let pairs = runs.iter_mut().map(|r| (pipeline.as_str(), r)).collect();
    with_commits(pool, &declarations, Some((secrets, registry)), pairs).await?;
    Ok(runs)
}

/// A pipeline's settings, read live from its source (ADR-0022, #851): the
/// repositories it builds (each with its mirrored repository's id where the
/// mirror holds one), its triggers and its parameters. `None` for a source
/// that keeps no pipelines.
///
/// # Errors
///
/// As [`older_pipeline_runs_inner`].
pub async fn pipeline_settings_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    pipeline_id: &str,
) -> Result<Option<knobas_source::pipeline::PipelineSettings>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(pool, registry.as_ref()).await?;
    live_pipeline(pool, &declarations, &entity).await?;
    crate::sources::crud::pipeline_settings(pool, secrets, registry, &entity.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))
}

/// One step of the chain a run is in (#857, R-9.13): the step's pipeline,
/// its name in the source's word, and its run on the run's branch read
/// through the run kind's declarations (`None` where it has not run there).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct ChainStepRow {
    pub pipeline_id: String,
    pub name: String,
    pub run: Option<RunRow>,
}

/// The chain the run `run_id` is in, read live from its source (ADR-0022,
/// #857): one row per step in running order, each with its run (see
/// [`knobas_source::pipeline::RunChainStep::run`]). Empty for a run of a
/// pipeline in no chain, and for a source that keeps no pipelines.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when its source is
/// not configured; the source's own failure as [`crate::sources::to_ipc`]
/// maps it.
pub async fn run_chain_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
) -> Result<Vec<ChainStepRow>, IpcError> {
    let run = EntityRef::parse(run_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(pool, registry.as_ref()).await?;
    let identity = identity_of(pool).await?;
    let steps = crate::sources::crud::run_chain(pool, secrets, registry, &run.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&run.namespace)))?;
    let mut chain: Vec<ChainStepRow> = steps
        .into_iter()
        .map(|step| ChainStepRow {
            pipeline_id: step.step.pipeline,
            name: step.step.name,
            run: step
                .run
                .as_ref()
                .and_then(|item| run_of_item(&declarations, &identity, item)),
        })
        .collect();
    let pairs = chain
        .iter_mut()
        .filter_map(|step| {
            let ChainStepRow {
                pipeline_id, run, ..
            } = step;
            let pipeline_id: &str = pipeline_id;
            run.as_mut().map(|r| (pipeline_id, r))
        })
        .collect();
    with_commits(pool, &declarations, Some((secrets, registry)), pairs).await?;
    Ok(chain)
}

/// The chain one run is in, read live (#857).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`run_chain_inner`] refuses with.
#[tauri::command]
pub async fn run_chain<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<Vec<ChainStepRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_chain_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
    )
    .await
}

/// Older runs of one pipeline, beyond the mirror's window (#851).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`older_pipeline_runs_inner`] refuses with.
#[tauri::command]
pub async fn older_pipeline_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipeline_id: String,
    before: String,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    older_pipeline_runs_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &pipeline_id,
        &before,
        limit,
    )
    .await
}

/// One pipeline's settings, read live (#851).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`pipeline_settings_inner`] refuses with.
#[tauri::command]
pub async fn pipeline_settings<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipeline_id: String,
) -> Result<Option<knobas_source::pipeline::PipelineSettings>, IpcError> {
    let state = crate::sources::state(&app)?;
    pipeline_settings_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &pipeline_id,
    )
    .await
}

// -- the run page's live reads (ADR-0022; #854, #855, #856) ------------------

/// The parsed run id and its live adapter, or the refusal: `invalid` for an
/// address that is not an entity id, `not_found` for a source that is not
/// configured.
async fn run_adapter(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
) -> Result<(EntityRef, Box<dyn knobas_source::Source>), IpcError> {
    let entity = EntityRef::parse(run_id).map_err(IpcError::invalid)?;
    let source = crate::sources::crud::run_source(pool, secrets, registry, &entity.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))?;
    Ok((entity, source))
}

/// A source's failure on a run read, as [`crate::sources::to_ipc`] maps it.
fn run_read_failed(entity: &EntityRef, error: knobas_source::SourceError) -> IpcError {
    crate::sources::to_ipc(&error.into(), Some(&entity.namespace))
}

/// A range of run `run_id`'s whole log and its steps (#854): `count` lines
/// (all [`knobas_source::run::LOG_LINES_MAX`] of the cap when `None`) from
/// line `from`, or the last `count` when `from` is `None`. `None` for a
/// source that keeps no runs.
///
/// # Errors
///
/// `invalid` for an address that is not an entity id, `not_found` for a
/// source that is not configured, and the source's own failure as
/// [`crate::sources::to_ipc`] maps it (a run it does not have included).
pub async fn run_log_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
    from: Option<u64>,
    count: Option<u32>,
) -> Result<Option<knobas_source::run::RunLog>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    let count = count
        .unwrap_or(knobas_source::run::LOG_LINES_MAX)
        .clamp(1, knobas_source::run::LOG_LINES_MAX);
    source
        .run_log(&entity.to_string(), from, count)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// Run `run_id`'s tests, failed first (#855). Errors as [`run_log_inner`].
///
/// # Errors
///
/// As [`run_log_inner`].
pub async fn run_tests_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
) -> Result<Option<knobas_source::run::RunTests>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .run_tests(&entity.to_string())
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// How test `test_id` came out in the last ten runs of `run_id`'s pipeline,
/// newest first (#855).
///
/// # Errors
///
/// As [`run_log_inner`].
pub async fn test_history_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
    test_id: &str,
) -> Result<Option<Vec<knobas_source::run::TestOutcome>>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .test_history(&entity.to_string(), test_id)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// `invalid` unless `path` stays inside a run's artifacts.
fn artifact_path(path: &str) -> Result<(), IpcError> {
    if knobas_source::run::is_artifact_path(path) {
        Ok(())
    } else {
        Err(IpcError::invalid(format!(
            "{path:?} is not a path inside a run's artifacts"
        )))
    }
}

/// One folder of run `run_id`'s artifacts, the root when `path` is `None`
/// (#856).
///
/// # Errors
///
/// As [`run_log_inner`], and `invalid` for a path that leaves the artifacts.
pub async fn run_artifacts_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
    path: Option<&str>,
) -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, IpcError> {
    let path = path.unwrap_or_default();
    artifact_path(path)?;
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .run_artifacts(&entity.to_string(), path)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// The head of one text artifact of run `run_id`, for *Preview* (#856).
///
/// # Errors
///
/// As [`run_artifacts_inner`].
pub async fn artifact_preview_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
    path: &str,
) -> Result<Option<knobas_source::run::ArtifactPreview>, IpcError> {
    artifact_path(path)?;
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .artifact_preview(&entity.to_string(), path)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// Stream `target` of run `run_id` into a new file in `dir` with the stored
/// credential (D8, #856), and answer the file's path; `None` for a source
/// that keeps no runs. The file is named as the source suggests
/// ([`crate::downloads::safe_name`]), numbered where the name is taken, and a
/// download that fails leaves no file.
///
/// # Errors
///
/// As [`run_artifacts_inner`]; a file that cannot be written is `internal`.
pub async fn download_run_file_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    run_id: &str,
    target: &knobas_source::run::DownloadTarget,
    dir: &std::path::Path,
) -> Result<Option<String>, IpcError> {
    use knobas_source::run::DownloadTarget;
    if let DownloadTarget::File { path } | DownloadTarget::Folder { path } = target {
        artifact_path(path)?;
    }
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    let mut sink = crate::downloads::FileSink::new(dir);
    let streamed = source
        .download(&entity.to_string(), target, &mut sink)
        .await;
    let finished = match streamed {
        Ok(true) => sink.finish().await.map(Some),
        Ok(false) => Ok(None),
        Err(error) => Err(error),
    };
    match finished {
        Ok(path) => Ok(path.map(|p| p.display().to_string())),
        Err(error) => {
            sink.discard().await;
            Err(run_read_failed(&entity, error))
        }
    }
}

/// A range of one run's log (#854).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`run_log_inner`] refuses with.
#[tauri::command]
pub async fn run_log<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    from: Option<u64>,
    count: Option<u32>,
) -> Result<Option<knobas_source::run::RunLog>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_log_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
        from,
        count,
    )
    .await
}

/// One run's tests (#855).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn run_tests<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<Option<knobas_source::run::RunTests>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_tests_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
    )
    .await
}

/// One test's last ten outcomes (#855).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn test_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    test_id: String,
) -> Result<Option<Vec<knobas_source::run::TestOutcome>>, IpcError> {
    let state = crate::sources::state(&app)?;
    test_history_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
        &test_id,
    )
    .await
}

/// One folder of a run's artifacts (#856).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn run_artifacts<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    path: Option<String>,
) -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_artifacts_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
        path.as_deref(),
    )
    .await
}

/// The head of one text artifact (#856).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn artifact_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    path: String,
) -> Result<Option<knobas_source::run::ArtifactPreview>, IpcError> {
    let state = crate::sources::state(&app)?;
    artifact_preview_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
        &path,
    )
    .await
}

/// A run's log, an artifact, a folder or every artifact, into the downloads
/// folder (D8, #856).
///
/// # Errors
///
/// As [`run_log`], and `internal` when the system names no downloads folder.
#[tauri::command]
pub async fn download_run_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    target: knobas_source::run::DownloadTarget,
) -> Result<Option<String>, IpcError> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .map_err(|error| IpcError::internal(format!("no downloads folder: {error}")))?;
    let state = crate::sources::state(&app)?;
    download_run_file_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &run_id,
        &target,
        &dir,
    )
    .await
}

/// The Builds list (#848).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pipelines_inner`] refuses with.
#[tauri::command]
pub async fn pipelines(lifecycle: State<'_, Lifecycle>) -> Result<Vec<PipelineRow>, IpcError> {
    let pool = lifecycle.pool()?;
    pipelines_inner(&pool, lifecycle.registry()).await
}

/// One pipeline's runs (#848).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pipeline_runs_live_inner`] refuses with. The sources
/// state is managed before `Ready` is announced, so there is no moment the
/// database answers and the live read cannot: no mirror-only fallback here.
/// [`pipeline_runs_inner`] stays as the mirror read the tests drive.
#[tauri::command]
pub async fn pipeline_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipeline_id: String,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    pipeline_runs_live_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &pipeline_id,
        limit,
    )
    .await
}

// -- pins and *This job* (#849, #850) ---------------------------------------

/// Pin or unpin one pipeline (#849, R-9.3), and answer what is now stored:
/// [`knobas_core::builds::set_pinned`].
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_pipeline_pinned_inner(
    pool: &PgPool,
    pipeline_id: &str,
    pinned: bool,
) -> Result<bool, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    Ok(knobas_core::builds::set_pinned(pool, &entity, pinned).await?)
}

/// Pin or unpin a pipeline (#849).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`set_pipeline_pinned_inner`] refuses with.
#[tauri::command]
pub async fn set_pipeline_pinned(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
    pinned: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    set_pipeline_pinned_inner(&pool, &pipeline_id, pinned).await
}

// -- Run… presets (#852) -----------------------------------------------------

/// The Run… presets of one pipeline (#852, R-9.7): [`crate::run_presets::list`].
///
/// # Errors
///
/// As [`crate::run_presets::list`].
pub async fn run_presets_inner(
    pool: &PgPool,
    pipeline_id: &str,
) -> Result<Vec<crate::run_presets::RunPreset>, IpcError> {
    crate::run_presets::list(pool, pipeline_id).await
}

/// Save one Run… preset (#852, R-9.7): [`crate::run_presets::save`].
///
/// # Errors
///
/// As [`crate::run_presets::save`].
pub async fn save_run_preset_inner(
    pool: &PgPool,
    preset: crate::run_presets::RunPreset,
) -> Result<crate::run_presets::RunPreset, IpcError> {
    crate::run_presets::save(pool, preset).await
}

/// Remove one Run… preset (#996): [`crate::run_presets::delete`].
///
/// # Errors
///
/// As [`crate::run_presets::delete`].
pub async fn delete_run_preset_inner(
    pool: &PgPool,
    pipeline_id: &str,
    name: &str,
) -> Result<(), IpcError> {
    crate::run_presets::delete(pool, pipeline_id, name).await
}

/// The Run… presets of a pipeline (#852).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`run_presets_inner`] refuses with.
#[tauri::command]
pub async fn run_presets(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
) -> Result<Vec<crate::run_presets::RunPreset>, IpcError> {
    let pool = lifecycle.pool()?;
    run_presets_inner(&pool, &pipeline_id).await
}

/// Save a Run… preset, replacing the pipeline's preset of that name (#852).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`save_run_preset_inner`] refuses with.
#[tauri::command]
pub async fn save_run_preset(
    lifecycle: State<'_, Lifecycle>,
    preset: crate::run_presets::RunPreset,
) -> Result<crate::run_presets::RunPreset, IpcError> {
    let pool = lifecycle.pool()?;
    save_run_preset_inner(&pool, preset).await
}

/// Remove a pipeline's Run… preset by name (#996).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`delete_run_preset_inner`] refuses with.
#[tauri::command]
pub async fn delete_run_preset(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
    name: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    delete_run_preset_inner(&pool, &pipeline_id, &name).await
}

pub use knobas_core::builds::{JobPipeline, PipelineReason};

/// The pipelines on the job of `ticket_id` (#850), each once with its
/// reasons, by entity id: [`knobas_core::builds::job_pipelines`] over the
/// ticket's reach. A ticket that reaches nothing has none.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn job_pipelines_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    ticket_id: &str,
) -> Result<Vec<JobPipeline>, IpcError> {
    let reached = knobas_core::reach::reach(pool, ticket_id).await?;
    let declarations = declared_paths(pool, registry).await?;
    Ok(knobas_core::builds::job_pipelines(pool, &declarations, &reached).await?)
}

/// The pipelines on a job, with their reasons (#850).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`job_pipelines_inner`] refuses with.
#[tauri::command]
pub async fn job_pipelines(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Vec<JobPipeline>, IpcError> {
    let pool = lifecycle.pool()?;
    job_pipelines_inner(&pool, lifecycle.registry(), &entity_id).await
}
