//! Credential health: what knobas last learned about a source's secret.
//!
//! One value each, one row per source -- `knobas.source_config` carries these
//! as columns (migration 0002), and this is their Rust spelling. Stream F
//! writes them; stream D's top strip and stream E's launcher board read them.

use chrono::{DateTime, Utc};

/// What the last attempt to use a source's credential established.
///
/// The wire spellings are the same five literals `source_config_auth_state_chk`
/// allows -- one list, two places, pinned by tests on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    /// The credential worked.
    Ok,
    /// 401/403: the credential is wrong, expired, or locked out. **No
    /// automatic retry** -- a human must act (interfaces §8 P7).
    Unauthorized,
    /// The system could not be reached. Retried with backoff.
    Unreachable,
    /// The source is configured but the keychain has no secret for it: the
    /// scheduler skips it entirely rather than churning backoff on a request
    /// it knows will fail (§3).
    MissingSecret,
    /// Never tested, or not tested since something changed.
    Unknown,
}

impl AuthState {
    /// Every state, so a caller cannot miss one when mapping.
    pub const ALL: [AuthState; 5] = [
        AuthState::Ok,
        AuthState::Unauthorized,
        AuthState::Unreachable,
        AuthState::MissingSecret,
        AuthState::Unknown,
    ];

    /// The stored spelling, for binding into `knobas.source_config.auth_state`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuthState::Ok => "ok",
            AuthState::Unauthorized => "unauthorized",
            AuthState::Unreachable => "unreachable",
            AuthState::MissingSecret => "missing_secret",
            AuthState::Unknown => "unknown",
        }
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

    /// The enum and the CHECK constraint are one list in two places. The other
    /// half of this pin lives in `crates/knobas-db/tests/schema.rs`.
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
