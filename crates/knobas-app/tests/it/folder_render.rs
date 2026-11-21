//! Folder renderers (ADR-0044) at the IPC seam: saved to a real
//! `knobas.setting`, asked for the way `folder_render` asks, over a real git
//! clone on this disk, run by a real program.
//!
//! What is witnessed: the folder is read from the clone's objects at the
//! commit (not its working folder), copied under its own name and listed by
//! the program from an empty folder of its own; the same folder at a later
//! commit that changed nothing in it is drawn at once, one that changed it is
//! run again; the tree's marks name the folders holding the project file; a
//! mirrored name that is no plain path here is refused before the program
//! runs; a repository with no clone and no listing says so.

use std::path::Path;
use std::time::{Duration, Instant};

use knobas_app::checkout::set_override;
use knobas_app::folder_render::runs::{Ask, FolderFailure, FolderRenderView, ReadFrom, Runs};
use knobas_app::folder_render::{self, FolderOutput, FolderRendererDraft, resolve};
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=knobas test",
            "-c",
            "user.email=test@knobas.invalid",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A repository the mirror holds, its clone this directory.
async fn repo(pool: &PgPool, clone: &Path, token: &str) -> String {
    let id = format!("gitea-fr:tidewater/flows-{token}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'repo',$2)")
        .bind(&id)
        .bind(format!("tidewater/flows-{token}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1,'gitea-fr','repo',$2,'','{}'::jsonb,null)",
    )
    .bind(&id)
    .bind(format!("tidewater/flows-{token}"))
    .execute(pool)
    .await
    .unwrap();
    set_override(pool, &id, Some(&clone.to_string_lossy()))
        .await
        .unwrap();
    id
}

async fn settled(
    pool: &PgPool,
    id: &str,
    renderer: &str,
    folder: &str,
    commit: &str,
) -> FolderRenderView {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let view = folder_render::ask(
            pool,
            None,
            id,
            renderer,
            folder,
            Some(commit),
            None,
            Ask::Look,
            "",
        )
        .await
        .unwrap();
        if view.running.is_none() || Instant::now() > deadline {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn text(view: &FolderRenderView) -> String {
    let drawn = view.drawn.as_ref().expect("a drawing");
    String::from_utf8(
        Runs::global()
            .output(&drawn.token)
            .expect("its bytes")
            .to_vec(),
    )
    .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn a_folder_is_copied_from_the_clone_at_the_commit_and_drawn_by_the_program() {
    let pool = scratch("folder_render_clone").await;
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q", "-b", "main"]);
    write(clone.path(), "README.md", "# flows\n");
    write(
        clone.path(),
        "flows/orders/orders.lcproj",
        "{\"name\": \"orders\"}\n",
    );
    write(
        clone.path(),
        "flows/orders/steps/main.flow",
        "start -> end\n",
    );
    write(clone.path(), "flows/other/notes.txt", "n\n");
    let first = commit(clone.path(), "first");
    // The working folder differs from the commit: the drawing is the commit's.
    write(
        clone.path(),
        "flows/orders/steps/main.flow",
        "uncommitted\n",
    );

    let tools = tempfile::tempdir().unwrap();
    let script = tools.path().join("list.sh");
    std::fs::write(
        &script,
        "empty=$(ls -A | wc -l | tr -d ' '); cd \"$1\" && echo \"$(basename \"$1\") cwd=$empty\" && find . -type f | sort && cat steps/main.flow\n",
    )
    .unwrap();
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Lister".to_owned(),
            command: format!("sh {} {{path}}", script.display()),
            holds: vec!["*.lcproj".to_owned()],
            folders: vec![],
            output: FolderOutput::Detect,
            timeout_secs: Some(20),
        }],
    )
    .await
    .unwrap();
    let lister = saved[0].id.clone();
    let id = repo(&pool, clone.path(), &format!("{}", std::process::id())).await;

    // The tree's marks: the folder holding the project file, and no other.
    let marks = resolve::marks(&pool, None, &id, Some(&first), None)
        .await
        .unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].folders, ["flows/orders"]);

    let opened = folder_render::ask(
        &pool,
        None,
        &id,
        &lister,
        "flows/orders",
        Some(&first),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    assert!(opened.running.is_some(), "{opened:?}");
    let view = settled(&pool, &id, &lister, "flows/orders", &first).await;
    assert!(view.current, "{view:?}");
    assert_eq!(
        text(&view),
        "orders cwd=0\n./orders.lcproj\n./steps/main.flow\nstart -> end\n"
    );

    // A later commit that changed nothing in the folder: drawn at once.
    write(clone.path(), "flows/other/notes.txt", "more\n");
    git(clone.path(), &["checkout", "-q", "--", "flows/orders"]);
    let second = commit(clone.path(), "outside");
    let same = folder_render::ask(
        &pool,
        None,
        &id,
        &lister,
        "flows/orders",
        Some(&second),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    assert!(same.current && same.running.is_none(), "{same:?}");

    // One that changed it: the last drawing is shown, not current, while it runs.
    write(
        clone.path(),
        "flows/orders/steps/main.flow",
        "start -> pay -> end\n",
    );
    let third = commit(clone.path(), "inside");
    let changed = folder_render::ask(
        &pool,
        None,
        &id,
        &lister,
        "flows/orders",
        Some(&third),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    assert!(
        !changed.current && changed.drawn.is_some() && changed.running.is_some(),
        "{changed:?}"
    );
    let redrawn = settled(&pool, &id, &lister, "flows/orders", &third).await;
    assert!(
        text(&redrawn).ends_with("start -> pay -> end\n"),
        "{}",
        text(&redrawn)
    );
}

/// A mirrored name that is no plain path here (`a:b`, an NTFS stream on
/// Windows) is refused before the program runs, naming it.
#[cfg(unix)]
#[tokio::test]
async fn a_name_that_is_no_plain_path_is_refused_before_anything_runs() {
    let pool = scratch("folder_render_names").await;
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q", "-b", "main"]);
    write(clone.path(), "odd/ok.txt", "ok\n");
    write(clone.path(), "odd/a:b.txt", "stream\n");
    let sha = commit(clone.path(), "odd");
    let marker = clone.path().join("ran");
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Toucher".to_owned(),
            command: format!("touch {} {{path}}", marker.display()),
            holds: vec![],
            folders: vec![],
            output: FolderOutput::Text,
            timeout_secs: None,
        }],
    )
    .await
    .unwrap();
    let id = repo(
        &pool,
        clone.path(),
        &format!("names-{}", std::process::id()),
    )
    .await;
    folder_render::ask(
        &pool,
        None,
        &id,
        &saved[0].id,
        "odd",
        Some(&sha),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    let view = settled(&pool, &id, &saved[0].id, "odd", &sha).await;
    match view.failure {
        Some(FolderFailure::Copy { reason }) => assert!(reason.contains("a:b.txt"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(!marker.exists(), "the program ran over a refused copy");
}

/// No clone and no listing from the source: said, and nothing runs.
#[tokio::test]
async fn a_repository_with_no_clone_and_no_listing_says_so() {
    let pool = scratch("folder_render_no_clone").await;
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Lister".to_owned(),
            command: "ls {path}".to_owned(),
            holds: vec![],
            folders: vec![],
            output: FolderOutput::Text,
            timeout_secs: None,
        }],
    )
    .await
    .unwrap();
    let somewhere = tempfile::tempdir().unwrap();
    let id = &repo(&pool, somewhere.path(), "not-cloned").await;
    // Its clone is gone: the override names a folder that is no longer there.
    set_override(&pool, id, None).await.unwrap();
    let view = folder_render::ask(
        &pool,
        None,
        id,
        &saved[0].id,
        "x",
        Some(&"c".repeat(40)),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    assert!(view.running.is_none());
    match view.failure {
        Some(FolderFailure::Copy { reason }) => {
            assert!(reason.contains("no clone here"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    let unknown = folder_render::ask(&pool, None, id, "gone", "x", None, None, Ask::Open, "")
        .await
        .unwrap_err();
    assert!(
        unknown
            .message
            .contains("'gone' is not one of your folder renderers"),
        "{unknown:?}"
    );
}

/// A folder renderer that makes a folder of files (ADR-0046) over a real
/// clone: what it wrote into `{outdir}` is kept in the store, its tree and a
/// file read back by the token, a link it wrote left out; a tool that then
/// fails keeps nothing and the kept folder is still the answer.
#[cfg(unix)]
#[tokio::test]
async fn a_folder_of_files_is_kept_and_read_back_by_its_token() {
    let pool = scratch("folder_render_made").await;
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q", "-b", "main"]);
    write(clone.path(), "flows/orders/orders.lcproj", "good\n");
    let first = commit(clone.path(), "first");
    let store = tempfile::tempdir().unwrap();
    Runs::global().set_store(store.path().join("knobas-rendered"));

    let tools = tempfile::tempdir().unwrap();
    let script = tools.path().join("site.sh");
    std::fs::write(
        &script,
        "[ \"$(cat \"$1/orders.lcproj\")\" = good ] || { echo bad project >&2; printf x > \"$2/half.md\"; exit 4; }\n\
         mkdir -p \"$2/img\" && echo '# Orders' > \"$2/index.md\" && printf a > \"$2/img/a.txt\" && ln -s /etc/hosts \"$2/hosts\"\n",
    )
    .unwrap();
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Site".to_owned(),
            command: format!("sh {} {{path}} {{outdir}}", script.display()),
            holds: vec!["*.lcproj".to_owned()],
            folders: vec![],
            output: FolderOutput::Detect,
            timeout_secs: Some(20),
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        saved[0].output,
        FolderOutput::Folder,
        "{{outdir}} sets the kind"
    );
    let site = saved[0].id.clone();
    let id = repo(&pool, clone.path(), &format!("made-{}", std::process::id())).await;

    folder_render::ask(
        &pool,
        None,
        &id,
        &site,
        "flows/orders",
        Some(&first),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    let view = settled(&pool, &id, &site, "flows/orders", &first).await;
    let token = view
        .drawn
        .as_ref()
        .expect("a generated folder")
        .token
        .clone();
    let tree = folder_render::made::tree(&token).unwrap();
    let paths: Vec<&str> = tree.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["img/a.txt", "index.md"]);
    assert_eq!(tree.left_out_total, 1);
    assert!(tree.dir.starts_with(&*store.path().to_string_lossy()));
    let block = folder_render::made::file(&token, "index.md").await.unwrap();
    let knobas_source::view::Block::File {
        text,
        language,
        commit: at,
        ..
    } = block
    else {
        panic!("a file block");
    };
    assert_eq!(text, "# Orders\n");
    assert_eq!(language.as_deref(), Some("markdown"));
    assert_eq!(at, None);
    assert_eq!(
        folder_render::made::file_bytes(&token, "img/a.txt")
            .await
            .unwrap(),
        b"a"
    );
    assert!(folder_render::made::file(&token, "hosts").await.is_err());
    assert!(Runs::global().output(&token).is_none());

    // The project changes and the tool fails: nothing of it is kept.
    write(clone.path(), "flows/orders/orders.lcproj", "broken\n");
    let second = commit(clone.path(), "second");
    folder_render::ask(
        &pool,
        None,
        &id,
        &site,
        "flows/orders",
        Some(&second),
        None,
        Ask::Open,
        "",
    )
    .await
    .unwrap();
    let failed = settled(&pool, &id, &site, "flows/orders", &second).await;
    assert!(
        matches!(
            failed.failure,
            Some(FolderFailure::Failed {
                status: Some(4),
                ..
            })
        ),
        "{failed:?}"
    );
    assert_eq!(failed.drawn.map(|d| d.token), Some(token.clone()));
    assert!(
        folder_render::made::tree(&token).is_ok(),
        "the kept folder stays"
    );
}

/// The Files tab on *Working tree* (ux29; Björn: "when I have the working tree selected, it tries to render the folder
/// from the head and not from my local changes"): asked at `~working-tree` in the working tree's lane, the folder is
/// the working folder as it is on disk, a changed and an untracked file included and an ignored one never; a further
/// change on disk is a new state (the last drawing shown, not current); and the commit's drawing, in the Files tab's
/// own lane, is neither served for it nor overwritten by it.
#[cfg(unix)]
#[tokio::test]
async fn the_working_tree_is_rendered_from_disk_in_its_own_lane_and_leaves_the_commit_drawing_alone()
 {
    const WT: &str = "~working-tree";
    const LANE: &str = "cmp:~working-tree";
    let pool = scratch("folder_render_working").await;
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q", "-b", "main"]);
    write(clone.path(), ".gitignore", "*.log\n");
    write(clone.path(), "flows/orders/orders.lcproj", "{}\n");
    write(clone.path(), "flows/orders/main.flow", "start -> end\n");
    let head = commit(clone.path(), "first");
    // Uncommitted: a changed file, an untracked one, an ignored one.
    write(
        clone.path(),
        "flows/orders/main.flow",
        "start -> refund -> end\n",
    );
    write(clone.path(), "flows/orders/extra.flow", "new\n");
    write(clone.path(), "flows/orders/debug.log", "ignored\n");

    let tools = tempfile::tempdir().unwrap();
    let script = tools.path().join("list.sh");
    std::fs::write(
        &script,
        "cd \"$1\" && find . -type f | sort && cat main.flow\n",
    )
    .unwrap();
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Lister".to_owned(),
            command: format!("sh {} {{path}}", script.display()),
            holds: vec![],
            folders: vec!["flows/*".to_owned()],
            output: FolderOutput::Text,
            timeout_secs: Some(20),
        }],
    )
    .await
    .unwrap();
    let lister = saved[0].id.clone();
    let id = repo(&pool, clone.path(), &format!("wt-{}", std::process::id())).await;
    let head = head.as_str();

    // The commit's drawing first, in the Files tab's lane: HEAD's file, nothing untracked.
    ask_at(&pool, &id, &lister, head, "", Ask::Open).await;
    let at_head = settled_at(&pool, &id, &lister, head, "").await;
    assert_eq!(
        text(&at_head),
        "./main.flow\n./orders.lcproj\nstart -> end\n"
    );

    // The working tree: drawn from disk, not served the commit's drawing.
    let opened = ask_at(&pool, &id, &lister, WT, LANE, Ask::Open).await;
    assert!(opened.running.is_some(), "{opened:?}");
    let on_disk = settled_at(&pool, &id, &lister, WT, LANE).await;
    assert!(on_disk.current, "{on_disk:?}");
    assert_eq!(on_disk.from, Some(ReadFrom::WorkingTree));
    assert_eq!(
        text(&on_disk),
        "./extra.flow\n./main.flow\n./orders.lcproj\nstart -> refund -> end\n"
    );
    assert_ne!(
        on_disk.drawn.as_ref().unwrap().token,
        at_head.drawn.as_ref().unwrap().token
    );

    // A further change on disk is a new state: the last drawing shown, not current, and a run starts on open.
    write(
        clone.path(),
        "flows/orders/main.flow",
        "start -> pay -> end\n",
    );
    let moved = ask_at(&pool, &id, &lister, WT, LANE, Ask::Open).await;
    assert!(!moved.current && moved.drawn.is_some(), "{moved:?}");
    let again = settled_at(&pool, &id, &lister, WT, LANE).await;
    assert!(
        text(&again).ends_with("start -> pay -> end\n"),
        "{}",
        text(&again)
    );

    // The commit's drawing is still the commit's, current, in its own lane.
    let back = ask_at(&pool, &id, &lister, head, "", Ask::Look).await;
    assert!(back.current, "{back:?}");
    assert_eq!(
        back.drawn.as_ref().unwrap().token,
        at_head.drawn.as_ref().unwrap().token
    );
    assert_eq!(text(&back), "./main.flow\n./orders.lcproj\nstart -> end\n");
}

async fn ask_at(
    pool: &PgPool,
    id: &str,
    renderer: &str,
    commit: &str,
    lane: &str,
    how: Ask,
) -> FolderRenderView {
    folder_render::ask(
        pool,
        None,
        id,
        renderer,
        "flows/orders",
        Some(commit),
        None,
        how,
        lane,
    )
    .await
    .unwrap()
}

async fn settled_at(
    pool: &PgPool,
    id: &str,
    renderer: &str,
    commit: &str,
    lane: &str,
) -> FolderRenderView {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let view = ask_at(pool, id, renderer, commit, lane, Ask::Look).await;
        if view.running.is_none() || Instant::now() > deadline {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
