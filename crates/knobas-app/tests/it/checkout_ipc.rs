//! The checkout surface as the interface sees it (#499).
//!
//! The *finding* is proven in `knobas-core`'s own battery
//! (`src/checkout.rs`'s normalisation cases and `tests/it/checkout_scan.rs`'s
//! walk over a temporary tree). What is asserted here is the join those two
//! cannot see: the clones root round-tripping through `knobas.setting`, the
//! override winning over the scan and giving it back when cleared, a branch
//! answering its repository's checkout, and a repo with no clone answering
//! *no checkout* with a clone command built from its own URL -- and, since
//! #501 and #844, that running an application starts a real process with the
//! checkout path, and a file inside it, and nothing else in its argument list.
//!
//! # Why every test gets a database of its own
//!
//! The clones root is **one `knobas.setting` row per database**, and every test
//! here either sets it or asserts what it is. Sharing this file's database --
//! the pattern `contexts_ipc.rs` uses, and the right one where every fixture is
//! namespaced by an id -- would put the tests in each other's setting: measured,
//! and three of them failed on it. A global setting cannot be namespaced, so
//! `scratch_database` is what keeps them apart. Fixture ids stay unique anyway,
//! so a test moved back onto a shared database would still be honest about
//! everything but the root.

use std::fs;
use std::path::{Path, PathBuf};

use knobas_app::IpcErrorCode;
use knobas_app::checkout::open::{APPLICATIONS_KEY, GitFound, legacy_key, open};
use knobas_app::checkout::uncommitted::DiffRead;
use knobas_app::checkout::working_tree::TreeRead;
use knobas_app::checkout::{
    CheckoutView, FoundBy, clones_root, local_changes, set_applications, set_clones_root,
    set_override, settings, view,
};
use knobas_core::checkout::open::{Draft, shipped};
use sqlx::PgPool;

const HOST: &str = "gitea.example.com";

async fn pool() -> PgPool {
    knobas_db::test_util::scratch_database("checkout-ipc")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

/// A token no other test in this file writes.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// One live mirror item: an entity row (so the override's foreign key holds)
/// and a mirror row (so the resolution can read its kind and URL).
async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, web_url: Option<&str>) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1,$2,$3,$4,'','{}'::jsonb,$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(web_url)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A repo and a branch of it, spelled the way interfaces §4.2 fixes Gitea's
/// keys: `owner/repo` and `owner/repo@refs/heads/<name>`.
struct Repo {
    source: String,
    owner: String,
    name: String,
    id: String,
    url: String,
}

async fn repo(pool: &PgPool) -> Repo {
    let token = unique();
    let source = format!("gitea-{token}");
    let owner = "tidewater".to_owned();
    let name = format!("payout-service-{token}");
    let url = format!("https://{HOST}/{owner}/{name}");
    let id = item(
        pool,
        &source,
        "repo",
        &format!("{owner}/{name}"),
        Some(&url),
    )
    .await;
    Repo {
        source,
        owner,
        name,
        id,
        url,
    }
}

async fn branch_of(pool: &PgPool, repo: &Repo, branch: &str) -> String {
    item(
        pool,
        &repo.source,
        "branch",
        &format!("{}/{}@refs/heads/{branch}", repo.owner, repo.name),
        Some(&format!("{}/src/branch/{branch}", repo.url)),
    )
    .await
}

/// A clones root holding a clone of `repo`, spelled as an ssh remote so the
/// match is the normalisation doing its work and not a string comparison.
fn clones_root_with(repo: &Repo) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(&repo.name);
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(
        dir.join(".git").join("config"),
        format!(
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@{HOST}:{}/{}.git\n",
            repo.owner, repo.name
        ),
    )
    .unwrap();
    root
}

fn path_of(view: &CheckoutView) -> &Path {
    Path::new(view.path.as_deref().expect("a checkout path"))
}

/// The setting round-trips, and a blank one clears it rather than storing an
/// empty root the scan would walk from the working directory.
#[tokio::test]
async fn the_clones_root_is_set_read_back_and_cleared() {
    let pool = pool().await;
    let root = tempfile::tempdir().unwrap();
    let spelled = root.path().to_string_lossy().into_owned();

    set_clones_root(&pool, Some(&spelled)).await.unwrap();
    assert_eq!(
        clones_root(&pool).await.unwrap().as_deref(),
        Some(spelled.as_str())
    );

    // Idempotent: setting it twice replaces the row rather than conflicting.
    set_clones_root(&pool, Some(&spelled)).await.unwrap();
    assert_eq!(
        clones_root(&pool).await.unwrap().as_deref(),
        Some(spelled.as_str())
    );

    set_clones_root(&pool, Some("   ")).await.unwrap();
    assert_eq!(clones_root(&pool).await.unwrap(), None);

    set_clones_root(&pool, Some(&spelled)).await.unwrap();
    set_clones_root(&pool, None).await.unwrap();
    assert_eq!(clones_root(&pool).await.unwrap(), None);
}

