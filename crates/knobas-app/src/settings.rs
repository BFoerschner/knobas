//! The `knobas.setting` key/value store, read and written as strings.
//!
//! Migration `0002`, comment 6, calls that table the place "for app-level state
//! that has no other home", and two features now keep a handful of strings in
//! it: the checkout's clones root and its three command templates
//! (`crate::checkout`, issues #499 and #501), and the capture window's global
//! shortcut (`crate::capture`, issue #503).
//!
//! **Here rather than in `checkout.rs`, where all three were born, because
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
/// `""` would be a third one nobody reads.
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

/// One `knobas.setting` row as the JSON it holds, or nothing: for a setting
/// that is a structure rather than one string (the *Open in…* applications,
/// #844).
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the query fails.
pub async fn read_value(pool: &PgPool, key: &str) -> Result<Option<serde_json::Value>, IpcError> {
    stored_value(pool, key).await.map_err(IpcError::internal)
}

/// [`read_value`] as the database's own error, for a read that reports it
/// its own way (the pipeline over a file).
pub(crate) async fn stored_value(
    pool: &PgPool,
    key: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    sqlx::query_scalar("select value from knobas.setting where key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
}

/// A saved list's command (a user renderer's, a pipeline step's), trimmed:
/// run without a shell, so it has to split into words, and an empty one
/// cannot run.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid), naming `name`.
pub(crate) fn checked_command<'a>(name: &str, command: &'a str) -> Result<&'a str, IpcError> {
    let command = command.trim();
    knobas_core::checkout::open::words(command)
        .map_err(|error| IpcError::invalid(format!("{name}'s command cannot be run: {error}")))?;
    Ok(command)
}

