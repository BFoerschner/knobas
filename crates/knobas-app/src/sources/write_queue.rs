//! The decisions the write queue's IPC surface makes: what an *amendment* is
//! allowed to change (issue #42, story 15), and what a **new** write has to be
//! before it is queued at all (issue #43).
//!
//! Everything else in `commands::sources`'s queue block is a three-line
//! forward to `knobas_core::write_queue` or `knobas_sync::write_queue`, which
//! have their own batteries. This does not forward, so it lives here where a
//! test can reach it -- a `#[tauri::command]` cannot be called from one.

use knobas_core::write_queue::{QueuedWrite, WriteState};
use knobas_source::WriteOp;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};

use crate::IpcError;

/// Queue an outbound write and try to deliver it (issue #43).
///
/// `payload` is a serialized `knobas_source::WriteOp` -- the same shape
/// `pending_writes` hands back and `amend` takes -- rather than a typed
/// argument, on #42's ratified reasoning: `WriteOp` grows per milestone
/// (ADR-0006), and typing the argument would drag the SPI's enum onto the IPC
/// surface and make every growth an IPC change.
///
/// **The source is the entity's namespace**, not a separate argument.
/// Interfaces §4.1 makes the instance id and the `EntityRef` namespace the same
/// string, so a second argument could only ever agree with the id or
/// contradict it -- and a contradiction would route a write at a source the
/// target does not belong to. There is one answer, so it is read rather than
/// asked for.
///
/// Refused before anything is queued:
///
/// * a payload that is not a write op -- so an unreadable one is `invalid` at
///   the button rather than an undecodable row discovered at flush time;
/// * a target that is not an entity id, which nothing could ever deliver;
/// * a **source that is not configured**. Without this the write would queue
///   and wait forever with `no source with id ...` as its reason, which reads
///   as a source that is down rather than one that does not exist;
/// * an op the source's adapter does not declare. The adapter would refuse it
///   anyway -- battery clause 5 -- but only after a row was queued, a flush
///   was attempted and a refusal was recorded. `write_ops` is what the UI
///   renders its action bar from, so an op absent from it is one that should
///   never have been offered (story 13), and saying so at the call is the
///   difference between an interface that lies and one that does not.
///
/// # Errors
///
/// [`IpcError`] with `invalid` for a payload that is not a write op or names
/// an op the source does not declare, `not_found` for a source that is not
/// configured, and `internal` if the queue's own writes fail.
pub async fn submit(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
) -> Result<QueuedWrite, IpcError> {
    let (queued, source_id) = queue(state, payload).await?;
    flush(state, &source_id, queued.id).await?;
    Ok(queued)
}

/// [`submit`]'s first half: everything it decides, and the row, **without
/// trying to deliver it**.
///
/// Split out for one caller, `crate::time::worklog` (issue #280), and for one
/// reason: a worklog's local copy has to exist, carrying this row's id, before
/// the flush can settle the write -- because settling is what stamps Jira's
/// worklog id onto that copy, and it stamps by that id.
/// `knobas_sync::write_queue::queue` records the whole of the reasoning; the
/// pair is `queue`, then the copy, then [`flush`].
///
/// The source id comes back with the row because the caller needs it for
/// [`flush`] and it was already worked out here -- reading it out of the
/// entity id a second time would be a second answer to a settled question.
///
/// # Errors
///
/// As [`submit`].
pub(crate) async fn queue(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
) -> Result<(QueuedWrite, String), IpcError> {
    let (op, source_id) = submittable(&state.pool, state.registry.as_ref(), payload).await?;
    let queued = knobas_sync::write_queue::queue(state.scheduler.deps(), &source_id, op)
        .await
        .map_err(IpcError::internal)?;
    Ok((queued, source_id))
}

/// [`submit`]'s second half: send what the source owes, and re-read the mirror
/// if the write landed.
///
/// # Errors
///
/// As [`submit`]. A source that cannot take the write is **not** an error --
/// that is what the queue is for.
pub(crate) async fn flush(
    state: &crate::sources::SourcesState,
    source_id: &str,
    write_id: i64,
) -> Result<(), IpcError> {
    let scheduler = &state.scheduler;
    knobas_sync::write_queue::flush_source(scheduler.deps(), source_id)
        .await
        .map_err(IpcError::internal)?;

    // Story 15: a write that landed is reflected in the mirror now, rather
    // than at the next scheduled sync -- otherwise the app disagrees with
    // itself for up to five minutes right after the user acted.
    //
    // An **incremental** run, and through the scheduler rather than around it:
    // the SPI has no per-entity read (`Source::sync` is the whole of the read
    // direction), so the narrowest thing knobas can ask for is "whatever
    // changed since the cursor", which is exactly the item that was just
    // written to. `trigger` dedupes against a run already in flight, so a
    // burst of writes costs one run rather than one each.
    //
    // Only when the write actually **went**. A write that is waiting has
    // changed nothing at the source, and a run for it would re-read what
    // nobody touched.
    if landed(scheduler.deps(), write_id).await {
        refresh(scheduler, source_id).await;
    }
    Ok(())
}

