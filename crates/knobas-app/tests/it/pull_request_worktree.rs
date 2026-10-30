//! A pull request opened in a worktree under the clones root (#2285,
//! ADR-0035), and the *Diff tool* (#2275), over a scratch database and a real
//! `git`.
//!
//! The server is a bare repository in a temporary folder that serves
//! `refs/pull/<n>/head` as Gitea does, fetched from over `file://` through
//! `open_object_admitting`, the test-only door (the production flags are
//! https and ssh, `knobas_core::checkout::worktree`'s tests). The network half
//! against the testenv Gitea and Forgejo is `tests/clone_live.rs`.
//!
//! Each open is held to the person's checkout being where it was: its branch,
//! its `HEAD` and its `git status`, read before and after.

use std::fs;
use std::path::{Path, PathBuf};

use knobas_app::IpcErrorCode;
use knobas_app::checkout::open::open_object_admitting;
use knobas_app::checkout::worktree::{self, diff_tool, open_diff_tool};
use knobas_app::checkout::{set_applications, set_clones_root, set_override};
use knobas_app::sources::Registry;
use knobas_core::checkout::open::{Draft, Object};
use sqlx::PgPool;

async fn pool() -> PgPool {
    knobas_db::test_util::scratch_database("pr-worktree")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

/// `git -C dir args`, its stdout, or a panic naming git's words.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=k",
            "-c",
            "user.email=k@example.com",
            "-c",
            "protocol.file.allow=always",
        ])
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Where the person's checkout stands: branch, `HEAD`, `git status`.
fn standing(checkout: &Path) -> (String, String, String) {
    (
        git(checkout, &["branch", "--show-current"]),
        git(checkout, &["rev-parse", "HEAD"]),
        git(
            checkout,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        ),
    )
}

/// Everything one test needs: a server, a person's checkout with work of
/// their own in it, a clones root, a repo in the mirror and an application.
struct World {
    pool: PgPool,
    _dirs: tempfile::TempDir,
    server: PathBuf,
    /// A working clone of the server that pushes the pull request's heads.
    author: PathBuf,
    checkout: PathBuf,
    root: PathBuf,
    repo: String,
    source: String,
    record: PathBuf,
}

const OWNER: &str = "tidewater";