/// The scan, through the seam: a clones root is set, a repo detail is opened,
/// and it reads the clone on the disk.
#[tokio::test]
async fn a_repo_reads_the_clone_the_scan_found() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let root = clones_root_with(&repo);
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();

    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Scan);
    assert_eq!(path_of(&answer), root.path().join(&repo.name));
    assert_eq!(answer.repo_entity_id.as_deref(), Some(repo.id.as_str()));
    assert_eq!(answer.repo_url.as_deref(), Some(repo.url.as_str()));
    assert_eq!(
        answer.clone_command.as_deref(),
        Some(format!("git clone {}", repo.url).as_str())
    );
}

/// The override wins over the scan, and clearing it gives the scan back.
///
/// Both halves in one test on purpose: "the override wins" is only worth
/// anything if the scan would otherwise have answered something *else*, so the
/// clone the first assertion overrules is the one the last assertion finds.
#[tokio::test]
async fn an_override_wins_over_the_scan_and_clearing_it_gives_the_scan_back() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let root = clones_root_with(&repo);
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();
    let scanned = root.path().join(&repo.name);

    // The scan's answer, before anybody overrules it.
    assert_eq!(path_of(&view(&pool, &repo.id).await.unwrap()), scanned);

    let by_hand = "/Users/mara/work/payout-service-worktree";
    set_override(&pool, &repo.id, Some(by_hand)).await.unwrap();
    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Override);
    assert_eq!(answer.path.as_deref(), Some(by_hand));

    // Set twice: one answer per repository, replaced rather than doubled.
    let moved = "/Users/mara/elsewhere/payout-service";
    set_override(&pool, &repo.id, Some(moved)).await.unwrap();
    assert_eq!(
        view(&pool, &repo.id).await.unwrap().path.as_deref(),
        Some(moved)
    );
    let rows: i64 =
        sqlx::query_scalar("select count(*) from knobas.checkout_override where entity_id = $1")
            .bind(&repo.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);

    set_override(&pool, &repo.id, None).await.unwrap();
    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Scan);
    assert_eq!(path_of(&answer), scanned);
}

/// A branch shows its repository's checkout -- the scan's, and the override's
/// once one is set, because the override is keyed on the repository.
#[tokio::test]
async fn a_branch_answers_its_repositorys_checkout() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let branch = branch_of(&pool, &repo, "feature/PAY-231-sepa-retry").await;
    let root = clones_root_with(&repo);
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();

    let answer = view(&pool, &branch).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Scan);
    assert_eq!(path_of(&answer), root.path().join(&repo.name));
    assert_eq!(
        answer.repo_entity_id.as_deref(),
        Some(repo.id.as_str()),
        "the answer belongs to the repository, which is what an override keys on"
    );

    // Set from the *branch's* address, stored against the repository, and
    // therefore read by the repository's own detail too.
    let by_hand = "/Users/mara/work/payout-service";
    set_override(&pool, &branch, Some(by_hand)).await.unwrap();
    assert_eq!(
        view(&pool, &branch).await.unwrap().path.as_deref(),
        Some(by_hand)
    );
    assert_eq!(
        view(&pool, &repo.id).await.unwrap().path.as_deref(),
        Some(by_hand)
    );
}

/// A repo with no clone under the root reads *no checkout*, with the clone
/// command built from its own URL -- and the root it looked in, so the panel
/// can tell *not set* from *not found*.
#[tokio::test]
async fn a_repo_with_no_clone_reads_no_checkout_and_a_clone_command() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    // A root with somebody else's clone in it, so the miss is a miss on the
    // key rather than on an empty directory.
    let other = Repo {
        source: repo.source.clone(),
        owner: repo.owner.clone(),
        name: format!("ledger-{}", unique()),
        id: String::new(),
        url: String::new(),
    };
    let root = clones_root_with(&other);
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();

    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Nothing);
    assert_eq!(answer.path, None);
    assert_eq!(answer.tree, None, "no path, so nothing for git to read");
    assert_eq!(
        answer.clone_command.as_deref(),
        Some(format!("git clone {}", repo.url).as_str()),
        "the command is built from the repo's own URL and nothing else"
    );
    assert_eq!(
        answer.clones_root.as_deref(),
        Some(root.path().to_string_lossy().as_ref()),
        "*not found* and *no root set* are different states and the panel says which"
    );
}

/// With no clones root at all there is no scan, and the answer is the same
/// honest *no checkout* -- not an error, and not a walk of the process's
/// working directory.
#[tokio::test]
async fn no_clones_root_means_no_scan() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    set_clones_root(&pool, None).await.unwrap();

    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Nothing);
    assert_eq!(answer.clones_root, None);
    assert_eq!(
        answer.clone_command.as_deref(),
        Some(format!("git clone {}", repo.url).as_str())
    );
}

/// An override still answers with no clones root set: it is a path a person
/// gave, and it does not depend on the scan having anywhere to look.
#[tokio::test]
async fn an_override_answers_without_a_clones_root() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    set_clones_root(&pool, None).await.unwrap();
    set_override(&pool, &repo.id, Some("/Users/mara/work/payout-service"))
        .await
        .unwrap();

    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Override);
    assert_eq!(
        answer.path.as_deref(),
        Some("/Users/mara/work/payout-service")
    );
}