/// Store one `knobas.setting` row as JSON.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn write_value(
    pool: &PgPool,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), IpcError> {
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(IpcError::internal)?;
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

// -- the first run (§14a) ---------------------------------------------------

/// How many sources are configured, and whether this is still a first run:
/// what `app_status` reports (moved here from `commands::app` by #1870).
///
/// `app_status` does not propagate a failure: it is what the boot screen
/// polls, and a window that cannot say "the database is up" because a `count`
/// failed is worse than one reporting zero sources.
///
/// # Errors
///
/// The statement's.
pub async fn sources_and_first_run(pool: &PgPool) -> Result<(u32, bool), sqlx::Error> {
    let sources: i64 = sqlx::query_scalar("select count(*) from knobas.source_config")
        .fetch_one(pool)
        .await?;
    let completed: bool = sqlx::query_scalar(
        "select exists(select 1 from knobas.setting where key = 'first_run.completed')",
    )
    .fetch_one(pool)
    .await?;

    let source_count = u32::try_from(sources).unwrap_or(u32::MAX);
    // Both halves, because they are different facts: a user who finished the
    // wizard and then deleted their only source is not on a first run.
    Ok((source_count, source_count == 0 && !completed))
}

/// The SQL [`complete_first_run`] runs. Named so the test can assert on the
/// shape rather than on a copy of it.
const COMPLETE_FIRST_RUN_SQL: &str = "insert into knobas.setting (key, value) \
     values ('first_run.completed', 'true'::jsonb) \
     on conflict (key) do update set value = excluded.value, updated_at = now()";

/// Record that the §14a wizard finished: the row [`sources_and_first_run`] reads.
/// Idempotent by `on conflict`: finishing the wizard twice -- a double click,
/// a second window -- is the same as finishing it once.
///
/// # Errors
///
/// The statement's.
pub async fn complete_first_run(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(COMPLETE_FIRST_RUN_SQL).execute(pool).await?;
    Ok(())
}

// -- Pages kept open (#2247) ---------------------------------------------------

/// The `knobas.setting` key holding *Pages kept open*: how many pages the
/// shell keeps mounted and hidden besides the one in front (keep-alive, spec
/// #2234 step 3). Namespaced `shell.` because it is the window's, read by its
/// page host and nothing in the backend.
pub const PAGES_KEPT_OPEN_KEY: &str = "shell.pages_kept_open";

/// *Pages kept open* until somebody sets it.
pub const PAGES_KEPT_OPEN_DEFAULT: u32 = 5;

/// The most *Pages kept open* takes: the cap is the only memory valve, so it
/// has a ceiling.
pub const PAGES_KEPT_OPEN_MAX: u32 = 15;

/// What a stored row means: a whole number up to the ceiling, else the default.
///
/// A row that is not a number is one an older or a broken knobas wrote, and
/// reads as unset, the rule [`read`] keeps for strings.
fn pages_kept_open_from(value: Option<&serde_json::Value>) -> u32 {
    value
        .and_then(serde_json::Value::as_u64)
        .map_or(PAGES_KEPT_OPEN_DEFAULT, |stored| {
            u32::try_from(stored.min(u64::from(PAGES_KEPT_OPEN_MAX))).unwrap_or(PAGES_KEPT_OPEN_MAX)
        })
}

/// *Pages kept open* as stored, or the default.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the query fails.
pub async fn pages_kept_open(pool: &PgPool) -> Result<u32, IpcError> {
    Ok(pages_kept_open_from(
        read_value(pool, PAGES_KEPT_OPEN_KEY).await?.as_ref(),
    ))
}

/// Store *Pages kept open*, held to the ceiling, and answer what is now stored.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_pages_kept_open(pool: &PgPool, count: u32) -> Result<u32, IpcError> {
    let kept = count.min(PAGES_KEPT_OPEN_MAX);
    write_value(pool, PAGES_KEPT_OPEN_KEY, &serde_json::Value::from(kept)).await?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved command (a renderer's, a pipeline step's) is trimmed, and one
    /// that is empty or does not split into words is refused by its owner's
    /// name.
    #[test]
    fn a_saved_command_is_trimmed_and_refused_by_name_when_it_cannot_run() {
        assert_eq!(checked_command("Cat", "  cat -n ").unwrap(), "cat -n");
        let empty = checked_command("Empty", "   ").unwrap_err();
        assert_eq!(empty.code, crate::IpcErrorCode::Invalid);
        assert!(empty.message.contains("Empty"), "{}", empty.message);
        let quote = checked_command("Bad", "cat 'x").unwrap_err();
        assert!(
            quote.message.starts_with("Bad's command cannot be run"),
            "{}",
            quote.message
        );
    }

    /// The write has to be an upsert, and it has to touch exactly the key
    /// [`sources_and_first_run`] reads.
    ///
    /// Both halves are load-bearing and neither is visible from the type: a
    /// plain `insert` makes finishing the wizard twice a primary-key violation
    /// (a double click is enough), and a key that does not match the one
    /// `app_status` looks for leaves the wizard reappearing for ever with the
    /// row sitting in the table.
    #[test]
    fn completing_the_first_run_upserts_the_key_app_status_reads() {
        assert!(
            COMPLETE_FIRST_RUN_SQL.contains("on conflict (key) do update"),
            "a plain insert makes a second Finish a primary-key violation: {COMPLETE_FIRST_RUN_SQL}"
        );
        // `sources_and_first_run`'s literal query, quoted from the same file, so a
        // rename on one side fails here rather than at run time.
        assert!(
            COMPLETE_FIRST_RUN_SQL.contains("'first_run.completed'"),
            "the wizard would write a key nothing reads: {COMPLETE_FIRST_RUN_SQL}"
        );
        // The needle is assembled so this file holds it once, in the query:
        // spelled whole here, the test would find itself and could not fail.
        let reader = include_str!("settings.rs");
        assert!(
            reader.contains(concat!("where key = ", "'first_run.completed'")),
            "app_status no longer reads the key this command writes"
        );
    }

    /// *Pages kept open* reads 5 until set, 0 to 15 once set, and the default
    /// for a row no knobas could have meant (#2247).
    #[test]
    fn pages_kept_open_reads_the_stored_count_held_to_fifteen() {
        assert_eq!(pages_kept_open_from(None), 5);
        assert_eq!(pages_kept_open_from(Some(&serde_json::json!(0))), 0);
        assert_eq!(pages_kept_open_from(Some(&serde_json::json!(9))), 9);
        assert_eq!(pages_kept_open_from(Some(&serde_json::json!(40))), 15);
        assert_eq!(pages_kept_open_from(Some(&serde_json::json!(-1))), 5);
        assert_eq!(pages_kept_open_from(Some(&serde_json::json!("7"))), 5);
    }
}
