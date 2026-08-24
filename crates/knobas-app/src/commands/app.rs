//! App lifecycle and status -- stream D (interfaces §2.1).

/// Liveness probe. Answers only once `setup` has managed the state, so a
/// successful `ping` means the database is up and migrated.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}