/// Everything [`submit`] decides before a row exists, as one function so a
/// test can reach it: a `#[tauri::command]` cannot be called from one, and
/// neither can a path that needs a running scheduler.
///
/// Answers the decoded op and the source that owes it.
///
/// # Errors
///
/// As [`submit`].
pub(crate) async fn submittable(
    pool: &sqlx::PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    payload: serde_json::Value,
) -> Result<(WriteOp, String), IpcError> {
    let op: WriteOp = serde_json::from_value(payload)
        .map_err(|error| IpcError::invalid(format!("not a write operation: {error}")))?;
    let entity =
        knobas_core::entity::EntityRef::parse(knobas_sync::write_queue::target_entity(&op))
            .map_err(|error| IpcError::invalid(error.to_string()))?;

    let source = knobas_sync::config::get(pool, &entity.namespace)
        .await
        .map_err(IpcError::internal)?
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "{} names source {:?}, which is not configured",
                entity, entity.namespace
            ))
        })?;

    // The adapter's **template** descriptor, which is what `list_adapters`
    // serves and what the action bar is rendered from. `write_ops` is a
    // property of the adapter kind rather than of the instance, so this needs
    // no secret and no built adapter -- and a write for a source whose
    // credential is gone is exactly one the queue should be keeping, not one
    // this should refuse for want of a keychain entry.
    let declared: Vec<String> = registry
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == source.adapter_kind)
        .map(|d| d.write_ops)
        .ok_or_else(|| {
            IpcError::internal(format!(
                "source {:?} is a {:?}, which no compiled-in adapter answers to",
                source.id, source.adapter_kind
            ))
        })?;
    if !declared.iter().any(|o| o == op.identifier()) {
        return Err(IpcError::invalid(format!(
            "source {:?} does not offer {:?} -- it offers {declared:?}",
            source.id,
            op.identifier()
        )));
    }
    Ok((op, source.id))
}

/// Did this write settle as *sent*?
///
/// Re-read rather than inferred from what [`knobas_sync::write_queue::submit`]
/// returned: that row is the write **as queued**, before the attempt, which is
/// the honest shape for the caller and says nothing about the outcome.
async fn landed(deps: &SchedulerDeps, id: i64) -> bool {
    matches!(
        knobas_core::write_queue::get(&deps.pool, id).await,
        Ok(Some(row)) if row.state == WriteState::Sent
    )
}

/// Ask for an incremental sync of `source_id`, best effort.
///
/// Logged and dropped rather than raised: the write **landed**, and reporting
/// the follow-up read's failure as the write's would tell the user their edit
/// did not go when it did. The scheduler's own tick re-reads the source a few
/// minutes later either way, so the cost of a failure here is staleness, not
/// loss.
async fn refresh(scheduler: &Scheduler, source_id: &str) {
    if let Err(error) = scheduler
        .trigger(source_id, knobas_sync::SyncTrigger::Manual, None)
        .await
    {
        tracing::warn!(
            source = source_id,
            %error,
            "a write landed but the mirror could not be refreshed for it"
        );
    }
}