impl World {
    async fn new() -> Self {
        let pool = pool().await;
        let dirs = tempfile::tempdir().unwrap();
        let name = format!("payout-{}", unique());
        let served = dirs.path().join("server");
        let server = served.join(OWNER).join(format!("{name}.git"));
        fs::create_dir_all(&server).unwrap();
        git(&server, &["init", "-q", "--bare", "-b", "main"]);
        let author = dirs.path().join("author");
        git(
            dirs.path(),
            &["clone", "-q", &server.to_string_lossy(), "author"],
        );
        fs::write(author.join("README"), "one\n").unwrap();
        fs::write(author.join("lib.rs"), "fn main() {}\n").unwrap();
        git(&author, &["add", "."]);
        git(&author, &["commit", "-qm", "one"]);
        git(&author, &["push", "-q", "origin", "main"]);

        // The person's own checkout, on a branch of theirs, with a change
        // they have not committed and a file git does not know.
        let checkout = dirs.path().join("mine");
        git(
            dirs.path(),
            &["clone", "-q", &server.to_string_lossy(), "mine"],
        );
        git(&checkout, &["checkout", "-qb", "my-work"]);
        fs::write(checkout.join("README"), "mine, uncommitted\n").unwrap();
        fs::write(checkout.join("notes.txt"), "untracked\n").unwrap();

        let source = format!("gitea-{}", unique());
        let base = format!("file://localhost{}", served.display());
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, 'gitea', $1, $2, 'token')",
        )
        .bind(&source)
        .bind(&base)
        .execute(&pool)
        .await
        .unwrap();
        let repo = item(&pool, &source, "repo", &format!("{OWNER}/{name}"), "{}").await;
        set_override(&pool, &repo, Some(&checkout.to_string_lossy()))
            .await
            .unwrap();
        let root = dirs.path().join("code");
        set_clones_root(&pool, Some(&root.to_string_lossy()))
            .await
            .unwrap();

        let record = dirs.path().join("record");
        let stub = dirs.path().join("stub");
        recording_stub(&stub, &record);
        set_applications(
            &pool,
            &[Draft {
                id: Some("editor".to_owned()),
                name: "Editor".to_owned(),
                command: format!("{} {{path}}", stub.display()),
                pull_request_command: Some(format!(
                    "{} {{path}} {{name}} {{pr}} {{head}}",
                    stub.display()
                )),
                ..Draft::default()
            }],
            None,
        )
        .await
        .unwrap();
        Self {
            pool,
            _dirs: dirs,
            server,
            author,
            checkout,
            root,
            repo,
            source,
            record,
        }
    }

    fn name(&self) -> String {
        self.server
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    /// Commit `text` into `file` on the author's `pr-<n>` branch and serve it
    /// as `refs/pull/<n>/head`, as Gitea does on a push. Answers the sha.
    fn push(&self, n: u64, file: &str, text: &str) -> String {
        let branch = format!("pr-{n}");
        if git(&self.author, &["branch", "--list", &branch]).is_empty() {
            git(&self.author, &["checkout", "-qb", &branch, "main"]);
        } else {
            git(&self.author, &["checkout", "-q", &branch]);
        }
        fs::write(self.author.join(file), text).unwrap();
        git(&self.author, &["add", "."]);
        git(&self.author, &["commit", "-qm", text.trim()]);
        git(
            &self.author,
            &[
                "push",
                "-qf",
                "origin",
                &format!("{branch}:refs/pull/{n}/head"),
            ],
        );
        git(&self.author, &["rev-parse", "HEAD"])
    }

    /// The pull request's own mirror row, its payload's `state` as given.
    async fn pull(&self, n: u64, state: &str) -> String {
        let payload = serde_json::json!({ "state": state }).to_string();
        item(
            &self.pool,
            &self.source,
            "pr",
            &format!("{OWNER}/{}#{n}", self.name()),
            &payload,
        )
        .await
    }

    async fn set_state(&self, item: &str, state: &str) {
        sqlx::query("update sync.item set payload = jsonb_build_object('state', $2::text) where entity_id = $1")
            .bind(item)
            .bind(state)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// Open pull request `n` in the editor; on success, wait for the stub, so
    /// its record never lands in the next step's.
    async fn open(&self, n: u64, item: Option<&str>) -> Result<Vec<String>, knobas_app::IpcError> {
        let _ = fs::remove_file(&self.record);
        let answer = self.open_only(n, item).await;
        if answer.is_ok() {
            recorded(&self.record).await;
        }
        answer
    }

    async fn open_only(
        &self,
        n: u64,
        item: Option<&str>,
    ) -> Result<Vec<String>, knobas_app::IpcError> {
        open_object_admitting(
            &self.pool,
            Some(&Registry::for_process()),
            &self.repo,
            Some("editor"),
            &Object::PullRequest {
                number: n,
                base: "main".into(),
                head: format!("pr-{n}"),
                branch: format!("pr-{n}"),
                item: item.map(str::to_owned),
            },
            &["file"],
        )
        .await
    }

    fn worktree(&self, n: u64) -> PathBuf {
        self.root
            .join("pull-requests")
            .join(OWNER)
            .join(format!("{}-{n}", self.name()))
    }
}

async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, payload: &str) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1, $2, $3, $3, '', $4::jsonb, null)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A stub that writes its arguments, one per line, then renames the record
/// into place (`checkout_ipc.rs`'s `recording_stub`, #583).
fn recording_stub(stub: &Path, record: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(
        stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.tmp'\nmv '{0}.tmp' '{0}'\n",
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(stub, fs::Permissions::from_mode(0o755)).unwrap();
}

async fn recorded(record: &Path) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        if let Ok(text) = fs::read_to_string(record)
            && text.ends_with('\n')
        {
            return text.lines().map(str::to_owned).collect();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("nothing recorded at {}", record.display());
}

/// The first open makes the worktree at the pull request's head and opens
/// the application there; the person's checkout keeps its branch, its HEAD,
/// its uncommitted change and its untracked file.
#[tokio::test]
async fn a_pull_request_opens_in_a_worktree_at_its_head_and_the_checkout_is_untouched() {
    let w = World::new().await;
    let head = w.push(7, "lib.rs", "fn pay() {}\n");
    let before = standing(&w.checkout);

    let argv = w.open(7, None).await.unwrap();

    let wt = w.worktree(7);
    let expected = vec![
        wt.to_string_lossy().into_owned(),
        format!("{}-7", w.name()),
        "7".to_owned(),
        "pr-7".to_owned(),
    ];
    assert_eq!(recorded(&w.record).await, expected);
    assert_eq!(argv[1..], expected);
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(wt.join("lib.rs")).unwrap(),
        "fn pay() {}\n"
    );
    assert_eq!(
        git(&wt, &["status", "--porcelain=v1"]),
        "",
        "a clean worktree"
    );
    assert_eq!(
        standing(&w.checkout),
        before,
        "the person's checkout is untouched"
    );
    assert_eq!(
        fs::read_to_string(w.checkout.join("README")).unwrap(),
        "mine, uncommitted\n"
    );
    assert!(
        !w.checkout.join(".git/FETCH_HEAD").exists(),
        "the fetch wrote the worktree's FETCH_HEAD, not the checkout's"
    );
    assert_eq!(
        worktree::list(&w.pool).await.unwrap(),
        [worktree::PullRequestWorktree {
            repo_entity_id: w.repo.clone(),
            number: 7,
            path: wt.to_string_lossy().into_owned(),
            closed: false,
        }]
    );
}

