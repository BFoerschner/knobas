//! What the schema explorer and the XML view's check keep (the XSD design,
//! 2026-11-23, section 17; the XML view's design, sections 10 and 11; ADR-0058).
//!
//! - [`LOCATIONS_KEY`]: the files a person pointed to for a namespace and a
//!   location, and the addresses they had knobas download. A reference found
//!   by one is step 1 of the order, in any repository.
//! - [`CHECK_KEY`]: whether an XML file is checked against its schema when one
//!   is found. On by default.
//!
//! Two `knobas.setting` rows, no migration. A backup carries them; a share
//! export carries no settings row at all.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::settings::{read_value, write_value};
use crate::{IpcError, IpcErrorCode};

/// The row of what a person pointed to.
pub const LOCATIONS_KEY: &str = "xsd.locations";
/// The row of whether XML files are checked.
pub const CHECK_KEY: &str = "xml.check";
/// The most pointed files kept: a list a person builds by hand.
pub const POINTED_CAP: usize = 500;

/// Where a pointed reference is answered from: a file of a synced repository,
/// or an address knobas downloaded on a press.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(untagged)]
#[ts(export_to = "api.ts")]
pub enum PointedTo {
    /// A file of a repository: its entity id, its path, and its name as the
    /// person saw it.
    Repo {
        repo: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        name: Option<String>,
    },
    /// An address downloaded: the address, the file's SHA-256 (its name in the
    /// profile's `schemas/`), and the day.
    Url {
        url: String,
        sha256: String,
        at: String,
    },
}

/// One reference answered by hand: the namespace and location a schema or a
/// document asked for, and what answers them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct PointedSchema {
    pub namespace: String,
    pub location: String,
    pub to: PointedTo,
}

/// Everything pointed to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct SchemaLocations {
    pub pointed: Vec<PointedSchema>,
}

/// A schema file of a synced repository that declares a namespace: step 6
/// of the order, another repository answering by itself where exactly one
/// declares it (the XSD design, 2026-11-23, section 16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct DeclaringFile {
    /// The repository's entity id.
    pub repo: String,
    /// The branch its files are read on.
    pub branch: String,
    pub path: String,
}

/// The most files one namespace answers: more than two already asks the
/// person which.
pub const DECLARING_MOST: usize = 50;

/// The `.xsd` files of every synced repository that declare `namespace`, the
/// shortest paths first, at most [`DECLARING_MOST`]. A repository the index
/// knows no entity for is left out. Blocks: call it off the async thread.
#[must_use]
pub fn declaring(index: &knobas_files::FileIndex, namespace: &str) -> Vec<DeclaringFile> {
    let repos = index.repos();
    index
        .declaring(namespace, DECLARING_MOST)
        .files
        .into_iter()
        .filter_map(|file| {
            let repo = repos.get(&file.no)?;
            Some(DeclaringFile {
                repo: repo.entity_id.clone(),
                branch: repo.branch.clone(),
                path: file.path,
            })
        })
        .collect()
}

/// Whether an XML file is checked when a schema is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct XmlCheck {
    pub on: bool,
}

impl Default for XmlCheck {
    fn default() -> Self {
        Self { on: true }
    }
}

