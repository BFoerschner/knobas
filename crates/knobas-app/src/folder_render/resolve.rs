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

    // The working tree (ux26, ADR-0049): a copy of what git lists there.
    if commit.as_deref() == Some(crate::checkout::working::END) {
        return working_folder(pool, renderer, entity_id, folder, copy_name, source_id).await;
    }

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
/// The folder `folder` of the working tree at `entity_id`'s checkout (ux26, ADR-0049): every file git lists under it,
/// tracked files that are on disk and untracked files git does not ignore; never an ignored one. A tracked file that
/// does not differ from the index is written from its blob, as a commit's copy is; a changed or untracked one as it is
/// on disk. The key is the clone's over those items, an unchanged file by its blob and a changed or untracked one by
/// `w:` and the SHA-256 of its bytes, so a touch that changes no content changes no key. The tool still gets a copy
/// under the temp directory and never runs in the checkout.
async fn working_folder(
    pool: &PgPool,
    renderer: &FolderRenderer,
    entity_id: &str,
    folder: &str,
    copy_name: String,
    source_id: Option<String>,
) -> Result<Result<Resolved, Unresolved>, IpcError> {
    use crate::checkout::working;
    let from = Some(ReadFrom::WorkingTree);
    let fail = |reason: String, commit: Option<String>| {
        Ok(Err(Unresolved {
            failure: FolderFailure::Copy { reason },
            from,
            commit,
            source_id: source_id.clone(),
        }))
    };
    let repo = crate::checkout::repository_of(pool, entity_id)
        .await
        .unwrap_or_else(|| entity_id.to_owned());
    let Some(path) = crate::checkout::resolve(pool, &repo).await?.path else {
        return fail(
            "this repository is not cloned on this computer, so it has no working tree here"
                .to_owned(),
            None,
        );
    };
    let checkout = PathBuf::from(&path);
    let (status, index) = match working::listing(&checkout).await {
        Ok(listed) => listed,
        Err(refused) => return fail(refused.to_ipc().message, None),
    };
    let head = working::head(&checkout).await;
    let at = |rel: &str| -> Option<String> {
        if folder.is_empty() {
            Some(rel.to_owned())
        } else {
            rel.strip_prefix(folder)?
                .strip_prefix('/')
                .map(str::to_owned)
        }
    };
    let root = checkout.clone();
    let tracked: Vec<(String, String, String)> = index
        .iter()
        .filter_map(|(rel, (mode, blob))| {
            at(rel).map(|inner| (rel.clone(), inner, format!("{mode} {blob}")))
        })
        .collect();
    let untracked: Vec<(String, String)> = status
        .untracked
        .iter()
        .filter(|rel| !rel.ends_with('/'))
        .filter_map(|rel| at(rel).map(|inner| (rel.clone(), inner)))
        .collect();
    let changed = status.changed.clone();
    let listed =
        tokio::task::spawn_blocking(move || working_items(&root, &tracked, &untracked, &changed))
            .await
            .map_err(IpcError::internal)?;
    if listed.is_empty() {
        return Ok(Err(Unresolved {
            failure: FolderFailure::NoFolder,
            from,
            commit: head,
            source_id,
        }));
    }
    if let Some(too_big) = past(
        &listed,
        copy::CLONE_FILES,
        copy::CLONE_BYTES,
        ReadFrom::WorkingTree,
    ) {
        return Ok(Err(Unresolved {
            failure: too_big,
            from,
            commit: head,
            source_id,
        }));
    }
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    let key = clone_key(renderer, &copy_name, &listed);
    Ok(Ok(Resolved {
        key,
        copy_name,
        items: listed,
        bytes: Arc::new(WorkingBytes { checkout, prefix }),
        from: ReadFrom::WorkingTree,
        commit: head,
        source_id,
    }))
}

