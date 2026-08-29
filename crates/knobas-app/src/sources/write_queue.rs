//! The one decision the write queue's IPC surface makes: what an *amendment*
//! is allowed to change (issue #42, story 15).
//!
//! Everything else in `commands::sources`'s queue block is a three-line
//! forward to `knobas_core::write_queue` or `knobas_sync::write_queue`, which
//! have their own batteries. This does not forward, so it lives here where a
//! test can reach it -- a `#[tauri::command]` cannot be called from one.

use knobas_source::WriteOp;
use knobas_sync::scheduler::SchedulerDeps;

use crate::IpcError;

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
}
