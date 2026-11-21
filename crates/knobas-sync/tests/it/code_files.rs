//! The files pass (the everything-search decisions, 2026-11-20, D3 and D7),
//! over a forge of files in memory and a temporary index: the stored tree is
//! written straight into `sync.git_tree` and `sync.branch_tree`, as the tree
//! pass would leave it.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_files::{FileIndex, Reads};
use knobas_source::{Source, SourceDescriptor, SourceError};
use knobas_source_mock::MockSource;
use knobas_sync::code_files::{self, Bounds, Clones, Files, Passed};
use sqlx::PgPool;

const SOURCE: &str = "forge";

/// A forge: each repository's files at its head, by path.
#[derive(Default)]
struct Forge {
    inner: MockSource,
    files: Mutex<HashMap<String, BTreeMap<String, Vec<u8>>>>,
    archive: bool,
    archives: AtomicU32,
    reads: AtomicU32,
    /// Every read past this many fails as an unreachable forge would.
    fail_after: Option<u32>,
}

#[async_trait::async_trait]
impl Source for Forge {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: SOURCE.to_owned(),
            ..self.inner.descriptor()
        }
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
            branch_commit: &["commit"],
            pull: None,
        })
    }
    async fn read_blob(
        &self,
        repository: &str,
        _commit: &str,
        path: &str,
        _cap: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        let n = self.reads.fetch_add(1, Ordering::SeqCst);
        if self.fail_after.is_some_and(|after| n >= after) {
            return Err(SourceError::Unreachable("the forge is down".into()));
        }
        Ok(self
            .files
            .lock()
            .unwrap()
            .get(repository)
            .and_then(|files| files.get(path).cloned()))
    }
    async fn read_archive(
        &self,
        repository: &str,
        _commit: &str,
        wanted: &(dyn for<'p> Fn(&'p str) -> bool + Sync),
        each: &mut (dyn FnMut(String, Vec<u8>) + Send),
    ) -> Result<Option<u32>, SourceError> {
        if !self.archive {
            return Ok(None);
        }
        self.archives.fetch_add(1, Ordering::SeqCst);
        let files = self
            .files
            .lock()
            .unwrap()
            .get(repository)
            .cloned()
            .unwrap_or_default();
        for (path, bytes) in files {
            if wanted(&path) {
                each(path, bytes);
            }
        }
        Ok(Some(1))
    }
}

/// Checkouts on this machine: blobs by their sha.
#[derive(Default)]
struct Checkouts {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    asked: AtomicU32,
}

impl Clones for Checkouts {
    fn blob<'a>(
        &'a self,
        _repository: &'a str,
        sha: &'a str,
        _max: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<u8>>> + Send + 'a>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let found = self.blobs.lock().unwrap().get(sha).cloned();
        Box::pin(async move { found })
    }
    fn holds<'a>(
        &'a self,
        _repository: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
}

/// A folder's entries: name, mode, size, sha.
type Level = Vec<(String, i32, i64, Vec<u8>)>;

/// A blob's sha: 20 bytes from its content.
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn pool(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'gitea', 'Forge', 'http://x', 'Pat') on conflict do nothing",
    )
    .bind(SOURCE)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

struct World {
    pool: PgPool,
    forge: Forge,
    files: Files,
    _dir: tempfile::TempDir,
}

async fn world(label: &str, forge: Forge) -> World {
    let dir = tempfile::tempdir().unwrap();
    let index = FileIndex::open(&dir.path().join("files-1")).unwrap();
    World {
        pool: pool(label).await,
        forge,
        files: Files {
            index,
            clones: None,
        },
        _dir: dir,
    }
}

/// Repository `name` at `commit` holding `files`: its mirror row, its
/// default branch's stored tree, and the forge's bytes.
async fn push(world: &World, name: &str, commit: &str, files: &[(&str, &[u8])]) -> String {
    push_to(world, SOURCE, name, commit, files).await
}