/// The working folder's items, off the runtime: `tracked` as (path, path in the folder, `"<mode> <blob>"`), `untracked`
/// as (path, path in the folder). A tracked file gone from disk is left out; the folders on the way are items too.
fn working_items(
    root: &Path,
    tracked: &[(String, String, String)],
    untracked: &[(String, String)],
    changed: &std::collections::HashSet<String>,
) -> Vec<Item> {
    use sha2::Digest as _;
    let mut items: Vec<Item> = Vec::new();
    let mut dirs = std::collections::BTreeSet::new();
    let disk = |rel: &str| std::fs::symlink_metadata(root.join(rel)).ok();
    let digest = |rel: &str, link: bool| -> Option<String> {
        let full = root.join(rel);
        let bytes = if link {
            std::fs::read_link(&full)
                .ok()?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else {
            std::fs::read(&full).ok()?
        };
        Some(format!("w:{}", super::hex(&sha2::Sha256::digest(&bytes))))
    };
    let mut add = |inner: &str, item: Item| {
        let mut at = inner;
        while let Some(cut) = at.rfind('/') {
            at = &at[..cut];
            dirs.insert(at.to_owned());
        }
        items.push(item);
    };
    for (rel, inner, mode_blob) in tracked {
        let Some(meta) = disk(rel) else { continue };
        let (mode, blob) = mode_blob.split_once(' ').unwrap_or(("100644", ""));
        let kind = match mode {
            "120000" => ItemKind::Symlink,
            "160000" => ItemKind::Submodule,
            _ => ItemKind::File,
        };
        let link = kind == ItemKind::Symlink;
        let blob = if kind == ItemKind::Submodule {
            Some(blob.to_owned())
        } else if changed.contains(rel) {
            digest(rel, link)
        } else {
            Some(blob.to_owned())
        };
        let Some(blob) = blob else { continue };
        add(
            inner,
            Item {
                rel: inner.clone(),
                kind,
                size: if kind == ItemKind::File {
                    meta.len()
                } else {
                    0
                },
                executable: mode == "100755",
                blob: Some(blob),
            },
        );
    }
    for (rel, inner) in untracked {
        let Some(meta) = disk(rel) else { continue };
        let link = meta.file_type().is_symlink();
        if !link && !meta.is_file() {
            continue;
        }
        let Some(blob) = digest(rel, link) else {
            continue;
        };
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            !link && meta.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        add(
            inner,
            Item {
                rel: inner.clone(),
                kind: if link {
                    ItemKind::Symlink
                } else {
                    ItemKind::File
                },
                size: if link { 0 } else { meta.len() },
                executable,
                blob: Some(blob),
            },
        );
    }
    for dir in dirs {
        items.push(Item {
            rel: dir,
            kind: ItemKind::Dir,
            size: 0,
            executable: false,
            blob: None,
        });
    }
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    items.dedup_by(|a, b| a.rel == b.rel);
    items
}

/// A working folder's bytes: an unchanged file's blob from git's objects, a changed or untracked one (`w:` key) from
/// disk, never through a link (a link answers its target).
struct WorkingBytes {
    checkout: PathBuf,
    prefix: String,
}

#[async_trait::async_trait]
impl Bytes for WorkingBytes {
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
        let blob = item.blob.as_deref().ok_or("git names no blob for it")?;
        let rel = format!("{}{}", self.prefix, item.rel);
        if blob.starts_with("w:") {
            let cap = item.size.max(1).saturating_mul(2).max(copy::CLONE_BYTES);
            return match crate::checkout::working::file_bytes(&self.checkout, &rel, cap).await {
                Ok(crate::checkout::working::Bytes::Read(bytes)) => Ok(bytes),
                Ok(crate::checkout::working::Bytes::TooLarge(size)) => {
                    Err(format!("is {size} bytes, past what knobas copies"))
                }
                Ok(crate::checkout::working::Bytes::NotThere) => {
                    Err("is no longer in your working tree".to_owned())
                }
                Err(refused) => Err(refused.to_ipc().message),
            };
        }
        let wanted = usize::try_from(item.size)
            .unwrap_or(usize::MAX)
            .saturating_mul(2)
            .saturating_add(1);
        match local::blob(&self.checkout, blob, wanted).await {
            Some((bytes, false)) => Ok(bytes),
            Some((_, true)) => Err("is larger than its blob says".to_owned()),
            None => Err("could not be read from your clone".to_owned()),
        }
    }
}

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
    // The working tree (ux26): the files git lists there, tracked and untracked, never an ignored one.
    if commit == Some(crate::checkout::working::END) {
        let repo = crate::checkout::repository_of(pool, entity_id)
            .await
            .unwrap_or_else(|| entity_id.to_owned());
        let Some(path) = crate::checkout::resolve(pool, &repo).await?.path else {
            return Ok(super::marks(&renderers, std::iter::empty()));
        };
        let Ok((status, index)) = crate::checkout::working::listing(Path::new(&path)).await else {
            return Ok(super::marks(&renderers, std::iter::empty()));
        };
        let files = index
            .keys()
            .map(String::as_str)
            .chain(status.untracked.iter().map(String::as_str));
        return Ok(super::marks(&renderers, files));
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=knobas test",
                "-c",
                "user.email=test@knobas.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    async fn items_of(dir: &Path, folder: &str) -> Vec<Item> {
        let (status, index) = crate::checkout::working::listing(dir).await.unwrap();
        let at = |rel: &str| -> Option<String> {
            rel.strip_prefix(folder)?
                .strip_prefix('/')
                .map(str::to_owned)
        };
        let tracked: Vec<(String, String, String)> = index
            .iter()
            .filter_map(|(rel, (mode, blob))| {
                at(rel).map(|inner| (rel.clone(), inner, format!("{mode} {blob}")))
            })
            .collect();
        let untracked: Vec<(String, String)> = status
            .untracked
            .iter()
            .filter_map(|rel| at(rel).map(|inner| (rel.clone(), inner)))
            .collect();
        working_items(dir, &tracked, &untracked, &status.changed)
    }

    #[tokio::test]
    async fn the_working_folder_holds_what_git_lists_unchanged_files_by_their_blob() {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path();
        git(at, &["init", "-q"]);
        write(at, ".gitignore", "flows/node_modules/\n");
        write(at, "flows/a.json", "{}\n");
        write(at, "flows/b.json", "{\"b\":1}\n");
        write(at, "flows/gone.json", "{}\n");
        write(at, "other/x.txt", "x\n");
        git(at, &["add", "."]);
        git(at, &["commit", "-q", "-m", "first"]);
        let blob_a = git(at, &["rev-parse", "HEAD:flows/a.json"]);
        write(at, "flows/b.json", "{\"b\":2}\n");
        std::fs::remove_file(at.join("flows/gone.json")).unwrap();
        write(at, "flows/new/c.json", "{}\n");
        write(at, "flows/node_modules/dep.js", "x\n");
        let items = items_of(at, "flows").await;
        let rels: Vec<(&str, ItemKind)> = items.iter().map(|i| (i.rel.as_str(), i.kind)).collect();
        assert_eq!(
            rels,
            vec![
                ("a.json", ItemKind::File),
                ("b.json", ItemKind::File),
                ("new", ItemKind::Dir),
                ("new/c.json", ItemKind::File)
            ]
        );
        let blob = |rel: &str| {
            items
                .iter()
                .find(|i| i.rel == rel)
                .unwrap()
                .blob
                .clone()
                .unwrap()
        };
        assert_eq!(blob("a.json"), blob_a, "an unchanged file is its blob");
        assert!(
            blob("b.json").starts_with("w:"),
            "a changed file is its bytes on disk"
        );
        assert!(
            blob("new/c.json").starts_with("w:"),
            "an untracked file is its bytes on disk"
        );
        // A touch that changes no content changes no item.
        let before = items.clone();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(at, "flows/a.json", "{}\n");
        write(at, "flows/b.json", "{\"b\":2}\n");
        assert_eq!(items_of(at, "flows").await, before);
    }
}
