//! *Viewed* on a pull request's files (#842, R-8.17; moved here from the
//! app's command module by #1870): which files of which pull request the
//! reader ticked, held for the blob each file held when ticked (#2276), or,
//! where the diff names no blob, for the head commit they were ticked on.

use std::collections::BTreeMap;

use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

/// The `knobas.setting` key holding which files of which pull request the
/// reader marked *Viewed* (#842, R-8.17): a JSON object of pull request entity
/// id to `{ "head": <sha or null>, "files": [<path>, …], "blobs": { <path>:
/// <blob>, … } }`: `files` ticked on a head, `blobs` ticked on a blob
/// (#2276). In
/// `knobas.setting` for the pins' reason ([`crate::builds::PINS_SETTING`]):
/// short, knobas-local state with no other home, never the source's.
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
    crate::declared_string!("$2", "commit"),
    " as head from sync.item i where i.entity_id = $1"
);

async fn pr_head(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    entity: &EntityRef,
) -> Result<Option<String>, CoreError> {
    let head: Option<Option<String>> = sqlx::query_scalar(PR_HEAD)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?;
    Ok(head.flatten())
}

/// The paths ticked by head and the paths ticked by blob, as `stored`
/// holds them for `entity`: the head ones only where they were ticked on
/// `head`.
fn held(
    stored: &serde_json::Value,
    entity: &str,
    head: Option<&str>,
) -> (Vec<String>, BTreeMap<String, String>) {
    let Some(entry) = stored.get(entity) else {
        return (Vec::new(), BTreeMap::new());
    };
    let by_head = if entry.get("head").and_then(serde_json::Value::as_str) == head {
        entry
            .get("files")
            .and_then(serde_json::Value::as_array)
            .map(|files| {
                files
                    .iter()
                    .filter_map(|f| f.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let by_blob = entry
        .get("blobs")
        .and_then(serde_json::Value::as_object)
        .map(|blobs| {
            blobs
                .iter()
                .filter_map(|(path, blob)| Some((path.clone(), blob.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    (by_head, by_blob)
}

/// Whether two blob names, each abbreviated as a diff's `index` line printed
/// it, name one blob: git lengthens its abbreviations as a repository grows,
/// so the same blob may be named by a longer prefix after a new commit.
fn same_blob(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// The paths viewed now, in path order: a file whose blob the diff names
/// (`blobs`) is viewed where it was ticked on that blob (#2276); one whose
/// blob it does not name, where it was ticked on this head (#842).
fn viewed_now(
    by_head: &[String],
    by_blob: &BTreeMap<String, String>,
    blobs: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut files: Vec<String> = by_head
        .iter()
        .filter(|path| !blobs.contains_key(*path))
        .cloned()
        .chain(
            by_blob
                .iter()
                .filter(|(path, blob)| blobs.get(*path).is_some_and(|now| same_blob(now, blob)))
                .map(|(path, _)| path.clone()),
        )
        .collect();
    files.sort();
    files.dedup();
    files
}

/// The files of pull request `pr` the reader marked *Viewed*, in path order:
/// by blob where `blobs` names the file's blob in the diff drawn (#2276), so
/// a file a new commit did not change stays viewed; by the current head, as
/// `declarations` read it, otherwise (#842).
///
/// # Errors
///
/// [`CoreError::Db`] if a read fails.
pub async fn files(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    pr: &EntityRef,
    blobs: &BTreeMap<String, String>,
) -> Result<Vec<String>, CoreError> {
    let head = pr_head(pool, declarations, pr).await?;
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(VIEWED_SETTING)
            .fetch_optional(pool)
            .await?;
    Ok(stored
        .map(|stored| {
            let (by_head, by_blob) = held(&stored, &pr.to_string(), head.as_deref());
            viewed_now(&by_head, &by_blob, blobs)
        })
        .unwrap_or_default())
}

/// Mark file `path` of pull request `pr` *Viewed* or not, and answer the
/// files now viewed, in path order, as [`files`] reads them. A file whose
/// blob `blobs` names is kept by that blob (#2276); one it does not, by the
/// current head (#842), and a tick on a head the stored ones were not made on
/// starts that head's set afresh. Blob ticks outlive a head's move.
///
/// # Errors
///
/// [`CoreError::Db`] if a read or the write fails.
pub async fn set_file(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    pr: &EntityRef,
    path: &str,
    viewed: bool,
    blobs: &BTreeMap<String, String>,
) -> Result<Vec<String>, CoreError> {
    let id = pr.to_string();
    let head = pr_head(pool, declarations, pr).await?;
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
    let (mut by_head, mut by_blob) = held(&stored, &id, head.as_deref());
    by_head.retain(|f| f != path);
    by_blob.remove(path);
    if viewed {
        match blobs.get(path) {
            Some(blob) => {
                by_blob.insert(path.to_owned(), blob.clone());
            }
            None => {
                by_head.push(path.to_owned());
                by_head.sort();
            }
        }
    }
    let mut all = match stored {
        serde_json::Value::Object(all) => all,
        _ => serde_json::Map::new(),
    };
    if by_head.is_empty() && by_blob.is_empty() {
        all.remove(&id);
    } else {
        all.insert(
            id,
            serde_json::json!({ "head": head, "files": by_head, "blobs": by_blob }),
        );
    }
    sqlx::query("update knobas.setting set value = $2, updated_at = now() where key = $1")
        .bind(VIEWED_SETTING)
        .bind(serde_json::Value::Object(all))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(viewed_now(&by_head, &by_blob, blobs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2276: a blob ticked under a 7-character name stays viewed when the
    /// diff names it by 8, and a different blob does not.
    #[test]
    fn a_blob_named_by_a_longer_prefix_is_the_same_blob() {
        let ticked: BTreeMap<String, String> = [("a.rs".to_owned(), "7aa01f9".to_owned())].into();
        let longer = [("a.rs".to_owned(), "7aa01f9c".to_owned())].into();
        assert_eq!(viewed_now(&[], &ticked, &longer), ["a.rs"]);
        let other = [("a.rs".to_owned(), "7aa01f8c".to_owned())].into();
        assert!(viewed_now(&[], &ticked, &other).is_empty());
    }
}
