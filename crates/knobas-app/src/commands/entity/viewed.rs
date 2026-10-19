//! *Viewed* on a pull request's files.

use super::*;

// -- *Viewed* on a pull request's files (#842) ------------------------------

/// The `knobas.setting` key holding which files of which pull request the
/// reader marked *Viewed* (#842, R-8.17): a JSON object of pull request entity
/// id to `{ "head": <sha or null>, "files": [<path>, …] }`. In
/// `knobas.setting` for the pins' reason ([`PINS_SETTING`]): short,
/// knobas-local state with no other home, never the source's.
///
/// A pull request's ticks hold for the head commit they were made on, the
/// one its source declares (`KindPaths.commit`); once the mirror reads
/// another head they read as none, and the next tick replaces them. A stored
/// value that is not an object reads as none and is replaced by the next tick.
pub const VIEWED_SETTING: &str = "code.viewed";

/// The head commit the mirror's record of `entity` declares, `None` where it
/// declares none or the mirror does not hold it.
const PR_HEAD: &str = concat!(
    "select ",
    knobas_core::declared_string!("$2", "commit"),
    " as head from sync.item i where i.entity_id = $1"
);

async fn pr_head(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity: &EntityRef,
) -> Result<Option<String>, IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    let head: Option<Option<String>> = sqlx::query_scalar(PR_HEAD)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?;
    Ok(head.flatten())
}

/// The files `stored` holds as viewed for `entity` at `head`: none where it
/// holds them for another head, or holds nothing readable.
fn viewed_at(stored: &serde_json::Value, entity: &str, head: Option<&str>) -> Vec<String> {
    let Some(entry) = stored.get(entity) else {
        return Vec::new();
    };
    if entry.get("head").and_then(serde_json::Value::as_str) != head {
        return Vec::new();
    }
    let mut files: Vec<String> = entry
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.dedup();
    files
}

/// The files of pull request `pr_id` the reader marked *Viewed* on its
/// current head (#842), in path order.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
pub async fn pr_viewed_files_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pr_id: &str,
) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let head = pr_head(pool, registry, &entity).await?;
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(VIEWED_SETTING)
            .fetch_optional(pool)
            .await?;
    Ok(stored
        .map(|stored| viewed_at(&stored, &entity.to_string(), head.as_deref()))
        .unwrap_or_default())
}

/// Mark one file of pull request `pr_id` *Viewed* or not on its current head
/// (#842), and answer the files now viewed there, in path order. A tick on a
/// head the stored ones were not made on starts that head's set afresh.
///
/// # Errors
///
/// As [`pr_viewed_files_inner`]; [`Internal`](crate::IpcErrorCode::Internal)
/// if the write fails.
pub async fn set_pr_file_viewed_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pr_id: &str,
    path: &str,
    viewed: bool,
) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let head = pr_head(pool, registry, &entity).await?;
    let mut tx = pool.begin().await?;
    // The row first, then held: two ticks at once each read the other's.
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, '{}'::jsonb)
         on conflict (key) do nothing",
    )
    .bind(VIEWED_SETTING)
    .execute(&mut *tx)
    .await?;
    let stored: serde_json::Value =
        sqlx::query_scalar("select value from knobas.setting where key = $1 for update")
            .bind(VIEWED_SETTING)
            .fetch_one(&mut *tx)
            .await?;
    let mut files = viewed_at(&stored, &id, head.as_deref());
    files.retain(|f| f != path);
    if viewed {
        files.push(path.to_owned());
        files.sort();
    }
    let mut all = match stored {
        serde_json::Value::Object(all) => all,
        _ => serde_json::Map::new(),
    };
    if files.is_empty() {
        all.remove(&id);
    } else {
        all.insert(id, serde_json::json!({ "head": head, "files": files }));
    }
    sqlx::query("update knobas.setting set value = $2, updated_at = now() where key = $1")
        .bind(VIEWED_SETTING)
        .bind(serde_json::Value::Object(all))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(files)
}

/// The files of a pull request marked *Viewed* on its head (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pr_viewed_files_inner`] refuses with.
#[tauri::command]
pub async fn pr_viewed_files(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    pr_viewed_files_inner(&pool, lifecycle.registry(), &pr_id).await
}

/// Mark a pull request's file *Viewed* or not (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`set_pr_file_viewed_inner`] refuses with.
#[tauri::command]
pub async fn set_pr_file_viewed(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
    path: String,
    viewed: bool,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    set_pr_file_viewed_inner(&pool, lifecycle.registry(), &pr_id, &path, viewed).await
}