/// [`push`] under another source's id.
async fn push_to(
    world: &World,
    source: &str,
    name: &str,
    commit: &str,
    files: &[(&str, &[u8])],
) -> String {
    let id = format!("{source}:{name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'repo', $2) on conflict do nothing")
        .bind(&id)
        .bind(name)
        .execute(&world.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at, synced_at)
         values ($1, $2, 'repo', $3, '', '{\"default_branch\": \"main\"}', now(), now())
         on conflict (entity_id) do update set item_updated_at = now()",
    )
    .bind(&id)
    .bind(source)
    .bind(name)
    .execute(&world.pool)
    .await
    .unwrap();
    // The tree: a level a folder, bottom up.
    let mut folders: BTreeMap<String, Level> = BTreeMap::new();
    folders.entry(String::new()).or_default();
    for (path, bytes) in files {
        let mut at = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            let child = if at.is_empty() {
                (*part).to_owned()
            } else {
                format!("{at}/{part}")
            };
            folders.entry(child.clone()).or_default();
            at = child;
        }
        let size = i64::try_from(bytes.len()).unwrap();
        folders.get_mut(&at).unwrap().push((
            (*parts.last().unwrap()).to_owned(),
            0o100_644,
            size,
            sha_of(bytes),
        ));
    }
    let mut shas: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut keys: Vec<String> = folders.keys().cloned().collect();
    keys.sort_by_key(|key| {
        std::cmp::Reverse(key.matches('/').count() + usize::from(!key.is_empty()))
    });
    for folder in keys {
        let mut entries = folders[&folder].clone();
        for (child, sha) in &shas {
            let parent = child.rsplit_once('/').map_or("", |(parent, _)| parent);
            let is_child = if folder.is_empty() {
                !child.contains('/') && !child.is_empty()
            } else {
                parent == folder
            };
            if is_child {
                let name = child.rsplit('/').next().unwrap().to_owned();
                entries.push((name, 0o040_000, 0, sha.clone()));
            }
        }
        entries.sort();
        let level_sha = sha_of(format!("{name}:{folder}:{entries:?}").as_bytes());
        sqlx::query(
            "insert into sync.git_tree (source_id, sha, names, modes, sizes, shas, total)
             values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
        )
        .bind(source)
        .bind(&level_sha)
        .bind(entries.iter().map(|e| e.0.clone()).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.1).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.2).collect::<Vec<_>>())
        .bind(entries.iter().map(|e| e.3.clone()).collect::<Vec<_>>())
        .bind(i64::try_from(entries.len()).unwrap())
        .execute(&world.pool)
        .await
        .unwrap();
        shas.insert(folder, level_sha);
    }
    let branch = format!("{id}@main");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'branch', 'main') on conflict do nothing")
        .bind(&branch)
        .execute(&world.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at, synced_at)
         values ($1, $2, 'branch', 'main', '', '{}', now(), now()) on conflict do nothing",
    )
    .bind(&branch)
    .bind(source)
    .execute(&world.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.branch_tree (entity_id, source_id, repository_id, branch, commit_sha, root, entries)
         values ($1, $2, $3, 'main', $4, $5, $6)
         on conflict (entity_id) do update set commit_sha = excluded.commit_sha, root = excluded.root",
    )
    .bind(&branch)
    .bind(source)
    .bind(&id)
    .bind(commit)
    .bind(&shas[""])
    .bind(i64::try_from(files.len()).unwrap())
    .execute(&world.pool)
    .await
    .unwrap();
    world.forge.files.lock().unwrap().insert(
        id.clone(),
        files
            .iter()
            .map(|(p, b)| ((*p).to_owned(), b.to_vec()))
            .collect(),
    );
    id
}

async fn pass(world: &World) -> Passed {
    pass_within(world, Bounds::default(), u64::MAX).await
}

async fn pass_within(world: &World, bounds: Bounds, bound: u64) -> Passed {
    code_files::refresh_within(
        &world.pool,
        SOURCE,
        &world.forge,
        &world.files,
        bounds,
        bound,
        &|_| {},
    )
    .await
    .unwrap()
}

fn found(world: &World, typed: &str) -> Vec<String> {
    let repos = world.files.index.repos();
    world
        .files
        .index
        .ask(typed, Reads::BOTH, 30)
        .into_iter()
        .map(|hit| repos[&hit.no].entity_id.clone())
        .collect()
}

async fn row(world: &World, id: &str) -> (Option<String>, i32, i32) {
    sqlx::query_as(
        "select commit_sha, files, text_files from sync.code_repo where repository_id = $1",
    )
    .bind(id)
    .fetch_one(&world.pool)
    .await
    .unwrap()
}

const SERVER: &[u8] =
    b"package com.tidewater.settlebridge;\npublic final class SettleBridgeServer {}\n";

