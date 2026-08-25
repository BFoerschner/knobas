//! Credential health: what knobas last learned about a source's secret.
//!
//! One value each, one row per source -- `knobas.source_config` carries these
//! as columns (migration 0002), and this is their Rust spelling. Stream F
//! writes them; stream D's top strip and stream E's launcher board read them.

use chrono::{DateTime, Utc};

closed_vocabulary! {
    /// What the last attempt to use a source's credential established.
    ///
    /// The wire spellings are the same five literals
    /// `source_config_auth_state_chk` allows -- one list, two places, pinned
    /// by tests on both sides.
    pub enum AuthState {
        /// The credential worked.
        Ok => "ok",
        /// 401/403: the credential is wrong, expired, or locked out. **No
        /// automatic retry** -- a human must act (interfaces §8 P7).
        Unauthorized => "unauthorized",
        /// The system could not be reached. Retried with backoff.
        Unreachable => "unreachable",
        /// The source is configured but the keychain has no secret for it: the
        /// scheduler skips it entirely rather than churning backoff on a
        /// request it knows will fail (§3).
        MissingSecret => "missing_secret",
        /// Never tested, or not tested since something changed.
        Unknown => "unknown",
    }
}

/// One source's credential health -- the cheap poll the top strip's sync
/// monograms render from, and the payload of the `source:health` event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CredentialHealth {
    pub source_id: String,
    pub state: AuthState,
    pub checked_at: Option<DateTime<Utc>>,
    /// One line of detail for the sources view -- the server's own message
    /// where there is one. Never a secret.
    pub detail: Option<String>,
    /// When the credential expires, if the source says. Feeds §3's PAT expiry
    /// countdown.
    pub secret_expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same both-directions pin the run log's two vocabularies get: every
    /// variant is a spelling `source_config_auth_state_chk` allows, and it
    /// allows nothing the enum cannot produce.
    #[test]
    fn the_states_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0002_m1_cockpit.sql");
        let line = migration
            .lines()
            .find(|line| line.contains("auth_state in ("))
            .expect("source_config_auth_state_chk is missing from 0002");

        for state in AuthState::ALL {
            assert!(
                line.contains(&format!("'{}'", state.as_str())),
                "{state:?} is a variant the constraint does not allow: {line}"
            );
        }
        assert_eq!(
            line.matches('\'').count() / 2,
            AuthState::ALL.len(),
            "the constraint and the enum list different numbers of states: {line}"
        );
    }

    /// The enum and the CHECK constraint are one list in two places. The other
    /// half of this pin lives in `crates/knobas-db/tests/schema.rs`.
    /// The five states are also a TypeScript union, and the frontend branches
    /// on them: `unauthorized` is what puts *Re-enter password* on a row. A
    /// spelling that exists on one side only is a branch that can never be
    /// taken.
    #[test]
    fn the_states_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/sources.ts");
        for state in AuthState::ALL {
            let wire = serde_json::to_string(&state).unwrap();
            assert!(
                mirror.contains(&wire),
                "{wire} is missing from app/src/lib/ipc/sources.ts"
            );
        }
        // The struct half, scoped to its own interface. A whole-file
        // `contains` passes as soon as *any* interface declares a field of the
        // name -- `detail:` is also `SyncRunRow`'s neighbour in that file.
        crate::mirror::assert_shape(
            mirror,
            "CredentialHealth",
            &serde_json::to_value(CredentialHealth {
                source_id: "mock".to_owned(),
                state: AuthState::Unauthorized,
                checked_at: Some(chrono::Utc::now()),
                detail: Some("401".to_owned()),
                secret_expires_at: Some(chrono::Utc::now()),
            })
            .expect("a health serializes"),
            &[
                "checked_at",
                "detail",
                "secret_expires_at",
                "source_id",
                "state",
            ],
        );
    }

    #[test]
    fn the_wire_spelling_is_the_stored_spelling() {
        for state in AuthState::ALL {
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::json!(state.as_str()),
                "{state:?}"
            );
        }
    }

    /// The five the migration's CHECK allows, spelled out here rather than
    /// derived from [`AuthState::ALL`]: a sixth variant added without a
    /// matching `0003` would otherwise slip through, because the list it was
    /// added to is the same list the assertion iterates.
    #[test]
    fn the_five_states_are_the_five_the_check_constraint_allows() {
        let spelled: Vec<&str> = AuthState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            spelled,
            [
                "ok",
                "unauthorized",
                "unreachable",
                "missing_secret",
                "unknown"
            ],
            "source_config_auth_state_chk allows exactly these -- adding a \
             state needs a migration, not just a variant"
        );
    }
}
