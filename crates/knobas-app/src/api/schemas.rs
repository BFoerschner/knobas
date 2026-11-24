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
//!
//! **A schema at an address is downloaded on a press** ([`download`]), never
//! by itself: one `GET` through the person client (as an image at an address
//! is loaded), [`SCHEMA_CAP`] at most, kept in the profile's `schemas/` under
//! its SHA-256, so the address is asked once and the file stands while the
//! address changes or goes. *Download again* in Settings asks it again.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// The most a schema downloaded may be: the explorer reads 5 MiB a file, and
/// an answer is refused whole rather than cut.
pub const SCHEMA_CAP: usize = 16 * 1024 * 1024;

/// A schema downloaded: the address asked, the file's SHA-256 (its name in
/// the profile's `schemas/`), its size and the day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct DownloadedSchema {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// `YYYY-MM-DD`, the day it was downloaded.
    pub at: String,
}

const SCHEMA: super::image::Fetching = super::image::Fetching {
    noun: "schema",
    done: "downloaded",
    verb: "download",
};

/// The folder downloaded schemas are kept in.
#[must_use]
pub fn schemas_dir(profile: &Path) -> PathBuf {
    profile.join("schemas")
}

/// Whether `text` begins as an XML Schema: its first element is `schema`, of
/// any prefix, and the text names XML Schema's namespace.
fn is_schema(text: &str) -> bool {
    let mut rest = text.trim_start_matches('\u{feff}');
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("<?") {
            let Some(end) = after.find("?>") else {
                return false;
            };
            rest = &after[end + 2..];
        } else if let Some(after) = rest.strip_prefix("<!--") {
            let Some(end) = after.find("-->") else {
                return false;
            };
            rest = &after[end + 3..];
        } else if rest.starts_with("<!") {
            let Some(end) = rest.find('>') else {
                return false;
            };
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    let Some(tag) = rest.strip_prefix('<') else {
        return false;
    };
    let name: String = tag
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
        .collect();
    let local = name.rsplit(':').next().unwrap_or_default();
    local == "schema" && text.contains("http://www.w3.org/2001/XMLSchema")
}

/// Downloads the schema at `address` into `<profile>/schemas/<sha256>.xsd`.
///
/// # Errors
///
/// No `http` or `https` address; the request failed or left its origin; the
/// server answered no `200`; the answer is past [`SCHEMA_CAP`], not text, or
/// no XML Schema; the file cannot be written.
pub async fn download(profile: &Path, address: &str) -> Result<DownloadedSchema, IpcError> {
    let url = url::Url::parse(address)
        .map_err(|error| IpcError::invalid(format!("Not an address: {error}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(IpcError::invalid(
            "knobas downloads schemas from http and https addresses only.",
        ));
    }
    let answer = super::image::one_get(&url, SCHEMA_CAP, SCHEMA).await?;
    if answer.status != 200 {
        return Err(IpcError::invalid(format!(
            "The server answered {} {}.",
            answer.status, answer.reason
        )));
    }
    if answer.cut || answer.body.len() > SCHEMA_CAP {
        return Err(IpcError::invalid(
            "The schema is larger than 16 MiB, the most knobas downloads.",
        ));
    }
    let content_type = answer
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let text = std::str::from_utf8(&answer.body).ok();
    if !text.is_some_and(is_schema) {
        let said = super::image::what_it_was(&answer.body, &content_type);
        let said = said.replace("not an image", "not an XML schema");
        return Err(IpcError::invalid(format!("The address answered {said}.")));
    }
    let sha256 = format!("{:x}", Sha256::digest(&answer.body));
    let dir = schemas_dir(profile);
    let body = answer.body.to_vec();
    let name = format!("{sha256}.xsd");
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, &body)?;
        std::fs::rename(&part, dir.join(&name))
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(|error| IpcError::internal(format!("The schema could not be kept: {error}")))?;
    Ok(DownloadedSchema {
        url: address.to_owned(),
        sha256,
        size: answer.body.len() as u64,
        at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    })
}

/// The path of a downloaded schema by its SHA-256; `None` for a name that is
/// no SHA-256, so no other file of the profile is ever read.
fn downloaded_path(profile: &Path, sha256: &str) -> Option<PathBuf> {
    (sha256.len() == 64
        && sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
    .then(|| schemas_dir(profile).join(format!("{sha256}.xsd")))
}

/// The text of a schema downloaded before; `None` where it is not kept (it
/// was forgotten, or the profile moved).
///
/// # Errors
///
/// A name that is no SHA-256.
pub async fn downloaded(profile: &Path, sha256: &str) -> Result<Option<String>, IpcError> {
    let path = downloaded_path(profile, sha256)
        .ok_or_else(|| IpcError::invalid("Not a downloaded schema's name."))?;
    let read = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(IpcError::internal)?;
    Ok(read
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
}

/// Removes a downloaded schema's file; nothing where it is not kept.
///
/// # Errors
///
/// A name that is no SHA-256, or the file cannot be removed.
pub async fn forget_download(profile: &Path, sha256: &str) -> Result<(), IpcError> {
    let path = downloaded_path(profile, sha256)
        .ok_or_else(|| IpcError::invalid("Not a downloaded schema's name."))?;
    tokio::task::spawn_blocking(move || match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(|error| IpcError::internal(format!("The schema could not be removed: {error}")))
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
    fn a_schema_is_known_by_its_first_element_and_namespace() {
        use super::is_schema;
        assert!(is_schema(
            "\u{feff}<?xml version=\"1.0\"?>\n<!-- a note -->\n<!DOCTYPE x>\n<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"/>"
        ));
        assert!(is_schema(
            "<schema xmlns=\"http://www.w3.org/2001/XMLSchema\"></schema>"
        ));
        assert!(!is_schema(
            "<!doctype html><html>http://www.w3.org/2001/XMLSchema</html>"
        ));
        assert!(!is_schema(
            "<xs:schemas xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"/>"
        ));
        assert!(!is_schema("<xs:schema/>"));
        assert!(!is_schema("<!-- never closed"));
    }

    #[test]
    fn only_a_sha256_names_a_downloaded_file() {
        use super::downloaded_path;
        let profile = std::path::Path::new("/p");
        let sha = "a".repeat(64);
        assert_eq!(
            downloaded_path(profile, &sha),
            Some(std::path::PathBuf::from(format!("/p/schemas/{sha}.xsd")))
        );
        assert_eq!(downloaded_path(profile, "../../etc/passwd"), None);
        assert_eq!(downloaded_path(profile, &"A".repeat(64)), None);
        assert_eq!(downloaded_path(profile, &"a".repeat(63)), None);
    }

    #[test]
    fn checking_is_on_unless_turned_off() {
        assert!(XmlCheck::default().on);
    }
}
