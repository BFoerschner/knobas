//! The one error shape every command rejects with.
//!
//! M0 flattened every failure to a `String`, which is enough to *show* an
//! error and useless for *acting* on one. M1's frontend has to branch: a 401
//! offers *Re-enter password*, an unreachable source offers *Retry*, and a
//! command that arrived before the database was up must not look like a bug
//! (interfaces §2, ruling P1). So the wire form carries a code.
//!
//! The `message` is for humans and is not parsed by anyone. `source_id` is
//! present when the failure belongs to one source, which is what lets the
//! sources view highlight the right row without the caller threading it back.

use std::fmt;

use knobas_source::SourceError;

/// Why a command failed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
    pub source_id: Option<String>,
}

/// The failure classes the frontend branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    /// The remote system refused the credential (401/403). A human must act.
    Unauthorized,
    /// The remote system could not be reached: connect, DNS, TLS, timeout.
    Unreachable,
    /// The thing addressed does not exist.
    NotFound,
    /// The write lost a race, or a uniqueness rule rejected it.
    Conflict,
    /// The arguments cannot be honoured -- a malformed id, an unsupported
    /// combination, a mode the app is not in.
    Invalid,
    /// The app is not there yet: the database is still starting, or the
    /// source has never been configured. Retrying later is the right move.
    NotReady,
    /// Everything else. Nothing downstream can do anything but show it.
    Internal,
}

impl IpcError {
    pub fn new(code: IpcErrorCode, message: impl fmt::Display) -> Self {
        Self {
            code,
            message: message.to_string(),
            source_id: None,
        }
    }

    /// Attach the source this failure belongs to.
    #[must_use]
    pub fn with_source(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }

    pub fn internal(message: impl fmt::Display) -> Self {
        Self::new(IpcErrorCode::Internal, message)
    }

    pub fn invalid(message: impl fmt::Display) -> Self {
        Self::new(IpcErrorCode::Invalid, message)
    }

    pub fn not_found(message: impl fmt::Display) -> Self {
        Self::new(IpcErrorCode::NotFound, message)
    }

    pub fn not_ready(message: impl fmt::Display) -> Self {
        Self::new(IpcErrorCode::NotReady, message)
    }

    pub fn conflict(message: impl fmt::Display) -> Self {
        Self::new(IpcErrorCode::Conflict, message)
    }

    /// An adapter failure, keeping the classification the SPI already made.
    ///
    /// `Protocol` and `Sink` are knobas' or the source's own bug rather than
    /// something the user can act on, so both land on `Internal` -- the point
    /// of this mapping is that `Unauthorized` and `Unreachable` do not.
    #[must_use]
    pub fn from_source_error(error: &SourceError, source_id: Option<&str>) -> Self {
        let code = match error {
            SourceError::Unauthorized => IpcErrorCode::Unauthorized,
            SourceError::Unreachable(_) => IpcErrorCode::Unreachable,
            SourceError::Protocol(_) | SourceError::Sink(_) => IpcErrorCode::Internal,
        };
        let mut mapped = Self::new(code, error);
        mapped.source_id = source_id.map(ToOwned::to_owned);
        mapped
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IpcError {}

impl From<sqlx::Error> for IpcError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(error)
    }
}

impl From<knobas_core::CoreError> for IpcError {
    fn from(error: knobas_core::CoreError) -> Self {
        match error {
            knobas_core::CoreError::Duplicate => Self::conflict(error),
            knobas_core::CoreError::LinkNotFound(_)
            // An endpoint with no mirror row is the same "no such thing" as a
            // link id nothing carries: the entity has not synced yet, which is
            // a normal event and not knobas being broken.
            | knobas_core::CoreError::EndpointMissing => Self::not_found(error),
            knobas_core::CoreError::Db(_) => Self::internal(error),
        }
    }
}

impl IpcError {
    /// A failed run, keeping both the classification and the source it belongs
    /// to.
    ///
    /// `source_id` is threaded in by the caller because [`knobas_sync::SyncError`]
    /// does not carry it and the sources view needs it: `unauthorized` is
    /// exactly the code this field exists to route -- the row to highlight and
    /// offer *Re-enter password* on is the one the run was for. Dropping it
    /// makes a 401 arrive as a 401 belonging to nobody.
    #[must_use]
    pub fn from_sync_error(error: &knobas_sync::SyncError, source_id: Option<&str>) -> Self {
        let mut mapped = match error {
            knobas_sync::SyncError::Source(source) => Self::from_source_error(source, source_id),
            // A descriptor whose id cannot be a namespace is a packaging bug,
            // not something the user typed -- but it is also the one failure
            // whose message names the fix, so it is not swallowed as internal.
            // Its own `id` is the source when the caller named none.
            knobas_sync::SyncError::BadSourceId { id, .. } => {
                Self::invalid(error).with_source(source_id.unwrap_or(id))
            }
            // Asked to sync something that has no configuration row: the id
            // is a real source id as far as the engine is concerned, there is
            // simply no such source. `NotFound` is what the sources view needs
            // to distinguish "gone" from "broken" -- and its own `id` is the
            // source when the caller named none, as with `BadSourceId`.
            knobas_sync::SyncError::NotConfigured { id } => {
                Self::not_found(error).with_source(source_id.unwrap_or(id))
            }
            knobas_sync::SyncError::Db(_) => Self::internal(error),
        };
        if mapped.source_id.is_none() {
            mapped.source_id = source_id.map(ToOwned::to_owned);
        }
        mapped
    }
}