/// The view carries where the checkout's working tree stands, read by git at
/// the path the view resolved (#704, ADR-0021) -- here a real repository made
/// by git, set as the override, on a branch of its own.
#[tokio::test]
async fn the_view_says_where_the_working_tree_stands() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let clone = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "feature/sepa-retry"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "first",
        ][..],
    ] {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(clone.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
    fs::write(clone.path().join("draft.txt"), "x").unwrap();
    set_override(&pool, &repo.id, Some(&clone.path().to_string_lossy()))
        .await
        .unwrap();

    let answer = view(&pool, &repo.id).await.unwrap();
    match answer.tree {
        Some(TreeRead::Read(state)) => {
            assert_eq!(state.branch.as_deref(), Some("feature/sepa-retry"));
            assert_eq!(state.untracked, 1);
        }
        other => panic!("expected the working tree read, got {other:?}"),
    }

    // A path that is no repository is a state the panel says, not a refusal.
    let empty = tempfile::tempdir().unwrap();
    set_override(&pool, &repo.id, Some(&empty.path().to_string_lossy()))
        .await
        .unwrap();
    let answer = view(&pool, &repo.id).await.unwrap();
    assert_eq!(answer.tree, Some(TreeRead::NotARepo));
}

/// The uncommitted diff is read at the path the checkout resolved (#824,
/// ADR-0026), for a repo and for a branch of it; an entity with no checkout
/// is refused as `open_checkout` refuses it, and a ticket as the view does.
#[tokio::test]
async fn the_local_changes_are_read_at_the_resolved_checkout() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let branch = branch_of(&pool, &repo, "main").await;

    let none = local_changes(&pool, &repo.id)
        .await
        .expect_err("no checkout yet");
    assert_eq!(none.code, IpcErrorCode::NotFound);

    let clone = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "a.txt"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.invalid",
            "commit",
            "-q",
            "-m",
            "first",
        ][..],
    ] {
        if args[0] == "add" {
            fs::write(clone.path().join("a.txt"), "one\n").unwrap();
        }
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(clone.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
    fs::write(clone.path().join("a.txt"), "two\n").unwrap();
    fs::write(clone.path().join("draft.txt"), "x").unwrap();
    set_override(&pool, &repo.id, Some(&clone.path().to_string_lossy()))
        .await
        .unwrap();

    for entity in [&repo.id, &branch] {
        match local_changes(&pool, entity).await.unwrap() {
            DiffRead::Read(diff) => {
                assert_eq!(diff.unstaged.files.len(), 1, "{entity}");
                assert_eq!(diff.unstaged.files[0].path, "a.txt");
                assert_eq!(diff.untracked, ["draft.txt"]);
            }
            other => panic!("expected the diff for {entity}, got {other:?}"),
        }
    }

    let ticket = item(
        &pool,
        &format!("jira-{}", unique()),
        "ticket",
        "PAY-232",
        None,
    )
    .await;
    let wrong = local_changes(&pool, &ticket)
        .await
        .expect_err("a ticket has no clone");
    assert_eq!(wrong.code, IpcErrorCode::Invalid);
}

/// The three refusals, each with the code the panel branches on.
#[tokio::test]
async fn the_refusals_carry_the_codes_the_panel_branches_on() {
    let pool = pool().await;

    let bad = view(&pool, "not-an-entity-id").await.expect_err("no ':'");
    assert_eq!(bad.code, IpcErrorCode::Invalid);

    let absent = format!("gitea:nobody/nothing-{}", unique());
    let missing = view(&pool, &absent).await.expect_err("not mirrored");
    assert_eq!(missing.code, IpcErrorCode::NotFound);
    // The word is the glossary's: `CONTEXT.md`'s **Mirror** entry lists
    // *index* under `_Avoid_` (#518).
    assert_eq!(
        missing.message,
        format!("{absent} is not in the mirror"),
        "the refusal names the id and the mirror"
    );

    // A checkout belongs to a repo or a branch. A ticket has no clone, and
    // saying so by name is better than answering an empty view that a panel
    // would draw as *no checkout* on a surface that should never ask.
    let ticket = item(
        &pool,
        &format!("jira-{}", unique()),
        "ticket",
        "PAY-231",
        None,
    )
    .await;
    let wrong = view(&pool, &ticket)
        .await
        .expect_err("a ticket has no clone");
    assert_eq!(wrong.code, IpcErrorCode::Invalid);
    assert!(wrong.message.contains("ticket"), "{}", wrong.message);

    let wrong = set_override(&pool, &ticket, Some("/tmp/x"))
        .await
        .expect_err("and it cannot be given one either");
    assert_eq!(wrong.code, IpcErrorCode::Invalid);
}

/// A branch whose repository is not mirrored is *no checkout*, not an error --
/// a repo outside the configured allowlist that a link points into. There is
/// nothing to key an override on, and asking to set one says so.
#[tokio::test]
async fn a_branch_with_no_repository_in_the_mirror_is_no_checkout() {
    let pool = pool().await;
    let token = unique();
    let source = format!("gitea-{token}");
    let branch = item(
        &pool,
        &source,
        "branch",
        &format!("tidewater/orphan-{token}@refs/heads/main"),
        Some("https://gitea.example.com/tidewater/orphan/src/branch/main"),
    )
    .await;

    let answer = view(&pool, &branch).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Nothing);
    assert_eq!(answer.repo_entity_id, None);
    assert_eq!(
        answer.clone_command, None,
        "there is no repo URL to build one from, and inventing one from the branch's would be a guess"
    );

    let refused = set_override(&pool, &branch, Some("/tmp/x"))
        .await
        .expect_err("nothing to key the override on");
    assert_eq!(refused.code, IpcErrorCode::NotFound);
    // Named surfaces, in the glossary's word (#518): the branch, its missing
    // repository, and the mirror rather than an *index*.
    assert_eq!(
        refused.message,
        format!(
            "{branch} is a branch whose repository is not in the mirror, \
             so there is nothing to set a checkout on"
        ),
        "the refusal names the branch, the missing repository and the mirror"
    );
}

