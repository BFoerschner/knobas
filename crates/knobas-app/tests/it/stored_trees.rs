//! Every branch's tree, stored by the sync and drawn by *Files* with no
//! request to the forge (the stored-trees pass of PR #2479, 2026-11-14;
//! migration `0085`): `knobas_sync::stored_trees::refresh` over a seeded
//! mirror, then a page's reads through `sources::crud::shown`.
//!
//! The adapter is Gitea's for everything pure (its tree shape, what a
//! section asks of the store, the section drawn from it) and a fake for the
//! two reads (`read_tree`, `view`), which count what they are asked.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use knobas_app::commands::search::search_inner;
use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_search::{SearchFilters, SearchQuery};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::tree::{HeldTrees, MODE_TREE, StoredTree, TreeItem, TreeLevel, TreeRead};
use knobas_source::view::SourceView;
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

const WEB: &str = "https://gitea.example/tidewater/svc";

fn sha(n: u32) -> String {
    format!("{n:040x}")
}

fn blob(name: &str, n: u32) -> TreeItem {
    TreeItem {
        name: name.to_owned(),
        mode: 0o100_644,
        size: u64::from(n),
        sha: sha(n),
    }
}

fn dir(name: &str, n: u32) -> TreeItem {
    TreeItem {
        name: name.to_owned(),
        mode: MODE_TREE,
        size: 0,
        sha: sha(n),
    }
}

/// Each commit's root key and its levels. `c1`: README, src (`0xa1`).
/// `c2`: README, docs (`0xd1`), src (`0xa2`). `c3`: README changed, docs and
/// src as `c1`'s: only its root is new.
fn commits() -> HashMap<String, (String, Vec<TreeLevel>)> {
    let level = |n: u32, entries: Vec<TreeItem>| TreeLevel {
        sha: sha(n),
        entries,
    };
    let src1 = level(0xa1, vec![blob("lib.rs", 0xf2)]);
    let src2 = level(0xa2, vec![blob("lib.rs", 0xf3), blob("main.rs", 0xf6)]);
    let docs = level(0xd1, vec![dir("guide", 0xd2), blob("index.md", 0xf4)]);
    let guide = level(0xd2, vec![blob("a.md", 0xf7)]);
    HashMap::from([
        (
            sha(0xc1),
            (
                sha(0x01),
                vec![
                    level(0x01, vec![blob("README.md", 0xf1), dir("src", 0xa1)]),
                    src1.clone(),
                ],
            ),
        ),
        (
            sha(0xc2),
            (
                sha(0x02),
                vec![
                    level(
                        0x02,
                        vec![blob("README.md", 0xf1), dir("docs", 0xd1), dir("src", 0xa2)],
                    ),
                    docs.clone(),
                    guide.clone(),
                    src2,
                ],
            ),
        ),
        (
            sha(0xc3),
            (
                sha(0x03),
                vec![
                    level(
                        0x03,
                        vec![blob("README.md", 0xf5), dir("docs", 0xd1), dir("src", 0xa1)],
                    ),
                    docs,
                    guide,
                    src1,
                ],
            ),
        ),
    ])
}

/// What the fake was asked: each tree read (branch commit, levels held for
/// it), each view.
#[derive(Default)]
struct Asked {
    trees: Vec<String>,
    views: Vec<(String, Option<String>)>,
}

struct Forge {
    pure: Box<dyn Source>,
    asked: Arc<Mutex<Asked>>,
}