#[tokio::test]
async fn a_first_pass_reads_every_file_and_a_second_writes_nothing() {
    let world = world("code_files_first", Forge::default()).await;
    let id = push(
        &world,
        "tidewater/ledger-gateway",
        "c1",
        &[
            ("src/settlebridge/SettleBridgeServer.java", SERVER),
            ("README.md", b"The ledger gateway."),
            ("Cargo.lock", b"lock"),
            ("bin/tool.exe", b"MZ\0\0"),
        ],
    )
    .await;
    let first = pass(&world).await;
    // The lock file is left out by its name; the binary is read, then left out.
    assert_eq!(
        (first.repositories, first.files, first.reads),
        (1, 4, 3),
        "{first:?}"
    );
    assert_eq!(found(&world, "settlebridge"), std::slice::from_ref(&id));
    assert_eq!(row(&world, &id).await, (Some("c1".to_owned()), 4, 2));
    let left: serde_json::Value = sqlx::query_scalar("select left_out from sync.code_repo")
        .fetch_one(&world.pool)
        .await
        .unwrap();
    assert_eq!(left, serde_json::json!({ "lock_file": 1, "binary": 1 }));
    let second = pass(&world).await;
    assert_eq!(second, Passed::default());
}

#[tokio::test]
async fn a_push_reads_only_what_changed_and_a_moved_or_deleted_file_follows() {
    let world = world("code_files_push", Forge::default()).await;
    let a: &[u8] = b"class Alpha {}";
    let b: &[u8] = b"class Beta {}";
    let c: &[u8] = b"class Gamma {}";
    let id = push(
        &world,
        "acme/r",
        "c1",
        &[("a/Alpha.java", a), ("b/Beta.java", b), ("c/Gamma.java", c)],
    )
    .await;
    pass(&world).await;
    let reads = world.forge.reads.load(Ordering::SeqCst);
    // Beta changed, Gamma moved, Alpha deleted.
    push(
        &world,
        "acme/r",
        "c2",
        &[
            ("b/Beta.java", b"class Beta { int retryqueue; }"),
            ("moved/Gamma.java", c),
        ],
    )
    .await;
    let second = pass(&world).await;
    assert_eq!((second.files, second.removed), (2, 2), "{second:?}");
    assert_eq!(
        world.forge.reads.load(Ordering::SeqCst) - reads,
        2,
        "the changed and the moved file"
    );
    assert_eq!(found(&world, "retryqueue"), std::slice::from_ref(&id));
    assert!(found(&world, "alpha").is_empty());
    assert_eq!(
        world.files.index.in_repo(1, "gamma", Reads::BOTH, 8).0[0].path,
        "moved/Gamma.java"
    );
    assert_eq!(row(&world, &id).await, (Some("c2".to_owned()), 2, 2));
}

#[tokio::test]
async fn more_than_3_wanted_files_are_one_archive_where_the_archive_is_mostly_them() {
    let forge = Forge {
        archive: true,
        ..Forge::default()
    };
    let world = world("code_files_archive", forge).await;
    let many: Vec<(String, Vec<u8>)> = (0..25)
        .map(|n| {
            (
                format!("src/F{n}.java"),
                format!("class F{n} {{}}").into_bytes(),
            )
        })
        .collect();
    let files: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    push(&world, "acme/many", "c1", &files).await;
    let passed = pass(&world).await;
    assert_eq!((passed.archives, passed.reads), (1, 0), "{passed:?}");
    // Four small files: one archive, not four reads (Fable's ruling of
    // 2026-11-21: the first fill of a scale home's small repositories).
    push(&world, "acme/four", "c1", &files[..4]).await;
    let passed = pass(&world).await;
    assert_eq!((passed.archives, passed.reads), (1, 0), "{passed:?}");
    // Three: a file at a time.
    push(&world, "acme/few", "c1", &files[..3]).await;
    let passed = pass(&world).await;
    assert_eq!((passed.archives, passed.reads), (0, 3), "{passed:?}");
    // Ten beside a 6 MB file the rules leave out: the archive would carry
    // more than ten single reads could, so a file at a time.
    let large = vec![b'x'; 6 << 20];
    let mut beside: Vec<(&str, &[u8])> = files[..10].to_vec();
    beside.push(("assets/video.bin", large.as_slice()));
    push(&world, "acme/beside", "c1", &beside).await;
    let passed = pass(&world).await;
    assert_eq!((passed.archives, passed.reads), (0, 10), "{passed:?}");
}

#[test]
fn an_archive_is_at_most_what_its_single_reads_could_be_and_a_gib() {
    use knobas_core::code_text::SIZE_CAP;
    assert!(!code_files::by_archive(3, 0));
    assert!(code_files::by_archive(4, 4 * SIZE_CAP));
    assert!(!code_files::by_archive(4, 4 * SIZE_CAP + 1));
    assert!(code_files::by_archive(5_000, 1 << 30));
    assert!(!code_files::by_archive(5_000, (1 << 30) + 1), "past a GiB");
}