/// A branch resolves to the **longest** repo id its own id starts with.
///
/// One source may hold `tidewater/payout` and `tidewater/payout-service`, and
/// the first is a strict prefix of the second — so a branch of
/// `payout-service` starts with *both* repo ids, and the shorter one is a
/// different repository with a different clone. Nothing else in this file
/// can see it: every other test gives its source one repo, and the ordering
/// clause is invisible while that is true.
#[tokio::test]
async fn a_branch_resolves_to_the_longest_repo_id_it_starts_with() {
    let pool = pool().await;
    let token = unique();
    let source = format!("gitea-{token}");
    let owner = "tidewater";
    let short = format!("payout-{token}");
    let long = format!("{short}-service");

    let short_id = item(
        &pool,
        &source,
        "repo",
        &format!("{owner}/{short}"),
        Some(&format!("https://{HOST}/{owner}/{short}")),
    )
    .await;
    let long_id = item(
        &pool,
        &source,
        "repo",
        &format!("{owner}/{long}"),
        Some(&format!("https://{HOST}/{owner}/{long}")),
    )
    .await;
    let branch = item(
        &pool,
        &source,
        "branch",
        &format!("{owner}/{long}@refs/heads/main"),
        None,
    )
    .await;
    assert!(
        long_id.starts_with(&short_id),
        "the shorter repo id has to be a strict prefix or this test proves nothing: \
         {short_id} / {long_id}"
    );

    let answer = view(&pool, &branch).await.unwrap();
    assert_eq!(
        answer.repo_entity_id.as_deref(),
        Some(long_id.as_str()),
        "a branch of {long} belongs to {long} and not to {short}"
    );
    assert_eq!(
        answer.repo_url.as_deref(),
        Some(format!("https://{HOST}/{owner}/{long}").as_str())
    );
}

/// An entity id is a source's string, not a pattern.
///
/// A Gitea repository may be called `payout_svc`, and `_` is SQL's
/// single-character wildcard: written as `$2 like entity_id || '%'`, the
/// repository lookup would match a branch of `payoutXsvc` -- a *different*
/// repository, with a different clone on the disk -- and the panel would show
/// somebody the wrong working tree. `starts_with` has no pattern in it, and
/// this is the only test in the file that can tell the two apart, because
/// every other fixture's ids are wildcard-free.
#[tokio::test]
async fn an_underscore_in_a_repo_name_is_not_a_wildcard() {
    let pool = pool().await;
    let token = unique();
    let source = format!("gitea-{token}");
    let owner = "tidewater";
    let underscored = format!("payout_svc{token}");

    item(
        &pool,
        &source,
        "repo",
        &format!("{owner}/{underscored}"),
        Some(&format!("https://{HOST}/{owner}/{underscored}")),
    )
    .await;

    // A branch of a repository the mirror does not hold, whose key differs
    // from the one above in exactly the character `_` would match.
    let stranger = underscored.replacen('_', "X", 1);
    let branch = item(
        &pool,
        &source,
        "branch",
        &format!("{owner}/{stranger}@refs/heads/main"),
        None,
    )
    .await;

    let answer = view(&pool, &branch).await.unwrap();
    assert_eq!(
        answer.repo_entity_id, None,
        "{stranger} is not {underscored}, and matching them would point an editor at the wrong tree"
    );
    assert_eq!(answer.found_by, FoundBy::Nothing);
}

/// A repo the adapter reported no page for has no clone command, and the scan
/// has nothing to match on -- the P5 miss, met the same way *Open in browser*
/// meets it: by being absent rather than by inventing a URL.
#[tokio::test]
async fn a_repo_with_no_url_has_no_clone_command_and_is_not_scanned_for() {
    let pool = pool().await;
    let token = unique();
    let id = item(
        &pool,
        &format!("gitea-{token}"),
        "repo",
        &format!("tidewater/urlless-{token}"),
        None,
    )
    .await;
    let root = tempfile::tempdir().unwrap();
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();

    let answer = view(&pool, &id).await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Nothing);
    assert_eq!(answer.repo_url, None);
    assert_eq!(answer.clone_command, None);
    // The override is still available: a repository knobas has no URL for is
    // exactly the case a person answers by hand.
    set_override(&pool, &id, Some("/Users/mara/work/urlless"))
        .await
        .unwrap();
    assert_eq!(view(&pool, &id).await.unwrap().found_by, FoundBy::Override);
}

