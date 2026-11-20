//! The files pass (the everything-search decisions, 2026-11-20, D7 and D16)
//! run against a profile's database with files from a folder: how a copy of
//! the saved scale home gets files, since its forge is not there.
//!
//! ```sh
//! cargo run --release -p knobas-app --example files_fill -- \
//!   --db postgresql://postgres:<pass>@127.0.0.1:<port>/knobas --index <profile>/files-1 \
//!   --from <folder> [--source <id>] [--kept-mb 2048]
//! ```
//!
//! Each immediate subfolder of `<folder>` becomes the files of one of the
//! source's mirrored repositories, in entity id order, the subfolders in name
//! order: its default branch's stored tree (`sync.git_tree`,
//! `sync.branch_tree`) is written at a commit of its own, and the pass then
//! reads it through its own write path, the bytes answered from the folder.
//! `--source` is the first source holding repositories where it is not
//! given. One JSON line at the end. Run it on a copy: it rewrites the
//! repositories' default branches' trees.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use knobas_source::{Source, SourceDescriptor, SourceError};
use knobas_sync::code_files::{self, Bounds, Files};

/// A forge that answers from the folder.
struct Folder {
    inner: knobas_source_mock::MockSource,
    /// Each repository's folder.
    roots: BTreeMap<String, PathBuf>,
}

#[async_trait::async_trait]
impl Source for Folder {
    async fn view(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.inner.descriptor()
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.inner.test_connection().await
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        self.inner.sync(cursor, sink).await
    }
    async fn write(
        &self,
        op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        self.inner.write(op).await
    }
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
    fn tree_shape(&self) -> Option<knobas_source::tree::TreeShape> {
        Some(knobas_source::tree::TreeShape {
            repository: "repo",
            default_branch: &["default_branch"],
            branch: "branch",
            branch_name: &["name"],
            branch_commit: &["commit", "id"],
            pull: None,
        })
    }
    async fn read_blob(
        &self,
        repository: &str,
        _: &str,
        path: &str,
        cap: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        let Some(root) = self.roots.get(repository) else {
            return Ok(None);
        };
        let bytes = std::fs::read(root.join(path)).ok();
        Ok(bytes.filter(|b| b.len() as u64 <= cap))
    }
    async fn read_archive(
        &self,
        repository: &str,
        _: &str,
        wanted: &(dyn for<'p> Fn(&'p str) -> bool + Sync),
        each: &mut (dyn FnMut(String, Vec<u8>) + Send),
    ) -> Result<Option<u32>, SourceError> {
        let Some(root) = self.roots.get(repository) else {
            return Ok(Some(1));
        };
        for (path, _) in walk(root) {
            if wanted(&path)
                && let Ok(bytes) = std::fs::read(root.join(&path))
            {
                each(path, bytes);
            }
        }
        Ok(Some(1))
    }
}

/// Every regular file under `root`, by its path from it, with its size.
fn walk(root: &Path) -> Vec<(String, u64)> {
    fn go(root: &Path, at: &Path, out: &mut Vec<(String, u64)>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if entry.file_name() != ".git" {
                    go(root, &path, out);
                }
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, entry.metadata().map_or(0, |m| m.len())));
            }
        }
    }
    let mut out = Vec::new();
    go(root, root, &mut out);
    out.sort();
    out
}

/// A folder's entries: name, mode, size, sha.
type Level = Vec<(String, i32, i64, Vec<u8>)>;

/// A sha of 20 bytes for `bytes`: what the folder's tree is keyed by.
fn sha_of(bytes: &[u8]) -> Vec<u8> {
    use std::hash::{Hash, Hasher};
    let mut out = Vec::with_capacity(24);
    for seed in 0..3_u8 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        bytes.hash(&mut hasher);
        out.extend_from_slice(&hasher.finish().to_be_bytes());
    }
    out.truncate(20);
    out
}