/// Replace a held or refused write's payload with one the user has just
/// written, then try to send it.
///
/// # Errors
///
/// `invalid` if `payload` is not a write op, or names a different op or target
/// from the row it is amending; `not_found` if no write carries `id`;
/// `conflict` if the write has already settled.
pub async fn amend(
    deps: &SchedulerDeps,
    id: i64,
    payload: serde_json::Value,
) -> Result<(), IpcError> {
    // Decoded before it is stored, so a payload the SPI cannot read is a
    // refusal the user sees now rather than an undecodable row discovered at
    // flush time, against a source, with the edit already lost from the box.
    let op: WriteOp = serde_json::from_value(payload)
        .map_err(|error| IpcError::invalid(format!("not a write operation: {error}")))?;

    let existing = knobas_core::write_queue::get(&deps.pool, id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no queued write with id {id}")))?;
    check(&existing, &op)?;

    knobas_sync::write_queue::amend(deps, id, op)
        .await
        .map_err(IpcError::internal)?
        .map(|_| ())
        .ok_or_else(|| IpcError::conflict(format!("write {id} has already settled")))
}

/// What an amendment may not change: the operation, and the target.
///
/// Both refusals are structural rather than defensive. A queued write holds a
/// **place in its entity's queue** and a **snapshot of that entity** taken when
/// it was made; an amendment that changed either would inherit an ordering
/// guarantee and a hold comparison that were established for a different
/// write. Turning a comment into a transition, or repointing it at another
/// ticket, is a *new* write -- and queueing one is how to make it.
///
/// It is also the only guard between an edit box and `WriteOp`: the payload
/// arrives as a `serde_json::Value` the webview has been holding, so "the same
/// write, edited" has to be checked rather than assumed.
fn check(existing: &knobas_core::write_queue::QueuedWrite, op: &WriteOp) -> Result<(), IpcError> {
    if op.identifier() != existing.op {
        return Err(IpcError::invalid(format!(
            "write {} is a {:?}, not a {:?} -- queue a new write instead",
            existing.id,
            existing.op,
            op.identifier()
        )));
    }
    let target = knobas_sync::write_queue::target_entity(op);
    if target != existing.entity_id {
        return Err(IpcError::invalid(format!(
            "write {} targets {:?}, not {:?} -- queue a new write instead",
            existing.id, existing.entity_id, target
        )));
    }
    Ok(())
}

/// Sync a source and **wait for a run that can see what was just written**
/// (issue #289).
///
/// [`refresh`] triggers and moves on, which is right for the write that has
/// just landed: nothing is waiting on the mirror, and story 15 only asks that
/// the app stop disagreeing with itself within the second. A caller that is
/// about to *look for what its write created* needs the other behaviour, and
/// [`Scheduler::sync_after_write`](knobas_sync::scheduler::Scheduler::sync_after_write)
/// is where it lives -- including the reason one wait is not enough (#358).
///
/// A failure to trigger, or to wait, is **not** an error: it leaves a mirror
/// that may be behind, which every caller of this already has to handle --
/// there is no link yet, and the next read draws it. Logged and dropped
/// rather than raised, exactly as [`refresh`] does with its own: nothing here
/// reaches the caller, and a warning is what makes a source that could not be
/// re-read visible to whoever is reading the log.
pub(crate) async fn sync_after_write(state: &crate::sources::SourcesState, source_id: &str) {
    if let Err(error) = state.scheduler.sync_after_write(source_id).await {
        tracing::warn!(
            source = source_id,
            %error,
            "a write landed but the mirror could not be re-read for it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IpcErrorCode;

    fn queued(op: &str, entity: &str) -> knobas_core::write_queue::QueuedWrite {
        knobas_core::write_queue::QueuedWrite {
            id: 7,
            source_id: "jira".to_owned(),
            entity_id: entity.to_owned(),
            op: op.to_owned(),
            payload: serde_json::json!({}),
            target_snapshot: serde_json::json!({}),
            state: knobas_core::write_queue::WriteState::Held,
            wait_reason: None,
            detail: None,
            queued_at: chrono::Utc::now(),
            attempted_at: None,
            attempts: 0,
            held_snapshot: None,
            settled_at: None,
            source_enabled: true,
        }
    }

    fn comment(entity: &str, body: &str) -> WriteOp {
        WriteOp::Comment {
            entity: entity.to_owned(),
            body: body.to_owned(),
        }
    }

    /// Editing the text is the whole point, and it is allowed.
    #[test]
    fn the_same_write_with_different_words_is_an_amendment() {
        let existing = queued("comment", "jira:PAY-231");
        assert!(check(&existing, &comment("jira:PAY-231", "merged")).is_ok());
    }

    /// Repointing it is not. The write would inherit another entity's place in
    /// the queue and a snapshot taken of the first one.
    #[test]
    fn an_amendment_may_not_move_the_write_to_another_target() {
        let existing = queued("comment", "jira:PAY-231");
        let error = check(&existing, &comment("jira:PAY-999", "merged")).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Invalid);
        assert!(error.message.contains("PAY-999"), "{}", error.message);
    }

    /// Nor may it change the operation. Written against a row whose `op` is
    /// one the SPI does not define yet, because that is the shape the check
    /// has to survive when `WriteOp` grows (ADR-0006): the comparison is
    /// against the *stored* identifier, not against a table of known ops.
    #[test]
    fn an_amendment_may_not_change_the_operation() {
        let existing = queued("transition", "jira:PAY-231");
        let error = check(&existing, &comment("jira:PAY-231", "merged")).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Invalid);
        assert!(error.message.contains("transition"), "{}", error.message);
    }

    // -- what a *new* write has to be (issue #43) --------------------------

    /// The shared test database, migrated.
    async fn pool() -> sqlx::PgPool {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.expect("migrations");
        pool
    }

    /// A name nothing else in this binary uses: `knobas.source_config` is
    /// shared across every test here.
    fn unique(prefix: &str) -> String {
        format!("{prefix}-{}", std::process::id())
    }

    /// A source of `kind`, so `submittable` has something to find.
    async fn configured(pool: &sqlx::PgPool, id: &str, kind: &str) {
        sqlx::query("delete from knobas.source_config where id = $1")
            .bind(id)
            .execute(pool)
            .await
            .expect("a clean slate for this id");
        knobas_sync::config::insert(
            pool,
            &knobas_sync::config::InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: "http://127.0.0.1:1".to_owned(),
                auth_kind: knobas_sync::config::AuthKind::Method(knobas_source::AuthMethod::Pat),
                config: serde_json::json!({}),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }

    fn registry() -> crate::sources::Registry {
        crate::sources::Registry::builtin()
    }

    /// The happy path, and the reason there is no `source_id` argument: the
    /// target's namespace **is** the source, so one string decides both what is
    /// written and who is asked.
    #[tokio::test]
    async fn the_targets_namespace_is_the_source_that_owes_the_write() {
        let pool = pool().await;
        let id = unique("jira-submit");
        configured(&pool, &id, "jira").await;
        let (op, source) = submittable(
            &pool,
            &registry(),
            serde_json::json!({
                "Transition": { "entity": format!("{id}:PAY-231"), "status": "In Review" }
            }),
        )
        .await
        .expect("a declared op on a configured source");
        assert_eq!(op.identifier(), "transition");
        assert_eq!(source, id);
    }

    /// A payload the SPI cannot read is `invalid` at the button rather than an
    /// undecodable row discovered at flush time, with the edit already gone
    /// from the box.
    #[tokio::test]
    async fn a_payload_that_is_not_a_write_op_is_refused_before_anything_is_queued() {
        let pool = pool().await;
        let error = submittable(&pool, &registry(), serde_json::json!({ "Nonsense": {} }))
            .await
            .expect_err("not a write op");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(
            error.message.contains("not a write operation"),
            "{}",
            error.message
        );
    }

    /// A target that is not an entity id names nothing the queue could ever
    /// order, hold or deliver.
    #[tokio::test]
    async fn a_target_that_is_not_an_entity_id_is_refused() {
        let pool = pool().await;
        let error = submittable(
            &pool,
            &registry(),
            serde_json::json!({ "Comment": { "entity": "PAY-231", "body": "hi" } }),
        )
        .await
        .expect_err("not an entity id");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
    }

    /// A source that is not configured is `not_found` rather than a queued row.
    ///
    /// Without this the write would queue and wait for ever, with `no source
    /// with id ...` as its reason -- which reads in the pending-writes panel as
    /// a source that is *down*, and invites the user to wait for something that
    /// is never coming back.
    #[tokio::test]
    async fn a_target_naming_no_configured_source_is_not_found_rather_than_queued() {
        let pool = pool().await;
        let error = submittable(
            &pool,
            &registry(),
            serde_json::json!({
                "Comment": { "entity": "jira-not-configured:PAY-231", "body": "hi" }
            }),
        )
        .await
        .expect_err("no such source");
        assert_eq!(error.code, crate::IpcErrorCode::NotFound);
        assert!(
            error.message.contains("jira-not-configured"),
            "{}",
            error.message
        );
    }

    /// Story 13: an operation a source does not support is **absent** rather
    /// than offered and failing. The action bar is rendered from that source's
    /// `write_ops`, so an op outside it is one that should never have reached
    /// here -- and refusing at the call is what keeps the interface from lying,
    /// instead of letting a row be queued, flushed, refused and shown.
    ///
    /// The refusal names what the source *does* offer, so the mismatch is
    /// diagnosable from the message.
    #[tokio::test]
    async fn an_op_the_source_does_not_offer_is_refused_rather_than_queued() {
        let pool = pool().await;
        let id = unique("teamcity-submit");
        configured(&pool, &id, "teamcity").await;
        let error = submittable(
            &pool,
            &registry(),
            serde_json::json!({
                "Comment": { "entity": format!("{id}:build:1187"), "body": "hi" }
            }),
        )
        .await
        .expect_err("teamcity does not comment");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(error.message.contains("comment"), "{}", error.message);
        assert!(
            error.message.contains("trigger_build"),
            "the refusal must say what the source does offer: {}",
            error.message
        );
    }
}