/// Deleting the repo takes its override with it, which is `0024`'s cascade.
///
/// The point is not the foreign key for its own sake: an override is an answer
/// *about* a repository, and a row that outlived one would be handed to the
/// next repository that happened to be given the same id.
#[tokio::test]
async fn purging_the_repo_takes_its_override_with_it() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    set_override(&pool, &repo.id, Some("/Users/mara/work/payout-service"))
        .await
        .unwrap();

    sqlx::query("delete from knobas.entity where id = $1")
        .bind(&repo.id)
        .execute(&pool)
        .await
        .unwrap();

    let left: i64 =
        sqlx::query_scalar("select count(*) from knobas.checkout_override where entity_id = $1")
            .bind(&repo.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

/// **A withdrawn repo and a turned-off source still answer their checkout** --
/// the reason `repo_of` reads `sync.item` rather than `sync.live_item`.
///
/// The panel is mounted inside the detail, and `get_entity`'s `DETAIL` is
/// exempt from both of the view's halves (`CONTEXT.md`, **Live item**, reader
/// 2) precisely so a withdrawn or turned-off entity's detail opens and says so.
/// A checkout read that went through the view would draw *not in the mirror* on
/// a page the app can open -- and the clone is still on the disk either way,
/// which is the whole point of knowing where it is.
#[tokio::test]
async fn a_withdrawn_repo_and_a_turned_off_source_still_answer_their_checkout() {
    let pool = pool().await;

    let withdrawn = repo(&pool).await;
    set_override(&pool, &withdrawn.id, Some("/Users/mara/src/withdrawn"))
        .await
        .unwrap();
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&withdrawn.id)
        .execute(&pool)
        .await
        .unwrap();
    let answer = view(&pool, &withdrawn.id).await.unwrap();
    assert_eq!(
        answer.found_by,
        FoundBy::Override,
        "a tombstoned repo keeps its entity row, its override and its clone"
    );
    assert_eq!(answer.path.as_deref(), Some("/Users/mara/src/withdrawn"));

    let turned_off = repo(&pool).await;
    let root = clones_root_with(&turned_off);
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, config, sync_interval_secs, enabled)
         values ($1,'gitea','Turned off','http://gitea','pat','{}'::jsonb, 60, false)",
    )
    .bind(&turned_off.source)
    .execute(&pool)
    .await
    .unwrap();
    let answer = view(&pool, &turned_off.id).await.unwrap();
    assert_eq!(
        answer.found_by,
        FoundBy::Scan,
        "turning a source off hides its items from readers, not the clone from its owner"
    );
    assert_eq!(path_of(&answer), root.path().join(&turned_off.name));
}

// -- the spawn half (#501) ---------------------------------------------------
//
// The template rule itself is `knobas_core::checkout`'s and is asserted there
// over every shape a person can type. What is asserted here is the join: that
// the value reaching a real process is the checkout this database resolved,
// and that each refusal names the thing a person has to go and change.

/// An executable that records the arguments it was given, and the file it
/// records them in.
///
/// A stub rather than a real editor for the reason ADR-0013 gives a fake in
/// general: the thing under test is *what knobas passed*, and only a program
/// that writes its `argv` down can answer that. What the editor then does with
/// a directory is the editor's business and no assertion of knobas'.
///
/// One argument per line: the whole point is the count as well as the values,
/// and a space-joined line could not tell one argument holding a space from
/// two arguments.
///
/// Written beside the record and renamed onto it, so the record appears whole
/// or not at all: `printf` may write each line separately, and a reader that
/// saw the first line alone once failed the gate (#583). A rename is atomic;
/// a trailing newline cannot tell a finished record from a half-written one,
/// because every line ends in one. The desktop witness's `stub_script` writes
/// its record the same way, and pins this one to it.
#[cfg(unix)]
fn recording_stub(dir: &Path) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let record = dir.join("record");
    let stub = dir.join("stub");
    fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.tmp'\nmv '{0}.tmp' '{0}'\n",
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    (stub, record)
}

/// What the stub recorded, waiting for it to have run.
///
/// A deadline rather than a sleep: `open_checkout` returns as soon as the
/// program has *started*, which is the only thing an IPC call can promise
/// about a process it does not wait for, so a fixed sleep would be either slow
/// or flaky depending on the machine. This is the same shape the desktop
/// driver polls in.
#[cfg(unix)]
async fn recorded(record: &Path) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        // The stub renames a finished record into place, so a record that
        // exists is a whole one (#583). The newline is only what a whole
        // record looks like, the shell twin's `record_is_complete` (#531).
        if let Ok(text) = fs::read_to_string(record)
            && text.ends_with('\n')
        {
            return text.lines().map(str::to_owned).collect();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("the stub at {} never recorded anything", record.display());
}

fn app(id: Option<&str>, name: &str, command: &str, file: Option<&str>) -> Draft {
    Draft {
        id: id.map(str::to_owned),
        name: name.to_owned(),
        command: command.to_owned(),
        file_command: file.map(str::to_owned),
    }
}

