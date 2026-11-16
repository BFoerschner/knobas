//! Where a folder is read from and what it holds now (ADR-0044): the clone's
//! tree at the commit when the clone holds it, else the forge's listing of
//! every path at the entity's reference.
//!
//! Every limit is checked against the listing before any file is read: the
//! listing holds every size, so a folder too big to copy costs no request
//! and says by how much.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use knobas_source::view::{Block, EntryKind, FileBytes};
use sqlx::PgPool;

use super::FolderRenderer;
use super::copy::{self, Bytes, Item, ItemKind};
use super::runs::{FolderFailure, ReadFrom, Resolved, Unresolved, clone_key, forge_key};
use crate::IpcError;
use crate::checkout::local;

/// What reads from the forge need: the app's sources.
#[derive(Clone)]
pub struct Forge {
    pub pool: PgPool,
    pub secrets: Arc<dyn knobas_secrets::SecretStore>,
    pub registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    pub live: crate::sources::live::LiveSources,
}

/// The folder `folder` of `entity_id` at `commit` (the mirror's tip when
/// `None`), for `renderer`. `paths` is the section the forge lists every
/// path of the tree in, as the page read it, for a repository with no clone.
///
/// # Errors
///
/// A query failure.
pub async fn resolve(
    pool: &PgPool,
    forge: Option<&Forge>,
    renderer: &FolderRenderer,
    entity_id: &str,
    folder: &str,
    commit: Option<&str>,
    paths: Option<&str>,
) -> Result<Result<Resolved, Unresolved>, IpcError> {
    let folder = folder.trim_matches('/');
    let copy_name = copy::copy_name(folder);
    let source_id = source_of(pool, entity_id).await?;
    let commit = match commit {
        Some(sha) => Some(sha.to_owned()),
        None => match forge {
            Some(forge) => crate::checkout::mirror_tip(pool, forge.registry.as_ref(), entity_id)
                .await?
                .map(|tip| tip.commit),
            None => None,
        },
    };
    let unresolved = |failure, from| Unresolved {
        failure,
        from,
        commit: commit.clone(),
        source_id: source_id.clone(),
    };

    // The clone, where it holds the commit.
    let checkout = crate::checkout::resolve(pool, entity_id).await?.path;
    if let (Some(path), Some(sha)) = (&checkout, &commit)
        && let Some(tree) = local::tree(Path::new(path), sha).await
    {
        if tree.truncated {
            return Ok(Err(unresolved(
                FolderFailure::Copy {
                    reason: "this tree is past what knobas reads of a clone at once".to_owned(),
                },
                Some(ReadFrom::Clone),
            )));
        }
        let Some(items) = clone_items(&tree, folder) else {
            return Ok(Err(unresolved(
                FolderFailure::NoFolder,
                Some(ReadFrom::Clone),
            )));
        };
        if let Some(too_big) = past(
            &items,
            copy::CLONE_FILES,
            copy::CLONE_BYTES,
            ReadFrom::Clone,
        ) {
            return Ok(Err(unresolved(too_big, Some(ReadFrom::Clone))));
        }
        let key = clone_key(renderer, &copy_name, &items);
        return Ok(Ok(Resolved {
            key,
            copy_name,
            items,
            bytes: Arc::new(CloneBytes {
                checkout: PathBuf::from(path),
            }),
            from: ReadFrom::Clone,
            commit: Some(tree.commit.clone()),
            source_id,
        }));
    }

    // The forge.
    let (Some(forge), Some(paths)) = (forge, paths) else {
        let reason = if checkout.is_some() {
            "your clone does not hold this commit yet (knobas fetches it behind the page), and the folder's \
             files were not listed from the source"
        } else {
            "this repository has no clone here, and its files were not listed from the source yet"
        };
        return Ok(Err(unresolved(
            FolderFailure::Copy {
                reason: reason.to_owned(),
            },
            None,
        )));
    };
    let listed = crate::sources::crud::view(
        &forge.pool,
        &forge.secrets,
        &forge.registry,
        &forge.live,
        entity_id,
        Some(paths),
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))?;
    let tree = listed.and_then(|view| {
        view.sections
            .into_iter()
            .flat_map(|s| s.blocks)
            .find_map(|block| match block {
                Block::Tree {
                    entries,
                    truncated,
                    commit,
                    ..
                } => Some((entries, truncated, commit)),
                _ => None,
            })
    });
    let Some((entries, truncated, listed_at)) = tree else {
        return Ok(Err(unresolved(
            FolderFailure::Copy {
                reason: "the source did not list this tree's files".to_owned(),
            },
            Some(ReadFrom::Forge),
        )));
    };
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    let mut items: Vec<Item> = entries
        .iter()
        .filter_map(|entry| {
            let rel = entry.name.strip_prefix(&prefix)?;
            let kind = match entry.kind {
                EntryKind::File => ItemKind::File,
                EntryKind::Symlink => ItemKind::Symlink,
                EntryKind::Dir => ItemKind::Dir,
                EntryKind::Submodule => ItemKind::Submodule,
            };
            Some(Item {
                rel: rel.to_owned(),
                kind,
                size: entry.size.unwrap_or(0),
                executable: false,
                blob: None,
            })
        })
        .collect();
    if items.is_empty() {
        let failure = if truncated {
            FolderFailure::Copy {
                reason: "the source listed only part of this tree, not this folder: clone the repository \
                         to render it"
                    .to_owned(),
            }
        } else {
            FolderFailure::NoFolder
        };
        return Ok(Err(unresolved(failure, Some(ReadFrom::Forge))));
    }
    if truncated {
        return Ok(Err(unresolved(
            FolderFailure::Copy {
                reason: "the source listed only part of this tree, so the folder may not be whole: clone \
                         the repository to render it"
                    .to_owned(),
            },
            Some(ReadFrom::Forge),
        )));
    }
    if let Some(too_big) = past(
        &items,
        copy::FORGE_FILES,
        copy::FORGE_BYTES,
        ReadFrom::Forge,
    ) {
        return Ok(Err(unresolved(too_big, Some(ReadFrom::Forge))));
    }
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    let at = listed_at.or_else(|| commit.clone()).unwrap_or_default();
    let key = forge_key(renderer, &copy_name, &at, folder);
    Ok(Ok(Resolved {
        key,
        copy_name,
        items,
        bytes: Arc::new(ForgeBytes {
            forge: forge.clone(),
            entity_id: entity_id.to_owned(),
            prefix,
            commit: (!at.is_empty()).then_some(at.clone()),
        }),
        from: ReadFrom::Forge,
        commit: (!at.is_empty()).then_some(at),
        source_id,
    }))
}