#[async_trait::async_trait]
impl Source for Forge {
    fn descriptor(&self) -> SourceDescriptor {
        self.pure.descriptor()
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<SourceView>, SourceError> {
        self.asked
            .lock()
            .unwrap()
            .views
            .push((entity.to_owned(), section.map(str::to_owned)));
        Ok(Some(SourceView::new(Vec::new())))
    }
    fn view_standing(&self, entity: &str, section: Option<&str>) -> knobas_source::view::Standing {
        self.pure.view_standing(entity, section)
    }
    fn tree_shape(&self) -> Option<knobas_source::tree::TreeShape> {
        self.pure.tree_shape()
    }
    async fn read_tree(
        &self,
        _repository: &str,
        commit: &str,
        held: &dyn HeldTrees,
        cap: u64,
    ) -> Result<Option<TreeRead>, SourceError> {
        self.asked.lock().unwrap().trees.push(commit.to_owned());
        if commit == sha(0xbb) {
            return Ok(Some(TreeRead::TooLarge {
                entries: cap + 1,
                requests: 1,
            }));
        }
        let (root, levels) = commits()
            .remove(commit)
            .ok_or_else(|| SourceError::protocol("no such commit"))?;
        if held.held(&root).is_some() {
            return Ok(Some(TreeRead::Read {
                root,
                levels: Vec::new(),
                requests: 1,
            }));
        }
        let levels: Vec<TreeLevel> = levels
            .into_iter()
            .filter(|l| held.held(&l.sha).is_none())
            .collect();
        let requests = u32::try_from(levels.len()).unwrap();
        Ok(Some(TreeRead::Read {
            root,
            levels,
            requests,
        }))
    }
    fn tree_ask(
        &self,
        entity: &str,
        section: Option<&str>,
        payload: &serde_json::Value,
    ) -> Option<knobas_source::tree::TreeAsk> {
        self.pure.tree_ask(entity, section, payload)
    }
    fn tree_view(
        &self,
        entity: &str,
        section: Option<&str>,
        payload: &serde_json::Value,
        tree: &StoredTree,
    ) -> Option<SourceView> {
        self.pure.tree_view(entity, section, payload, tree)
    }
}

struct ForgeRegistry(Arc<Mutex<Asked>>);

fn gitea(id: &str) -> Box<dyn Source> {
    Registry::for_process()
        .build(SourceInstance {
            id: id.to_owned(),
            kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: "https://gitea.example".to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some("pat".to_owned()),
            account: None,
            config: serde_json::json!({}),
        })
        .expect("Gitea builds")
}

impl AdapterRegistry for ForgeRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Forge {
            pure: gitea(&instance.id),
            asked: self.0.clone(),
        }))
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    asked: Arc<Mutex<Asked>>,
    id: String,
    repo: String,
    live: knobas_app::sources::live::LiveSources,
}

impl Fixture {
    fn forge(&self) -> Forge {
        Forge {
            pure: gitea(&self.id),
            asked: self.asked.clone(),
        }
    }

    fn branch(&self, name: &str) -> String {
        format!("{}@refs/heads/{name}", self.repo)
    }

    async fn refresh(&self) -> knobas_sync::stored_trees::Filled {
        knobas_sync::stored_trees::refresh(&self.pool, &self.id, &self.forge())
            .await
            .unwrap()
    }

    fn trees_asked(&self) -> Vec<String> {
        std::mem::take(&mut self.asked.lock().unwrap().trees)
    }