/// A repo with its clone found by the scan, and a stub to record what runs.
#[cfg(unix)]
async fn scanned(pool: &PgPool) -> (Repo, tempfile::TempDir, PathBuf, PathBuf, tempfile::TempDir) {
    let repo = repo(pool).await;
    let root = clones_root_with(&repo);
    set_clones_root(pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();
    let workshop = tempfile::tempdir().unwrap();
    let (stub, record) = recording_stub(workshop.path());
    (repo, root, stub, record, workshop)
}

/// The whole feature, end to end: a clones root, a scan, an application, and
/// a real process that is handed the path the scan found -- and nothing else.
/// Named by its id, and as the default.
#[cfg(unix)]
#[tokio::test]
async fn opening_a_checkout_runs_the_application_with_the_scanned_path() {
    let pool = pool().await;
    let (repo, root, stub, record, _workshop) = scanned(&pool).await;
    set_applications(
        &pool,
        &[app(
            Some("vscode"),
            "VS Code",
            &format!("{} --wait {{path}}", stub.display()),
            None,
        )],
        None,
    )
    .await
    .unwrap();
    let checkout = root.path().join(&repo.name).to_string_lossy().into_owned();
    let expected = ["--wait".to_owned(), checkout.clone()];

    let argv = open(&pool, &repo.id, Some("vscode"), None, None)
        .await
        .unwrap();
    // Exactly two arguments: the flag the command carried, and the checkout.
    // The repo's URL, its key, its title and its source are all in scope at
    // the call site and **none of them** reaches the process (ADR-0016).
    assert_eq!(recorded(&record).await, expected);
    assert_eq!(argv[1..], expected, "the answer is the argv it started");

    // No application named is the default, which a list's first entry is.
    fs::remove_file(&record).unwrap();
    open(&pool, &repo.id, None, None, None).await.unwrap();
    assert_eq!(recorded(&record).await, expected);
}

/// The override is a path, and the path is what runs. It holds a space, and
/// there is no shell between here and `exec`: it arrives as **one** argument.
#[cfg(unix)]
#[tokio::test]
async fn an_override_with_a_space_is_one_argument() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let workshop = tempfile::tempdir().unwrap();
    let (stub, record) = recording_stub(workshop.path());
    let worktree = workshop.path().join("my work/pay; rm -rf ~");
    set_override(&pool, &repo.id, Some(&worktree.to_string_lossy()))
        .await
        .unwrap();
    set_applications(
        &pool,
        &[app(
            None,
            "Viewer",
            &format!("{} {{path}} {{name}}", stub.display()),
            None,
        )],
        None,
    )
    .await
    .unwrap();

    open(&pool, &repo.id, Some("viewer"), None, None)
        .await
        .unwrap();

    assert_eq!(
        recorded(&record).await,
        [
            worktree.to_string_lossy().into_owned(),
            // `{name}` is the checkout folder's own name, not the repo's.
            "pay; rm -rf ~".to_owned()
        ]
    );
}

/// A branch opens its repository's checkout: nothing checks anything out, so
/// what opens is the working tree as the person left it (ADR-0016).
#[cfg(unix)]
#[tokio::test]
async fn a_branch_opens_its_repositorys_checkout() {
    let pool = pool().await;
    let (repo, root, stub, record, _workshop) = scanned(&pool).await;
    let branch = branch_of(&pool, &repo, "feature/PAY-231-sepa-retry").await;
    set_applications(
        &pool,
        &[app(
            Some("vscode"),
            "VS Code",
            &format!("{} {{path}}", stub.display()),
            None,
        )],
        None,
    )
    .await
    .unwrap();

    open(&pool, &branch, Some("vscode"), None, None)
        .await
        .unwrap();

    assert_eq!(
        recorded(&record).await,
        [root.path().join(&repo.name).to_string_lossy().into_owned()]
    );
}