/// What was pointed to; nothing where the row is missing or unreadable.
///
/// # Errors
///
/// The read fails.
pub async fn locations(pool: &PgPool) -> Result<SchemaLocations, IpcError> {
    Ok(read_value(pool, LOCATIONS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Keeps what was pointed to: one answer for a namespace and a location, the
/// last given winning, at most [`POINTED_CAP`].
///
/// # Errors
///
/// A list over the cap, or the write fails.
pub async fn set_locations(
    pool: &PgPool,
    list: SchemaLocations,
) -> Result<SchemaLocations, IpcError> {
    let kept = tidy(list)?;
    let value = serde_json::to_value(&kept).map_err(IpcError::internal)?;
    write_value(pool, LOCATIONS_KEY, &value).await?;
    Ok(kept)
}

/// One answer for each namespace and location, the last given winning, in
/// the order first given; refused over the cap.
fn tidy(list: SchemaLocations) -> Result<SchemaLocations, IpcError> {
    let mut out: Vec<PointedSchema> = Vec::new();
    for p in list.pointed {
        if let Some(at) = out
            .iter()
            .position(|q| q.namespace == p.namespace && q.location == p.location)
        {
            out[at] = p;
        } else {
            out.push(p);
        }
    }
    if out.len() > POINTED_CAP {
        return Err(IpcError::new(
            IpcErrorCode::Invalid,
            format!("At most {POINTED_CAP} schema files can be pointed to."),
        ));
    }
    Ok(SchemaLocations { pointed: out })
}

/// Whether XML files are checked; on where the row is missing.
///
/// # Errors
///
/// The read fails.
pub async fn check(pool: &PgPool) -> Result<XmlCheck, IpcError> {
    Ok(read_value(pool, CHECK_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Stores whether XML files are checked; answers it as stored.
///
/// # Errors
///
/// The write fails.
pub async fn set_check(pool: &PgPool, check: XmlCheck) -> Result<XmlCheck, IpcError> {
    let value = serde_json::to_value(check).map_err(IpcError::internal)?;
    write_value(pool, CHECK_KEY, &value).await?;
    Ok(check)
}

#[cfg(test)]
mod tests {
    use super::{POINTED_CAP, PointedSchema, PointedTo, SchemaLocations, XmlCheck, tidy};

    fn repo(ns: &str, loc: &str, path: &str) -> PointedSchema {
        PointedSchema {
            namespace: ns.into(),
            location: loc.into(),
            to: PointedTo::Repo {
                repo: "r1".into(),
                path: path.into(),
                name: None,
            },
        }
    }

    #[test]
    fn one_answer_a_namespace_and_location_the_last_winning() {
        let kept = tidy(SchemaLocations {
            pointed: vec![
                repo("urn:a", "a.xsd", "x/a.xsd"),
                repo("urn:b", "b.xsd", "b.xsd"),
                repo("urn:a", "a.xsd", "y/a.xsd"),
            ],
        })
        .expect("tidy");
        assert_eq!(kept.pointed.len(), 2);
        assert_eq!(kept.pointed[0], repo("urn:a", "a.xsd", "y/a.xsd"));
        // The same namespace at another location is another answer.
        let two = tidy(SchemaLocations {
            pointed: vec![repo("urn:a", "a.xsd", "1"), repo("urn:a", "other.xsd", "2")],
        })
        .expect("tidy");
        assert_eq!(two.pointed.len(), 2);
    }

    #[test]
    fn a_list_over_the_cap_is_refused() {
        let pointed = (0..=POINTED_CAP)
            .map(|i| repo(&format!("urn:{i}"), "a.xsd", "a.xsd"))
            .collect();
        assert!(tidy(SchemaLocations { pointed }).is_err());
    }

    #[test]
    fn a_downloaded_address_and_a_repository_file_read_back_as_written() {
        let json = serde_json::json!({ "pointed": [
            { "namespace": "urn:a", "location": "https://example.com/a.xsd", "to": { "url": "https://example.com/a.xsd", "sha256": "ab", "at": "2026-11-24" } },
            { "namespace": "urn:b", "location": "b.xsd", "to": { "repo": "r9", "path": "lib/b.xsd" } }
        ]});
        let read: SchemaLocations = serde_json::from_value(json).expect("read");
        assert!(matches!(read.pointed[0].to, PointedTo::Url { .. }));
        assert!(matches!(read.pointed[1].to, PointedTo::Repo { .. }));
    }

    #[test]
    fn checking_is_on_unless_turned_off() {
        assert!(XmlCheck::default().on);
    }
}