#[tokio::test]
async fn a_checkout_that_holds_every_blob_spares_the_forge() {
    let mut world = world(
        "code_files_clones",
        Forge {
            archive: true,
            ..Forge::default()
        },
    )
    .await;
    let checkouts = Arc::new(Checkouts::default());
    let many: Vec<(String, Vec<u8>)> = (0..30)
        .map(|n| {
            (
                format!("F{n}.java"),
                format!("class F{n} {{}}").into_bytes(),
            )
        })
        .collect();
    for (_, bytes) in &many {
        checkouts
            .blobs
            .lock()
            .unwrap()
            .insert(hex(&sha_of(bytes)), bytes.clone());
    }
    world.files.clones = Some(checkouts.clone());
    let files: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    push(&world, "acme/cloned", "c1", &files).await;
    let passed = pass(&world).await;
    assert_eq!(
        (passed.from_clones, passed.archives, passed.reads),
        (30, 0, 0),
        "{passed:?}"
    );
    assert_eq!(
        world.forge.archives.load(Ordering::SeqCst) + world.forge.reads.load(Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn the_bounds_cut_a_repository_and_the_next_pass_finishes_it() {
    let world = world("code_files_bounds", Forge::default()).await;
    let many: Vec<(String, Vec<u8>)> = (0..10)
        .map(|n| {
            (
                format!("F{n}.java"),
                format!("class F{n} {{}}").into_bytes(),
            )
        })
        .collect();
    let files: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let id = push(&world, "acme/cut", "c1", &files).await;
    let bounds = Bounds {
        budget: Duration::from_secs(60),
        archives: 0,
        reads: 4,
    };
    let cut = pass_within(&world, bounds, u64::MAX).await;
    assert_eq!(
        (cut.repositories, cut.owed, cut.reads),
        (0, 1, 4),
        "{cut:?}"
    );
    // What it wrote is kept, its row at no commit.
    assert_eq!(row(&world, &id).await, (None, 4, 4));
    let rest = pass_within(
        &world,
        Bounds {
            reads: 100,
            ..bounds
        },
        u64::MAX,
    )
    .await;
    assert_eq!((rest.repositories, rest.reads), (1, 6), "{rest:?}");
    assert_eq!(row(&world, &id).await, (Some("c1".to_owned()), 10, 10));
}

#[tokio::test]
async fn the_bound_on_kept_text_leaves_a_repository_its_names_only() {
    let world = world("code_files_bound", Forge::default()).await;
    let big = "word ".repeat(100);
    let id = push(
        &world,
        "acme/big",
        "c1",
        &[("settlebridge/Big.java", big.as_bytes())],
    )
    .await;
    let passed = pass_within(&world, Bounds::default(), 10).await;
    assert_eq!(passed.reads, 0, "nothing is read for text not kept");
    assert_eq!(row(&world, &id).await, (Some("c1".to_owned()), 1, 0));
    assert_eq!(
        found(&world, "settlebridge"),
        std::slice::from_ref(&id),
        "found by its name"
    );
    assert!(found(&world, "word").is_empty());
    let left: serde_json::Value = sqlx::query_scalar("select left_out from sync.code_repo")
        .fetch_one(&world.pool)
        .await
        .unwrap();
    assert_eq!(left, serde_json::json!({ "bound": 1 }));
    // Kept text over a lowered bound: rewritten with names only.
    let world = World {
        files: Files {
            index: world.files.index.clone(),
            clones: None,
        },
        ..world
    };
    let other = push(
        &world,
        "acme/other",
        "c1",
        &[("Other.java", b"class Retryqueue {}")],
    )
    .await;
    pass(&world).await;
    assert_eq!(found(&world, "retryqueue"), std::slice::from_ref(&other));
    pass_within(&world, Bounds::default(), 5).await;
    assert!(found(&world, "retryqueue").is_empty(), "its text dropped");
    assert_eq!(row(&world, &other).await, (Some("c1".to_owned()), 1, 0));
}

#[tokio::test]
async fn a_withdrawn_repository_and_a_removed_source_are_found_no_more() {
    let world = world("code_files_gone", Forge::default()).await;
    let id = push(
        &world,
        "acme/gone",
        "c1",
        &[("Gone.java", b"class Settlebridge {}")],
    )
    .await;
    pass(&world).await;
    assert_eq!(found(&world, "settlebridge"), std::slice::from_ref(&id));
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(&world.pool)
        .await
        .unwrap();
    let passed = pass(&world).await;
    assert_eq!(passed.gone, 1);
    assert!(world.files.index.held().unwrap().is_empty());
    // A source removed: its repositories' rows go by cascade, and their files.
    let other = push(
        &world,
        "acme/kept",
        "c1",
        &[("Kept.java", b"class Kept {}")],
    )
    .await;
    pass(&world).await;
    assert_eq!(found(&world, "kept"), [other]);
    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(SOURCE)
        .execute(&world.pool)
        .await
        .unwrap();
    let gone: i64 = sqlx::query_scalar("select count(*) from sync.code_gone")
        .fetch_one(&world.pool)
        .await
        .unwrap();
    assert_eq!(gone, 1);
}

/// D7 point 8: the index committed, the record lost (a crash between the
/// two). The next pass makes the same difference again, and each file is in
/// the index once.
#[tokio::test]
async fn a_crash_between_the_commit_and_the_record_is_healed_by_the_next_pass() {
    let world = world("code_files_crash", Forge::default()).await;
    let id = push(
        &world,
        "acme/crash",
        "c1",
        &[
            ("A.java", b"class Settlebridge {}"),
            ("B.java", b"class Other {}"),
        ],
    )
    .await;
    pass(&world).await;
    // The record of a later pass lost: its rows as they were before it.
    push(
        &world,
        "acme/crash",
        "c2",
        &[
            ("A.java", b"class Settlebridge { int x; }"),
            ("B.java", b"class Other {}"),
            ("C.java", b"class Settlebridge2 {}"),
        ],
    )
    .await;
    let before: Vec<(String, Vec<u8>)> =
        sqlx::query_as("select path, sha from sync.code_file order by path")
            .fetch_all(&world.pool)
            .await
            .unwrap();
    pass(&world).await;
    let mut tx = world.pool.begin().await.unwrap();
    sqlx::query("delete from sync.code_file")
        .execute(&mut *tx)
        .await
        .unwrap();
    for (path, sha) in &before {
        sqlx::query("insert into sync.code_file (repository_id, path, sha, size, has_text) values ($1, $2, $3, 1, true)")
            .bind(&id)
            .bind(path)
            .bind(sha)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query("update sync.code_repo set commit_sha = 'c1', files = 2")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let healed = pass(&world).await;
    assert_eq!(healed.repositories, 1);
    assert_eq!(world.files.index.held().unwrap()[&1], 3, "every file once");
    assert_eq!(
        world
            .files
            .index
            .in_repo(1, "settlebridge", Reads::BOTH, 8)
            .1,
        2
    );
    assert_eq!(row(&world, &id).await.0.as_deref(), Some("c2"));
}

#[tokio::test]
async fn bring_up_heals_a_count_that_differs_and_a_stray_number() {
    let world = world("code_files_reconcile", Forge::default()).await;
    let id = push(
        &world,
        "acme/r",
        "c1",
        &[
            ("A.java", b"class Settlebridge {}"),
            ("B.java", b"class B {}"),
        ],
    )
    .await;
    pass(&world).await;
    // The database says three files; the index holds two. And a number the
    // database never gave.
    sqlx::query("update sync.code_repo set files = 3")
        .execute(&world.pool)
        .await
        .unwrap();
    let mut writer = world.files.index.writer().unwrap();
    writer.put(99, "Stray.java", Some("stray")).unwrap();
    writer.commit().unwrap();
    drop(writer);
    let reconciled = code_files::reconcile(&world.pool, &world.files.index)
        .await
        .unwrap();
    assert_eq!((reconciled.again, reconciled.strays), (1, 1));
    assert_eq!(row(&world, &id).await, (None, 0, 0));
    let healed = pass(&world).await;
    assert_eq!((healed.repositories, healed.gone), (1, 1));
    assert_eq!(
        world.files.index.held().unwrap(),
        [(1, 2)].into_iter().collect()
    );
    assert_eq!(found(&world, "settlebridge"), [id]);
    // In step again: nothing to heal.
    let again = code_files::reconcile(&world.pool, &world.files.index)
        .await
        .unwrap();
    assert_eq!((again.again, again.strays), (0, 0));
}

#[tokio::test]
async fn a_forge_that_fails_stops_the_repository_and_keeps_its_commit() {
    let forge = Forge {
        fail_after: Some(1),
        ..Forge::default()
    };
    let world = world("code_files_down", forge).await;
    let id = push(
        &world,
        "acme/down",
        "c1",
        &[
            ("A.java", b"class A {}"),
            ("B.java", b"class B {}"),
            ("C.java", b"class C {}"),
        ],
    )
    .await;
    let passed = pass(&world).await;
    assert_eq!((passed.repositories, passed.owed), (0, 1), "{passed:?}");
    assert_eq!(row(&world, &id).await.0, None);
}

#[tokio::test]
async fn a_gitattributes_mark_and_latin_1_text_are_read_as_d4_says() {
    let world = world("code_files_rules", Forge::default()).await;
    let id = push(
        &world,
        "acme/rules",
        "c1",
        &[
            (".gitattributes", b"gen/ linguist-generated\n"),
            ("gen/Made.java", b"class MadeSettlebridge {}"),
            ("src/Gr.java", b"// Gr\xf6\xdfe \xe4ndern\n"),
            ("node_modules/x/index.js", b"settlebridge"),
        ],
    )
    .await;
    pass(&world).await;
    assert_eq!(row(&world, &id).await, (Some("c1".to_owned()), 4, 2));
    assert_eq!(found(&world, "größe"), std::slice::from_ref(&id));
    // Found by its path alone: the marked file's text and the folder's are not read.
    let (files, _) = world.files.index.in_repo(1, "settlebridge", Reads::BOTH, 8);
    assert!(files.is_empty(), "{files:?}");
}

/// D15: the flat tree is a walk by `sync.git_tree`'s key, a level at a time,
/// never a scan of every level the source stores, even with workers made to
/// look free on tables never analyzed.
#[tokio::test]
async fn the_flat_tree_reads_each_level_by_its_key() {
    let world = world("code_files_plan", Forge::default()).await;
    push(
        &world,
        "acme/plan",
        "c1",
        &[("a/b/C.java", b"class C {}"), ("D.java", b"class D {}")],
    )
    .await;
    let root: Vec<u8> = sqlx::query_scalar("select root from sync.branch_tree limit 1")
        .fetch_one(&world.pool)
        .await
        .unwrap();
    let mut tx = world.pool.begin().await.unwrap();
    sqlx::query(
        "select set_config('parallel_setup_cost', '0', true), set_config('parallel_tuple_cost', '0', true),
                set_config('min_parallel_table_scan_size', '0', true), set_config('min_parallel_index_scan_size', '0', true)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let plan: serde_json::Value = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "explain (format json) {}",
        code_files::FLAT_TREE_SQL
    )))
    .bind(SOURCE)
    .bind(&root)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    // Every read of `git_tree` is by its key, the sha in the condition.
    fn reads(node: &serde_json::Value, out: &mut Vec<String>) {
        if node["Relation Name"] == "git_tree" {
            out.push(format!("{} {}", node["Node Type"], node["Index Cond"]));
        }
        for child in node["Plans"].as_array().into_iter().flatten() {
            reads(child, out);
        }
    }
    let mut found = Vec::new();
    reads(&plan[0]["Plan"], &mut found);
    assert_eq!(found.len(), 2, "{plan}");
    assert!(found.iter().all(|read| read.contains("sha =")), "{found:?}");
    tx.rollback().await.unwrap();
}

/// Two sources' passes at once on the one index: the second waits for the
/// first rather than finding the writer busy and reading nothing until its
/// next run. Gitea's and Forgejo's first runs end together; the live run of
/// 2026-11-21 read Forgejo's files after its first run and Gitea's only
/// after its second, five minutes later.
#[tokio::test]
async fn two_sources_passes_at_once_both_read() {
    let world = world("code_files_two_sources", Forge::default()).await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('forge-two', 'gitea', 'Forge two', 'http://y', 'Pat')",
    )
    .execute(&world.pool)
    .await
    .unwrap();
    push(&world, "acme/one", "c1", &[("a.txt", b"alpha")]).await;
    push_to(&world, "forge-two", "acme/two", "c1", &[("b.txt", b"beta")]).await;
    let run = |source: &'static str| {
        code_files::refresh_within(
            &world.pool,
            source,
            &world.forge,
            &world.files,
            Bounds::default(),
            u64::MAX,
            &|_| {},
        )
    };
    let (one, two) = tokio::join!(run(SOURCE), run("forge-two"));
    assert_eq!(one.unwrap().repositories, 1);
    assert_eq!(two.unwrap().repositories, 1);
}
