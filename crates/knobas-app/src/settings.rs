//! The `knobas.setting` key/value store, read and written as strings.
//!
//! Migration `0002`, comment 6, calls that table the place "for app-level state
//! that has no other home", and two features now keep a handful of strings in
//! it: the checkout's clones root and its three command templates
//! (`crate::checkout`, issues #499 and #501), and the capture window's global
//! shortcut (`crate::capture`, issue #503).
//!
//! **Here rather than in `checkout.rs`, where both functions were born, because
//! there are now two readers and one rule.** *Unset* has a meaning for every
//! setting that goes through here -- no clones root is scanned, an action falls
//! back to the platform's default, no shortcut is registered -- so a blank value
//! is stored as no row at all rather than as `""`, which would be a third state
//! that reads as neither. One copy of that rule is one place for it to be wrong.

use sqlx::PgPool;

use crate::IpcError;

/// One `knobas.setting` row as a string, or nothing.
///
/// A row that is not a JSON string is a row an older or a broken knobas wrote;
/// it reads as *unset*, which is the miss direction and the one a person can
/// fix from the settings pane. A blank one reads as unset too -- every caller
/// of [`write`] passes a value it has already trimmed, so a blank row is one an
/// older knobas left behind rather than one this code can make.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the query fails.
pub async fn read(pool: &PgPool, key: &str) -> Result<Option<String>, IpcError> {
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    Ok(value
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .filter(|stored| !stored.trim().is_empty()))
}

/// Store one `knobas.setting` row, or delete it when there is nothing to store.
///
/// Delete rather than store a blank, for the reason this module's own docs
/// give: *unset* is a state every caller has a meaning for, and a row holding
/// `""` would be a fourth one nobody reads.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn write(pool: &PgPool, key: &str, value: Option<&str>) -> Result<(), IpcError> {
    match value {
        Some(value) => {
            sqlx::query(
                "insert into knobas.setting (key, value) values ($1, $2)
                 on conflict (key) do update set value = excluded.value, updated_at = now()",
            )
            .bind(key)
            .bind(serde_json::Value::String(value.to_owned()))
            .execute(pool)
            .await
            .map_err(IpcError::internal)?;
        }
        None => {
            sqlx::query("delete from knobas.setting where key = $1")
                .bind(key)
                .execute(pool)
                .await
                .map_err(IpcError::internal)?;
        }
    }
    Ok(())
}

/// A value a person typed, cleared to `None` when it is blank.
///
/// One function because every write through this module means the same thing by
/// an empty field -- *forget this* -- and none may store a blank.
#[must_use]
pub fn settable(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