/// With a file, the file command runs, filled with the file inside the
/// checkout and the line; without a file command, the application's command
/// runs at the checkout.
#[cfg(unix)]
#[tokio::test]
async fn a_file_opens_with_the_file_command_and_its_line() {
    let pool = pool().await;
    let (repo, root, stub, record, _workshop) = scanned(&pool).await;
    let checkout = root.path().join(&repo.name);
    fs::create_dir_all(checkout.join("src/sepa")).unwrap();
    fs::write(checkout.join("src/sepa/retry.rs"), "fn retry() {}\n").unwrap();
    set_applications(
        &pool,
        &[
            app(
                None,
                "Editor",
                &format!("{} {{path}}", stub.display()),
                Some(&format!("{} --goto {{file}}:{{line}}", stub.display())),
            ),
            app(None, "Plain", &format!("{} {{path}}", stub.display()), None),
        ],
        None,
    )
    .await
    .unwrap();

    open(&pool, &repo.id, None, Some("src/sepa/retry.rs"), Some(42))
        .await
        .unwrap();
    assert_eq!(
        recorded(&record).await,
        [
            "--goto".to_owned(),
            format!("{}:42", checkout.join("src/sepa/retry.rs").display())
        ]
    );

    fs::remove_file(&record).unwrap();
    open(
        &pool,
        &repo.id,
        Some("plain"),
        Some("src/sepa/retry.rs"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        recorded(&record).await,
        [checkout.to_string_lossy().into_owned()]
    );
}

/// A file is opened only when it is a file inside the checkout **on disk**:
/// by its words (`..`, a root) and by where it really is (a symlink out of
/// the checkout), and one that is not there. Nothing runs for any of them.
#[cfg(unix)]
#[tokio::test]
async fn a_file_outside_the_checkout_is_refused_and_nothing_runs() {
    let pool = pool().await;
    let (repo, root, stub, record, workshop) = scanned(&pool).await;
    let checkout = root.path().join(&repo.name);
    fs::write(workshop.path().join("secret"), "x").unwrap();
    std::os::unix::fs::symlink(workshop.path().join("secret"), checkout.join("link")).unwrap();
    set_applications(
        &pool,
        &[app(
            None,
            "Editor",
            &format!("{} {{path}}", stub.display()),
            Some(&format!("{} {{file}}", stub.display())),
        )],
        None,
    )
    .await
    .unwrap();

    for file in ["../secret", "/etc/hosts", "link", "missing.rs", ".git"] {
        let error = open(&pool, &repo.id, None, Some(file), None)
            .await
            .expect_err(file);
        assert_eq!(error.code, IpcErrorCode::Invalid, "{file}");
        assert!(
            error.message.contains("not a file inside"),
            "{}",
            error.message
        );
    }
    // `open` starts its program before it answers and does not wait for it,
    // so a program started for a refused file is running now or has
    // recorded and exited. Read in that order, the process table and then
    // the record leave it nowhere to hide however slowly it runs (#1927).
    // Before, a 200 ms look at the record stood for this.
    let started = running(&stub);
    assert!(started.is_empty(), "a refused file started {started:?}");
    assert!(!record.exists(), "nothing may have run");
}

/// The processes running `stub`, by `ps`'s command column. A program `open`
/// started is listed from the moment `open` returned, its `exec` done, until
/// it exits, and the stub has written its record before it exits.
#[cfg(unix)]
fn running(stub: &Path) -> Vec<String> {
    let listed = std::process::Command::new("ps")
        .args(["-axww", "-o", "command="])
        .output()
        .expect("ps lists the processes");
    assert!(listed.status.success(), "ps failed: {listed:?}");
    let stub = stub.to_string_lossy();
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|command| command.contains(stub.as_ref()))
        .map(str::to_owned)
        .collect()
}

/// A repo with no clone on this machine refuses, and the refusal names the
/// entity somebody pressed the button on.
#[tokio::test]
async fn a_repo_with_no_checkout_refuses_and_names_it() {
    let pool = pool().await;
    let repo = repo(&pool).await;
    let error = open(&pool, &repo.id, None, None, None)
        .await
        .expect_err("there is nothing to open");
    assert_eq!(error.code, IpcErrorCode::NotFound);
    assert!(error.message.contains(&repo.id), "{}", error.message);
    assert!(error.message.contains("no checkout"), "{}", error.message);
}

/// A list with an entry that cannot be run is refused where it is typed,
/// naming the entry and the placeholder, and stores nothing.
#[tokio::test]
async fn a_command_that_cannot_be_run_is_refused_where_it_is_typed() {
    let pool = pool().await;
    let before = settings(&pool).await.unwrap();

    // The refusal ADR-0016 is about: a placeholder that would carry a field
    // the mirror wrote.
    let error = set_applications(
        &pool,
        &[app(None, "Viewer", "viewer {repo_url}", None)],
        None,
    )
    .await
    .expect_err("only the four placeholders fill");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(error.message.contains("{repo_url}"), "{}", error.message);
    assert!(error.message.contains("Viewer"), "{}", error.message);

    let error = set_applications(&pool, &[app(None, "  ", "viewer {path}", None)], None)
        .await
        .expect_err("an application has a name");
    assert_eq!(error.code, IpcErrorCode::Invalid);

    assert_eq!(
        settings(&pool).await.unwrap(),
        before,
        "a refusal stores nothing"
    );
}

/// A saved command whose program is not on this machine refuses, and the
/// refusal names the command -- the string somebody has to go and change.
#[cfg(unix)]
#[tokio::test]
async fn a_program_that_will_not_start_names_the_command() {
    let pool = pool().await;
    let (repo, _root, _stub, _record, _workshop) = scanned(&pool).await;
    let command = "/nonexistent/knobas-not-an-editor {path}";
    set_applications(&pool, &[app(None, "Gone", command, None)], None)
        .await
        .unwrap();
    let error = open(&pool, &repo.id, Some("gone"), None, None)
        .await
        .expect_err("there is no such program");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(error.message.contains(command), "{}", error.message);
}