    fn views_asked(&self) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut self.asked.lock().unwrap().views)
    }

    async fn shown(&self, entity: &str, section: &str) -> knobas_app::sources::live::ShownView {
        sources::crud::shown(
            &self.pool,
            &self.secrets,
            &crate::support::shared(ForgeRegistry(self.asked.clone())),
            &self.live,
            entity,
            Some(section),
        )
        .await
        .unwrap()
        .expect("drawn")
    }

    async fn item(
        &self,
        id: &str,
        kind: &str,
        title: &str,
        payload: serde_json::Value,
        hours_ago: i32,
        web: Option<&str>,
    ) {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url,
                                    item_updated_at, synced_at)
             values ($1, $2, $3, $4, '', $5, $6, now() - make_interval(hours => $7), now())",
        )
        .bind(id)
        .bind(&self.id)
        .bind(kind)
        .bind(title)
        .bind(payload)
        .bind(web)
        .bind(hours_ago)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn branch_item(&self, name: &str, commit: u32, hours_ago: i32) {
        let payload = serde_json::json!({
            "name": name, "commit": { "id": sha(commit) },
            "repository": { "full_name": "tidewater/svc", "name": "svc", "default_branch": "main" }
        });
        self.item(&self.branch(name), "branch", name, payload, hours_ago, None)
            .await;
    }

    async fn move_branch(&self, name: &str, commit: u32) {
        sqlx::query(
            "update sync.item set payload = jsonb_set(payload, '{commit,id}', to_jsonb($2::text))
              where entity_id = $1",
        )
        .bind(self.branch(name))
        .bind(sha(commit))
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// `(branch, commit, root)` of every stored row, by branch name.
    async fn rows(&self) -> Vec<(String, String, Option<String>)> {
        sqlx::query_as(
            "select branch, commit_sha, encode(root, 'hex') from sync.branch_tree
              where source_id = $1 order by branch",
        )
        .bind(&self.id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn levels(&self) -> Vec<String> {
        sqlx::query_scalar(
            "select encode(sha, 'hex') from sync.git_tree where source_id = $1 order by 1",
        )
        .bind(&self.id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    /// Every stored row's version: a write moves it.
    async fn versions(&self) -> Vec<String> {
        sqlx::query_scalar(
            "select xmin::text from sync.branch_tree where source_id = $1
             union all
             select xmin::text from sync.git_tree where source_id = $1
             order by 1",
        )
        .bind(&self.id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

fn record() -> serde_json::Value {
    serde_json::json!({
        "id": 1, "name": "svc", "full_name": "tidewater/svc", "owner": { "login": "tidewater" },
        "default_branch": "main", "html_url": WEB, "private": false, "internal": false,
        "empty": false, "mirror": false, "has_pull_requests": true,
        "stars_count": 0, "forks_count": 0, "size": 12
    })
}

/// A source with one repository and its branches: `main` and `old` at `c1`,
/// `feat` at `c2` with an open pull request, `stale` at `c2` and older than
/// the rest.
async fn fixture() -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let asked: Arc<Mutex<Asked>> = Arc::default();
    let id = format!("gt{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
    sources::crud::add(
        &pool,
        &secrets,
        &ForgeRegistry(asked.clone()),
        NewSource {
            id: id.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: "https://gitea.example".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({ "username": "bjoern" }),
            secret: SecretInput::of("pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    // The declarations, stored as every launcher opening stores them: what
    // names a branch's repository.
    search_inner(
        &pool,
        &Registry::for_process(),
        SearchQuery {
            titles: false,
            raw: "warmup".to_owned(),
            limit: 5,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
    let live = knobas_app::sources::live::LiveSources::default();
    live.hold_in(pool.clone());
    let f = Fixture {
        pool,
        secrets,
        asked,
        repo: format!("{id}:tidewater/svc"),
        id,
        live,
    };
    f.item(&f.repo, "repo", "tidewater/svc", record(), 1, Some(WEB))
        .await;
    f.branch_item("stale", 0xc2, 90).await;
    f.branch_item("old", 0xc1, 5).await;
    f.branch_item("feat", 0xc2, 9).await;
    f.branch_item("main", 0xc1, 3).await;
    f.item(
        &format!("{}#7", f.repo),
        "pr",
        "Feature",
        serde_json::json!({
            "number": 7, "state": "open", "title": "Feature",
            "head": { "ref": "feat" },
            "base": { "ref": "main", "repo": { "full_name": "tidewater/svc", "name": "svc" } }
        }),
        2,
        None,
    )
    .await;
    f
}

/// The first fill reads each branch once, in order of use (the default
/// branch, a pull request's, then the rest newest first), and stores each
/// directory once: `old` stands where `main` does and reads nothing new,
/// `stale` where `feat` does.
#[tokio::test]
async fn every_branch_is_stored_once_by_its_trees_in_order_of_use() {
    let f = fixture().await;
    let filled = f.refresh().await;
    assert_eq!(
        f.trees_asked(),
        [sha(0xc1), sha(0xc2), sha(0xc1), sha(0xc2)],
        "main, feat (its pull request), old, stale"
    );
    assert_eq!(filled.branches, 4);
    assert_eq!(filled.levels, 6, "two roots, src twice, docs and its guide");
    assert_eq!(
        f.rows().await,
        [
            ("feat".to_owned(), sha(0xc2), Some(sha(0x02))),
            ("main".to_owned(), sha(0xc1), Some(sha(0x01))),
            ("old".to_owned(), sha(0xc1), Some(sha(0x01))),
            ("stale".to_owned(), sha(0xc2), Some(sha(0x02))),
        ]
    );
    assert_eq!(
        f.levels().await,
        [
            sha(0x01),
            sha(0x02),
            sha(0xa1),
            sha(0xa2),
            sha(0xd1),
            sha(0xd2)
        ]
    );
    let entries: Vec<(String, i64)> = sqlx::query_as(
        "select branch, entries from sync.branch_tree where source_id = $1 order by branch",
    )
    .bind(&f.id)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        entries[0],
        ("feat".to_owned(), 3 + 2 + 1 + 2),
        "every level below counted"
    );
    assert_eq!(entries[1], ("main".to_owned(), 2 + 1));
}

/// A run that moved nothing reads nothing and writes nothing, even after the
/// sync stamped every row it saw; a push reads that branch alone, reusing
/// the levels held, and the level no branch reaches any more goes.
#[tokio::test]
async fn only_a_moved_branch_is_read_and_what_no_branch_reaches_goes() {
    let f = fixture().await;
    f.refresh().await;
    f.trees_asked();
    let before = f.versions().await;
    sqlx::query("update sync.item set synced_at = now() where source_id = $1")
        .bind(&f.id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.refresh().await,
        knobas_sync::stored_trees::Filled::default()
    );
    assert!(f.trees_asked().is_empty(), "no request");
    assert_eq!(f.versions().await, before, "no write");

    // `feat` and `stale` are pushed (a force-push alike) to `c3`, which
    // keeps `docs` and goes back to `c1`'s `src`: one root is new, and
    // `c2`'s root and its `src` are reached by nobody.
    f.move_branch("feat", 0xc3).await;
    f.move_branch("stale", 0xc3).await;
    let filled = f.refresh().await;
    assert_eq!(f.trees_asked(), [sha(0xc3), sha(0xc3)]);
    assert_eq!(filled.levels, 1, "only c3's root");
    assert_eq!(filled.removed, 2, "c2's root and src");
    assert_eq!(
        f.levels().await,
        [sha(0x01), sha(0x03), sha(0xa1), sha(0xd1), sha(0xd2)]
    );
}

/// A branch or repository withdrawn by the sync loses its row in the same
/// statement (the sync's transaction), and the next pass removes what only
/// it reached.
#[tokio::test]
async fn a_withdrawn_branch_or_repository_drops_its_tree() {
    let f = fixture().await;
    f.refresh().await;
    for name in ["feat", "stale"] {
        sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
            .bind(f.branch(name))
            .execute(&f.pool)
            .await
            .unwrap();
    }
    assert_eq!(
        f.rows()
            .await
            .iter()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        ["main", "old"],
        "gone with the tombstone"
    );
    let filled = f.refresh().await;
    assert_eq!(filled.removed, 4, "c2's root, its src, docs and guide");
    assert_eq!(f.levels().await, [sha(0x01), sha(0xa1)]);

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&f.repo)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        f.rows().await.is_empty(),
        "a repository's branches go with it"
    );
    f.refresh().await;
    assert!(f.levels().await.is_empty());
}

/// *Files* draws a branch's top, any folder at its commit and *Go to file*
/// from the store, as Gitea draws them from the same levels: the top as of
/// the sync and read behind as before, a folder and every file at the
/// commit with no request at all.
#[tokio::test]
async fn files_draws_from_the_store_without_waiting_on_the_forge() {
    let f = fixture().await;
    f.refresh().await;
    let (root, levels) = commits().remove(&sha(0xc2)).unwrap();
    let tree = StoredTree {
        commit: sha(0xc2),
        root,
        levels: levels.into_iter().map(|l| (l.sha, l.entries)).collect(),
        repository_url: Some(WEB.to_owned()),
    };
    let pure = gitea(&f.id);
    let feat = f.branch("feat");
    let synced: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select synced_at from sync.item where entity_id = $1")
            .bind(&feat)
            .fetch_one(&f.pool)
            .await
            .unwrap();

    let top = f.shown(&feat, "files").await;
    assert_eq!(
        Some(top.view.clone()),
        pure.tree_view(&feat, Some("files"), &serde_json::json!({}), &tree)
    );
    assert_eq!(
        top.held_at,
        Some(synced),
        "as of the sync that saw the head"
    );
    f.live.settled().await;
    assert_eq!(
        f.views_asked(),
        [(feat.clone(), Some("files".to_owned()))],
        "the top is checked behind, as before"
    );

    for section in [
        format!("files@{}:docs/guide", sha(0xc2)),
        format!("paths@{}", sha(0xc2)),
    ] {
        let shown = f.shown(&feat, &section).await;
        assert_eq!(
            Some(shown.view),
            pure.tree_view(&feat, Some(&section), &serde_json::json!({}), &tree),
            "{section}"
        );
        assert_eq!(shown.held_at, None, "a commit's tree does not move");
    }
    let top = f.shown(&f.repo, "files").await;
    assert_eq!(top.view.sections[0].blocks.len(), 1);
    f.live.settled().await;
    assert_eq!(
        f.views_asked(),
        [(f.repo.clone(), Some("files".to_owned()))],
        "the repository's top is main's, checked behind; nothing else asked"
    );
}

/// A branch the mirror holds at a commit its row was not read at is the
/// forge's, as before; one past the cap too, saying how large it is.
#[tokio::test]
async fn a_moved_branch_or_one_past_the_cap_is_read_from_the_forge() {
    let f = fixture().await;
    f.refresh().await;
    f.move_branch("feat", 0xc3).await;
    let shown = f.shown(&f.branch("feat"), "files").await;
    assert_eq!(shown.held_at, None);
    assert_eq!(
        f.views_asked(),
        [(f.branch("feat"), Some("files".to_owned()))]
    );

    f.branch_item("huge", 0xbb, 1).await;
    let filled = f.refresh().await;
    assert_eq!(filled.too_large, 1);
    let shown = f.shown(&f.branch("huge"), "files").await;
    assert_eq!(
        shown.tree_unkept,
        Some(knobas_sync::stored_trees::TREE_CAP + 1)
    );
    assert_eq!(
        f.views_asked(),
        [(f.branch("huge"), Some("files".to_owned()))]
    );
    let wire = serde_json::to_value(&shown).unwrap();
    assert_eq!(wire["tree_unkept"], knobas_sync::stored_trees::TREE_CAP + 1);
}

/// No index of the new tables holds a column a stamp moves.
#[tokio::test]
async fn no_stored_tree_index_holds_synced_at() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let defs: Vec<String> = sqlx::query_scalar(
        "select indexdef from pg_indexes
          where schemaname = 'sync' and tablename in ('git_tree', 'branch_tree', 'tree_gc')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!defs.is_empty());
    assert!(
        defs.iter()
            .all(|d| !d.contains("synced_at") && !d.contains("read_at")),
        "{defs:#?}"
    );
}