/// A second open after a push moves the worktree to the new head; a change
/// in the worktree that the next head would overwrite is refused in git's
/// words and kept.
#[tokio::test]
async fn a_second_open_fetches_and_moves_the_worktree_and_never_overwrites_a_change() {
    let w = World::new().await;
    w.push(7, "lib.rs", "fn pay() {}\n");
    w.open(7, None).await.unwrap();
    let before = standing(&w.checkout);
    let newer = w.push(7, "lib.rs", "fn pay() { retry() }\n");

    w.open(7, None).await.unwrap();

    let wt = w.worktree(7);
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), newer);
    assert_eq!(
        fs::read_to_string(wt.join("lib.rs")).unwrap(),
        "fn pay() { retry() }\n"
    );

    fs::write(wt.join("lib.rs"), "my fix\n").unwrap();
    let newest = w.push(7, "lib.rs", "fn pay() { retry(3) }\n");
    let error = w.open(7, None).await.unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Conflict, "{error:?}");
    assert!(error.message.contains("lib.rs"), "{}", error.message);
    assert_eq!(fs::read_to_string(wt.join("lib.rs")).unwrap(), "my fix\n");
    assert_ne!(git(&wt, &["rev-parse", "HEAD"]), newest);
    assert!(!w.record.exists(), "nothing ran");
    assert_eq!(standing(&w.checkout), before);
}

/// The next open of any pull request removes the worktree of one the mirror
/// now holds closed; one with a change in it stays.
#[tokio::test]
async fn a_closed_pull_requests_worktree_is_removed_on_the_next_open() {
    let w = World::new().await;
    w.push(7, "a.txt", "seven\n");
    w.push(8, "b.txt", "eight\n");
    w.push(9, "c.txt", "nine\n");
    let seven = w.pull(7, "open").await;
    let nine = w.pull(9, "open").await;
    w.open(7, Some(&seven)).await.unwrap();
    w.open(9, Some(&nine)).await.unwrap();
    fs::write(w.worktree(9).join("wip.txt"), "unsaved\n").unwrap();
    w.set_state(&seven, "closed").await;
    w.set_state(&nine, "merged").await;
    let before = standing(&w.checkout);

    w.open(8, None).await.unwrap();

    assert!(!w.worktree(7).exists(), "closed: removed");
    assert!(
        w.worktree(9).join("wip.txt").exists(),
        "an untracked file keeps it"
    );
    let listed: Vec<(u64, bool)> = worktree::list(&w.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|t| (t.number, t.closed))
        .collect();
    assert_eq!(listed, [(9, true), (8, false)]);
    let registered = git(&w.checkout, &["worktree", "list", "--porcelain"]);
    assert!(
        !registered.contains(&format!("{}-7", w.name())),
        "{registered}"
    );
    assert_eq!(standing(&w.checkout), before);
}