/// The source the entity is mirrored from.
async fn source_of(pool: &PgPool, entity_id: &str) -> Result<Option<String>, IpcError> {
    sqlx::query_scalar("select source_id from sync.item where entity_id = $1")
        .bind(entity_id)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)
}

/// Every entry under `folder` in `tree`, each by its path from the folder;
/// `None` where `folder` is no folder of it. Walked by the tree's index of
/// each folder's entries, so only the folder's own part is read.
#[must_use]
pub fn clone_items(tree: &local::LocalTree, folder: &str) -> Option<Vec<Item>> {
    let is_dir = folder.is_empty()
        || tree.children.contains_key(folder)
        || tree
            .entries
            .iter()
            .any(|e| e.path == folder && e.kind == EntryKind::Dir);
    if !is_dir {
        return None;
    }
    let cut = if folder.is_empty() {
        0
    } else {
        folder.len() + 1
    };
    let mut items = Vec::new();
    let mut stack = vec![folder.to_owned()];
    while let Some(dir) = stack.pop() {
        for at in tree.children.get(&dir).into_iter().flatten() {
            let entry = &tree.entries[*at];
            let kind = match entry.kind {
                EntryKind::File => ItemKind::File,
                EntryKind::Symlink => ItemKind::Symlink,
                EntryKind::Dir => {
                    stack.push(entry.path.clone());
                    ItemKind::Dir
                }
                EntryKind::Submodule => ItemKind::Submodule,
            };
            items.push(Item {
                rel: entry.path[cut..].to_owned(),
                kind,
                size: entry.size.unwrap_or(0),
                executable: entry.executable,
                blob: entry.blob.clone(),
            });
        }
    }
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Some(items)
}