/// Write `root`'s files as the stored tree of `repository`'s default branch
/// at `commit`, answering its root's sha.
async fn store_tree(
    pool: &sqlx::PgPool,
    source: &str,
    repository: &str,
    commit: &str,
    root: &Path,
) -> Result<(), sqlx::Error> {
    let files = walk(root);
    let mut folders: BTreeMap<String, Level> = BTreeMap::new();
    folders.entry(String::new()).or_default();
    for (path, size) in &files {
        let parts: Vec<&str> = path.split('/').collect();
        let mut at = String::new();
        for part in &parts[..parts.len() - 1] {
            let child = if at.is_empty() {
                (*part).to_owned()
            } else {
                format!("{at}/{part}")
            };
            folders.entry(child.clone()).or_default();
            at = child;
        }
        let content = std::fs::read(root.join(path)).unwrap_or_default();
        folders.get_mut(&at).expect("its folder").push((
            (*parts.last().expect("a name")).to_owned(),
            0o100_644,
            i64::try_from(*size).unwrap_or(i64::MAX),
            sha_of(&content),
        ));
    }
    let mut order: Vec<String> = folders.keys().cloned().collect();
    order.sort_by_key(|folder| {
        std::cmp::Reverse(if folder.is_empty() {
            0
        } else {
            1 + folder.matches('/').count()
        })
    });
    let mut shas: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for folder in order {
        let mut entries = folders[&folder].clone();
        for (child, sha) in &shas {
            let parent = child.rsplit_once('/').map_or("", |(parent, _)| parent);
            if !child.is_empty() && parent == folder {
                entries.push((
                    child.rsplit('/').next().unwrap_or(child).to_owned(),
                    0o040_000,
                    0,
                    sha.clone(),
                ));
            }
        }
        entries.sort();
        let sha = sha_of(format!("{repository}:{commit}:{folder}:{entries:?}").as_bytes());
        sqlx::query(
            "insert into sync.git_tree (source_id, sha, names, modes, sizes, shas, total)
             values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
        )
        .bind(source)
        .bind(&sha)
        .bind(entries.iter().map(|e| e.0.clone()).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.1).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.2).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.3.clone()).collect::<Vec<_>>())
        .bind(i64::try_from(entries.len()).unwrap_or(0))
        .execute(pool)
        .await?;
        shas.insert(folder, sha);
    }
    // The repository's default branch: its stored row moved to the folder's
    // tree, or one made beside a branch item of its own.
    let branch: String = sqlx::query_scalar("select coalesce(payload #>> '{default_branch}', 'main') from sync.item where entity_id = $1")
        .bind(repository)
        .fetch_one(pool)
        .await?;
    let entity = format!("{repository}@files-fill");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'branch', $2) on conflict do nothing")
        .bind(&entity)
        .bind(&branch)
        .execute(pool)
        .await?;
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at, synced_at)
         values ($1, $2, 'branch', $3, '', '{}', now(), now()) on conflict do nothing",
    )
    .bind(&entity)
    .bind(source)
    .bind(&branch)
    .execute(pool)
    .await?;
    sqlx::query(
        "delete from sync.branch_tree where repository_id = $1 and branch = $2 and entity_id <> $3",
    )
    .bind(repository)
    .bind(&branch)
    .bind(&entity)
    .execute(pool)
    .await?;
    sqlx::query(
        "insert into sync.branch_tree (entity_id, source_id, repository_id, branch, commit_sha, root, entries)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (entity_id) do update set commit_sha = excluded.commit_sha, root = excluded.root, entries = excluded.entries",
    )
    .bind(&entity)
    .bind(source)
    .bind(repository)
    .bind(&branch)
    .bind(commit)
    .bind(&shas[""])
    .bind(i64::try_from(files.len()).unwrap_or(0))
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (mut db, mut index, mut from, mut source, mut kept_mb) =
        (None, None, None, None, 2_048_u64);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next(),
            "--index" => index = args.next(),
            "--from" => from = args.next(),
            "--source" => source = args.next(),
            "--kept-mb" => kept_mb = args.next().and_then(|n| n.parse().ok()).unwrap_or(kept_mb),
            _ => {
                eprintln!(
                    "usage: files_fill --db <url> --index <dir> --from <folder> [--source <id>] [--kept-mb <n>]"
                );
                std::process::exit(2);
            }
        }
    }
    let (Some(db), Some(index), Some(from)) = (db, index, from) else {
        eprintln!("--db, --index and --from are required");
        std::process::exit(2);
    };
    let pool = sqlx::PgPool::connect(&db).await.expect("the database");
    let source: String = match source {
        Some(source) => source,
        None => sqlx::query_scalar("select source_id from sync.item where kind = 'repo' group by source_id order by count(*) desc limit 1")
            .fetch_one(&pool)
            .await
            .expect("a source holding repositories"),
    };
    let repositories: Vec<String> = sqlx::query_scalar(
        "select i.entity_id from sync.item i join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
          where i.source_id = $1 and i.kind = 'repo' order by i.entity_id",
    )
    .bind(&source)
    .fetch_all(&pool)
    .await
    .expect("the repositories");
    let mut folders: Vec<PathBuf> = std::fs::read_dir(&from)
        .expect("the folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    let started = Instant::now();
    let mut roots = BTreeMap::new();
    for (repository, folder) in repositories.iter().zip(&folders) {
        store_tree(&pool, &source, repository, "files-fill", folder)
            .await
            .expect("the tree");
        roots.insert(repository.clone(), folder.clone());
    }
    let trees_ms = started.elapsed().as_millis();
    let index =
        tokio::task::spawn_blocking(move || knobas_files::FileIndex::open(Path::new(&index)))
            .await
            .expect("the open")
            .expect("the index");
    code_files::reconcile(&pool, &index)
        .await
        .expect("the comparison");
    let files = Files {
        index,
        clones: None,
    };
    let forge = Folder {
        inner: knobas_source_mock::MockSource::new(),
        roots,
    };
    let bounds = Bounds {
        budget: std::time::Duration::from_secs(3_600),
        archives: u32::MAX,
        reads: u32::MAX,
    };
    let filled = Instant::now();
    let passed = code_files::refresh_within(
        &pool,
        &source,
        &forge,
        &files,
        bounds,
        kept_mb << 20,
        &|_| {},
    )
    .await
    .expect("the pass");
    println!(
        "{}",
        serde_json::json!({
            "source": source,
            "repositories": roots_len(&repositories, &folders),
            "trees_ms": trees_ms,
            "pass_ms": filled.elapsed().as_millis(),
            "passed": passed,
        })
    );
}

fn roots_len(repositories: &[String], folders: &[PathBuf]) -> usize {
    repositories.len().min(folders.len())
}