/// Without a source id, for the callers that genuinely have none. A caller
/// that *does* know which source failed uses [`IpcError::from_sync_error`].
impl From<knobas_sync::SyncError> for IpcError {
    fn from(error: knobas_sync::SyncError) -> Self {
        Self::from_sync_error(&error, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend branches on `code`, so its wire spelling is contract, not
    /// detail: `unauthorized` is what makes the sources view offer
    /// *Re-enter password* instead of shrugging (interfaces §2, P1).
    #[test]
    fn serializes_as_the_frontend_reads_it() {
        let error = IpcError::new(IpcErrorCode::Unauthorized, "401 from Jira").with_source("jira");
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "code": "unauthorized",
                "message": "401 from Jira",
                "source_id": "jira",
            })
        );
        let plain = IpcError::internal("pool closed");
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({ "code": "internal", "message": "pool closed", "source_id": null })
        );
    }

    /// Every code the enum has, in the spelling `app/src/lib/ipc/index.ts`
    /// declares. A code added on one side only is a branch the frontend can
    /// never take.
    #[test]
    fn every_code_has_its_snake_case_spelling() {
        use IpcErrorCode::*;
        for (code, wire) in [
            (Unauthorized, "unauthorized"),
            (Unreachable, "unreachable"),
            (NotFound, "not_found"),
            (Conflict, "conflict"),
            (Invalid, "invalid"),
            (NotReady, "not_ready"),
            (Internal, "internal"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), serde_json::json!(wire));
            let mirror = include_str!("../../../app/src/lib/ipc/index.ts");
            assert!(
                mirror.contains(&format!("\"{wire}\"")),
                "{wire} missing from the TS mirror"
            );
        }
    }

    /// A 401 mid-sync is the one error the UI must act on, so it may not
    /// collapse into `internal` on the way across the bridge.
    #[test]
    fn source_faults_keep_their_kind() {
        use knobas_source::SourceError;
        let cases = [
            (SourceError::Unauthorized, IpcErrorCode::Unauthorized),
            (
                SourceError::Unreachable("refused".into()),
                IpcErrorCode::Unreachable,
            ),
            (
                SourceError::Protocol("unexpected 500".into()),
                IpcErrorCode::Internal,
            ),
            (
                SourceError::Sink("pool closed".into()),
                IpcErrorCode::Internal,
            ),
        ];
        for (error, expected) in cases {
            let mapped = IpcError::from_source_error(&error, Some("jira"));
            assert_eq!(mapped.code, expected, "{error:?}");
            assert_eq!(mapped.source_id.as_deref(), Some("jira"));
            assert!(!mapped.message.is_empty());
        }
    }

    /// The run failed *for a source*, and the sources view highlights a row by
    /// `source_id`. A 401 that arrives belonging to nobody is a 401 the UI
    /// cannot offer *Re-enter password* for -- which is the whole reason the
    /// field exists.
    #[test]
    fn a_failed_run_keeps_the_source_it_belongs_to() {
        use knobas_source::SourceError;
        let unauthorized = knobas_sync::SyncError::Source(SourceError::Unauthorized);

        let routed = IpcError::from_sync_error(&unauthorized, Some("jira-eu"));
        assert_eq!(routed.code, IpcErrorCode::Unauthorized);
        assert_eq!(routed.source_id.as_deref(), Some("jira-eu"));

        // A database failure mid-run belongs to the source too: it is that
        // source's run that died.
        let db = knobas_sync::SyncError::Db(sqlx::Error::PoolClosed);
        let routed = IpcError::from_sync_error(&db, Some("jira-eu"));
        assert_eq!(routed.code, IpcErrorCode::Internal);
        assert_eq!(routed.source_id.as_deref(), Some("jira-eu"));

        // A bad descriptor id names itself when the caller named nothing.
        let bad = knobas_sync::SyncError::BadSourceId {
            id: "jira:eu".to_owned(),
            reason: "contains ':'",
        };
        let routed = IpcError::from_sync_error(&bad, None);
        assert_eq!(routed.code, IpcErrorCode::Invalid);
        assert_eq!(routed.source_id.as_deref(), Some("jira:eu"));

        // And the plain `From` still works, for callers with no id at all.
        assert_eq!(
            IpcError::from(knobas_sync::SyncError::Source(SourceError::Unauthorized)).source_id,
            None
        );
    }

    /// The conversions the command shims lean on: a `?` in a command must
    /// produce a usable code without the shim thinking about it.
    #[test]
    fn store_errors_convert() {
        let duplicate = IpcError::from(knobas_core::CoreError::Duplicate);
        assert_eq!(duplicate.code, IpcErrorCode::Conflict);
        let db = IpcError::from(sqlx::Error::PoolClosed);
        assert_eq!(db.code, IpcErrorCode::Internal);
    }

    /// Linking to an entity that is not in the mirror is `not_found`, not
    /// `internal`.
    ///
    /// The whole reason the store lifts a foreign-key violation out of
    /// [`knobas_core::CoreError::Db`]: `internal` tells the user knobas is
    /// broken and offers nothing to do about it, while the actual event --
    /// naming an entity that has not synced yet -- is the same ordinary
    /// "no such thing" `get_entity` already reports for a deep link into a
    /// corpus that has not arrived.
    #[test]
    fn a_link_endpoint_that_is_not_in_the_mirror_is_not_found() {
        let missing = IpcError::from(knobas_core::CoreError::EndpointMissing);
        assert_eq!(missing.code, IpcErrorCode::NotFound);
        // It is the store's message that crosses, not a placeholder: this is
        // the only text the user gets.
        assert_eq!(
            missing.message,
            knobas_core::CoreError::EndpointMissing.to_string()
        );
        assert!(!missing.message.is_empty());
        // ... and it is not the code a duplicate gets. The two arrive from the
        // same classifier and the UI acts on them differently -- one is
        // "nothing there", the other "already there".
        assert_ne!(
            missing.code,
            IpcError::from(knobas_core::CoreError::Duplicate).code
        );
    }
}