/// The failure when `items` hold more files or bytes than `files` and
/// `bytes`.
fn past(items: &[Item], files: usize, bytes: u64, from: ReadFrom) -> Option<FolderFailure> {
    let (count, size) = items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::File | ItemKind::Symlink))
        .fold((0_u64, 0_u64), |(n, s), i| (n + 1, s + i.size));
    (count > files as u64 || size > bytes).then_some(FolderFailure::TooBig {
        files: count,
        bytes: size,
        cap_files: files as u64,
        cap_bytes: bytes,
        from,
    })
}

/// A file's bytes from the clone's objects, by its blob.
struct CloneBytes {
    checkout: PathBuf,
}

#[async_trait::async_trait]
impl Bytes for CloneBytes {
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
        let blob = item
            .blob
            .as_deref()
            .ok_or("the clone names no blob for it")?;
        let wanted = usize::try_from(item.size)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        match local::blob(&self.checkout, blob, wanted).await {
            Some((bytes, false)) => Ok(bytes),
            Some((_, true)) => Err("is larger than the clone's tree says".to_owned()),
            None => Err("could not be read from your clone".to_owned()),
        }
    }
}

/// A file's bytes from the forge, at the commit the listing was read at.
struct ForgeBytes {
    forge: Forge,
    entity_id: String,
    prefix: String,
    commit: Option<String>,
}

#[async_trait::async_trait]
impl Bytes for ForgeBytes {
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
        let path = format!("{}{}", self.prefix, item.rel);
        let read = crate::sources::crud::file_bytes(
            &self.forge.pool,
            &self.forge.secrets,
            &self.forge.registry,
            &self.forge.live,
            &self.entity_id,
            &path,
            self.commit.as_deref(),
        )
        .await
        .map_err(|error| crate::sources::to_ipc(&error, None).message)?;
        match read.as_deref() {
            Some(FileBytes::Read(bytes)) => Ok(bytes.clone()),
            Some(FileBytes::TooLarge { size }) => {
                Err(format!("is {size} bytes, more than the source reads"))
            }
            None => Err("the source reads no file bytes".to_owned()),
        }
    }
}

/// The folders of `entity_id`'s tree at `commit` that each folder renderer
/// holding a file glob takes ([`super::marks`]), from the clone's tree, else
/// the forge's listing of every path.
///
/// # Errors
///
/// A query failure.
pub async fn marks(
    pool: &PgPool,
    forge: Option<&Forge>,
    entity_id: &str,
    commit: Option<&str>,
    paths: Option<&str>,
) -> Result<Vec<super::FolderMarks>, IpcError> {
    let renderers = super::list(pool).await?;
    if renderers.iter().all(|r| r.holds.is_empty()) {
        return Ok(super::marks(&renderers, std::iter::empty()));
    }
    let commit = match commit {
        Some(sha) => Some(sha.to_owned()),
        None => match forge {
            Some(forge) => crate::checkout::mirror_tip(pool, forge.registry.as_ref(), entity_id)
                .await?
                .map(|tip| tip.commit),
            None => None,
        },
    };
    let checkout = crate::checkout::resolve(pool, entity_id).await?.path;
    if let (Some(path), Some(sha)) = (&checkout, &commit)
        && let Some(tree) = local::tree(Path::new(path), sha).await
    {
        let files = tree
            .entries
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::File | EntryKind::Symlink))
            .map(|e| e.path.as_str());
        return Ok(super::marks(&renderers, files));
    }
    let (Some(forge), Some(paths)) = (forge, paths) else {
        return Ok(super::marks(&renderers, std::iter::empty()));
    };
    let listed = crate::sources::crud::view(
        &forge.pool,
        &forge.secrets,
        &forge.registry,
        &forge.live,
        entity_id,
        Some(paths),
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))?;
    let entries: Vec<knobas_source::view::TreeEntry> = listed
        .into_iter()
        .flat_map(|view| view.sections)
        .flat_map(|s| s.blocks)
        .filter_map(|block| match block {
            Block::Tree { entries, .. } => Some(entries),
            _ => None,
        })
        .flatten()
        .collect();
    let files = entries
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::File | EntryKind::Symlink))
        .map(|e| e.name.as_str());
    Ok(super::marks(&renderers, files))
}