/// Settings removes one by hand, refuses one with changes in git's words,
/// and forgets one whose folder is gone.
#[tokio::test]
async fn settings_removes_a_worktree_and_git_refuses_one_with_changes() {
    let w = World::new().await;
    w.push(7, "a.txt", "seven\n");
    w.push(8, "b.txt", "eight\n");
    w.open(7, None).await.unwrap();
    w.open(8, None).await.unwrap();
    let (seven, eight) = (w.worktree(7), w.worktree(8));
    fs::write(eight.join("b.txt"), "changed\n").unwrap();

    let left = worktree::remove(&w.pool, &seven.to_string_lossy())
        .await
        .unwrap();
    assert!(!seven.exists());
    assert_eq!(left.len(), 1);

    let error = worktree::remove(&w.pool, &eight.to_string_lossy())
        .await
        .unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Conflict, "{error:?}");
    assert!(eight.join("b.txt").exists());

    let error = worktree::remove(&w.pool, &w.checkout.to_string_lossy())
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        IpcErrorCode::NotFound,
        "the person's checkout is never one"
    );
    assert!(w.checkout.join("README").exists());
}

/// A folder at the worktree's place that knobas did not make is refused and
/// left as it is; a first fetch that fails takes its empty worktree back.
#[tokio::test]
async fn a_folder_knobas_did_not_make_is_refused_and_a_failed_fetch_leaves_nothing() {
    let w = World::new().await;
    w.push(7, "a.txt", "seven\n");
    let squatter = w.worktree(7);
    fs::create_dir_all(&squatter).unwrap();
    fs::write(squatter.join("mine.txt"), "keep\n").unwrap();

    let error = w.open(7, None).await.unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Conflict, "{error:?}");
    assert_eq!(
        fs::read_to_string(squatter.join("mine.txt")).unwrap(),
        "keep\n"
    );

    let before = standing(&w.checkout);
    let error = w.open(41, None).await.unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Invalid, "{error:?}");
    assert!(error.message.contains("#41"), "{}", error.message);
    assert!(!w.worktree(41).exists());
    assert!(worktree::list(&w.pool).await.unwrap().is_empty());
    let registered = git(&w.checkout, &["worktree", "list", "--porcelain"]);
    assert!(!registered.contains("-41"), "{registered}");
    assert_eq!(standing(&w.checkout), before);
}

/// The Diff tool is offered for two shas the checkout holds and nothing
/// else; it runs the tool the checkout's git configuration names, on the two
/// trees.
#[tokio::test]
async fn the_diff_tool_is_offered_for_two_local_shas_and_runs_the_configured_tool() {
    let w = World::new().await;
    let base = git(&w.checkout, &["rev-parse", "HEAD"]);
    let head = w.push(7, "lib.rs", "fn pay() {}\n");
    let short = &head[..12];

    // Not in the checkout yet: not offered.
    assert_eq!(
        diff_tool(&w.pool, &w.repo, &base, &head).await.unwrap(),
        None
    );
    w.open(7, None).await.unwrap();
    let offered = diff_tool(&w.pool, &w.repo, &base, short)
        .await
        .unwrap()
        .expect("both shas are local now");
    assert_eq!(
        offered[1..],
        [
            "-C",
            &*w.checkout.to_string_lossy(),
            "difftool",
            "--dir-diff",
            "--gui",
            "--no-prompt",
            &base,
            short,
            "--"
        ]
    );
    let parent = format!("{short}^");
    assert!(
        diff_tool(&w.pool, &w.repo, &parent, short)
            .await
            .unwrap()
            .is_some()
    );
    for (b, h) in [
        ("main", short),
        (base.as_str(), "pr-7"),
        ("0123456789ab", short),
    ] {
        assert_eq!(
            diff_tool(&w.pool, &w.repo, b, h).await.unwrap(),
            None,
            "{b} {h}"
        );
    }
    let error = open_diff_tool(&w.pool, &w.repo, "main", short)
        .await
        .unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Invalid);

    // The person's tool: a stub that lists the head's side of the diff.
    let tool = w._dirs.path().join("tool");
    fs::write(
        &tool,
        format!(
            "#!/bin/sh\nls \"$2\" > '{0}.tmp'\nmv '{0}.tmp' '{0}'\n",
            w.record.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&w.checkout, &["config", "diff.guitool", "knobas-witness"]);
    git(
        &w.checkout,
        &[
            "config",
            "difftool.knobas-witness.cmd",
            &format!("{} \"$LOCAL\" \"$REMOTE\"", tool.display()),
        ],
    );
    let _ = fs::remove_file(&w.record);
    let before = standing(&w.checkout);
    open_diff_tool(&w.pool, &w.repo, &base, &head)
        .await
        .unwrap();
    assert_eq!(
        recorded(&w.record).await,
        ["lib.rs"],
        "the one file the range changes"
    );
    assert_eq!(standing(&w.checkout), before);
}