/// *Try it* (#1643): the dialog saves the draft and opens it by its id, so the
/// program is the one `knobas.setting` holds at the moment of the open, and
/// nothing the webview sends names one (ADR-0016). Saved again with another
/// flag, the next open runs the new template.
#[cfg(unix)]
#[tokio::test]
async fn trying_a_command_runs_the_template_the_setting_holds() {
    let pool = pool().await;
    let (repo, root, stub, record, _workshop) = scanned(&pool).await;
    let checkout = root.path().join(&repo.name).to_string_lossy().into_owned();
    let draft = |flag: &str| {
        app(
            None,
            "MyGitViewer",
            &format!("{} {flag} {{path}}", stub.display()),
            None,
        )
    };

    set_applications(&pool, &[draft("--try")], None)
        .await
        .unwrap();
    let id = settings(&pool).await.unwrap().applications[0].id.clone();
    open(&pool, &repo.id, Some(&id), None, None).await.unwrap();
    assert_eq!(
        recorded(&record).await,
        ["--try".to_owned(), checkout.clone()]
    );

    fs::remove_file(&record).unwrap();
    set_applications(&pool, &[draft("--again")], None)
        .await
        .unwrap();
    open(&pool, &repo.id, Some(&id), None, None).await.unwrap();
    assert_eq!(recorded(&record).await, ["--again".to_owned(), checkout]);
}

/// Until a list is saved, the list is this platform's shipped one with #501's
/// stored templates over it, VS Code the default. Saving keeps the order and
/// the default given; a default whose entry is gone is the first entry.
#[tokio::test]
async fn the_shipped_list_with_501s_templates_is_the_list_until_one_is_saved() {
    let pool = pool().await;
    sqlx::query("insert into knobas.setting (key, value) values ($1, to_jsonb($2::text))")
        .bind(legacy_key("terminal"))
        .bind("my-term {path}")
        .execute(&pool)
        .await
        .unwrap();

    let first = settings(&pool).await.unwrap();
    let mut expected = shipped();
    match expected.iter_mut().find(|a| a.id == "terminal") {
        Some(terminal) => terminal.command = "my-term {path}".to_owned(),
        // Off macOS nothing ships, and #501's template is an entry of its own.
        None => expected.push(knobas_core::checkout::open::Application {
            id: "terminal".to_owned(),
            name: "Terminal".to_owned(),
            command: "my-term {path}".to_owned(),
            file_command: None,
        }),
    }
    assert_eq!(first.applications, expected);
    assert_eq!(
        first.default_application.as_deref(),
        Some(if shipped().is_empty() {
            "terminal"
        } else {
            "vscode"
        })
    );
    assert!(
        matches!(first.git, GitFound::Found { .. }),
        "{:?}",
        first.git
    );

    set_applications(
        &pool,
        &[
            app(Some("terminal"), "Terminal", "my-term {path}", None),
            app(None, "MyGitViewer", "MyGitViewer {path}", None),
        ],
        Some("mygitviewer"),
    )
    .await
    .unwrap();
    let saved = settings(&pool).await.unwrap();
    let ids: Vec<&str> = saved.applications.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["terminal", "mygitviewer"]);
    assert_eq!(saved.default_application.as_deref(), Some("mygitviewer"));
    let row: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(APPLICATIONS_KEY)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(
        row.is_some(),
        "the list is one row under {APPLICATIONS_KEY}"
    );

    set_applications(
        &pool,
        &[app(Some("terminal"), "Terminal", "t {path}", None)],
        Some("gone"),
    )
    .await
    .unwrap();
    assert_eq!(
        settings(&pool)
            .await
            .unwrap()
            .default_application
            .as_deref(),
        Some("terminal")
    );
}

/// *Where should clones go?* offers `~/code`; nothing knobas runs expands a
/// tilde, so the root is stored with the home directory spelled out (#846).
#[tokio::test]
async fn a_clones_root_typed_with_a_tilde_is_stored_under_home() {
    let pool = pool().await;
    let home = std::env::var("HOME").expect("the gate runs with a HOME");
    set_clones_root(&pool, Some("~/code")).await.unwrap();
    assert_eq!(
        clones_root(&pool).await.unwrap(),
        Some(format!("{}/code", home.trim_end_matches('/')))
    );
}

/// A root that is not absolute once `~` is home is refused where it is typed
/// (#1661): Clone refuses it on every click and the scan would read it against
/// the process's working directory. The stored root stays what it was.
#[tokio::test]
async fn a_relative_clones_root_is_refused_and_leaves_the_stored_one() {
    let pool = pool().await;
    let root = tempfile::tempdir().unwrap();
    let spelled = root.path().to_string_lossy().into_owned();
    set_clones_root(&pool, Some(&spelled)).await.unwrap();

    for typed in ["Code", "./src", "  ../up  ", "~user/code"] {
        let refused = set_clones_root(&pool, Some(typed)).await.unwrap_err();
        assert_eq!(refused.code, IpcErrorCode::Invalid, "{typed:?}");
        assert!(refused.message.contains("absolute"), "{}", refused.message);
    }
    assert_eq!(
        clones_root(&pool).await.unwrap().as_deref(),
        Some(spelled.as_str())
    );

    // An absolute path is still taken.
    set_clones_root(&pool, Some("/srv/clones")).await.unwrap();
    assert_eq!(
        clones_root(&pool).await.unwrap().as_deref(),
        Some("/srv/clones")
    );
}
