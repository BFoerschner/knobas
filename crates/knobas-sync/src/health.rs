//! Credential health: what knobas last learned about a source's secret.
//!
//! One value each, one row per source -- `knobas.source_config` carries these
//! as columns (migration 0002), and this is their Rust spelling. Stream F
//! writes them; stream D's top strip and stream E's launcher board read them.

use chrono::{DateTime, Utc};

knobas_core::closed_vocabulary! {
    /// What the last attempt to use a source's credential established.
    ///
    /// - `ok`: the credential worked.
    /// - `unauthorized`: 401/403, the credential is wrong, expired, or locked
    ///   out. **No automatic retry** -- a human must act (interfaces §8 P7).
    ///   The one state the UI must act on: it is what puts *Re-enter
    ///   password* on the row rather than a shrug.
    /// - `unreachable`: the system could not be reached. Retried with
    ///   backoff.
    /// - `missing_secret`: the source is configured but the keychain has no
    ///   secret for it, so the scheduler skips it entirely rather than
    ///   churning backoff on a request it knows will fail (§3). A different
    ///   prompt from a secret that was rejected.
    /// - `unknown`: never tested, or not tested since something changed.
    ///
    /// The wire spellings are the same five literals
    /// `source_config_auth_state_chk` allows -- one list, two places, pinned
    /// by tests on both sides.
    ///
    /// The states are documented here and not on each variant because the
    /// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
    /// type's and a field's doc but not a variant's (#1325).
    #[derive(ts_rs::TS)]
    #[ts(export_to = "sources.ts")]
    pub enum AuthState {
        Ok => "ok",
        Unauthorized => "unauthorized",
        Unreachable => "unreachable",
        MissingSecret => "missing_secret",
        Unknown => "unknown",
    }
}

impl AuthState {
    /// Whether only a person can make this source answer again: the
    /// credential was refused, or there is none. The scheduler starts no
    /// sync of such a source on its own, and the write queue's tick skips it
    /// (#1628).
    #[must_use]
    pub fn needs_human(self) -> bool {
        matches!(self, AuthState::Unauthorized | AuthState::MissingSecret)
    }
}

/// One source's credential health -- the cheap poll the top strip's sync
/// monograms render from, the payload of the `source:health` event, and the
/// answer of `credential_health`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct CredentialHealth {
    pub source_id: String,
    pub state: AuthState,
    /// When the credential was last checked; `None` if it never has been.
    pub checked_at: Option<DateTime<Utc>>,
    /// One line of detail for the sources view -- the server's own message
    /// where there is one. Never a secret. On `ok`, what the last sync on
    /// the good credential left out and why (#2374): *boards unavailable:
    /// Jira Software is unlicensed or refuses this account (403)*.
    pub detail: Option<String>,
    /// When the credential expires, if the source says. Feeds §3's credential
    /// expiry countdown.
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
