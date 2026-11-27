//! *Try it on* (#2562) at the seam, with real programs: a folder renderer, a
//! file pipeline and a command tried on pasted text, a chosen file and a
//! chosen folder outside any repository.
//!
//! Witnessed: what a try draws is read back by its token as a repository's
//! render is; nothing a try does is written to `knobas.setting` or any other
//! row, to the folder renders' disk cache, to the generated-folders store or
//! to the list of runs; a command's throwaway file is gone when it ends.

#![cfg(unix)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knobas_app::file_pipeline::{PipeOutput, PipeStep};
use knobas_app::folder_render::runs::{Ask, FolderRenderView, Runs};
use knobas_app::folder_render::{self, FolderOutput, FolderRendererDraft};
use knobas_app::oneoff::run::{Batch, Ended};
use knobas_app::oneoff::runs::Sink;
use knobas_app::try_on::{self, TryOn};
use knobas_core::checkout::oneoff::{Draft, Stdin};
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// Every row of every table knobas writes to, as text: what a try must leave as it found it.
async fn rows(pool: &PgPool) -> Vec<String> {
    let tables: Vec<(String, String)> = sqlx::query_as(
        "select table_schema, table_name from information_schema.tables
         where table_schema in ('knobas','sync') and table_type = 'BASE TABLE' order by 1, 2",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut all = Vec::new();
    for (schema, table) in tables {
        // The names come from the catalogue just read, never from input.
        let dump: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "select t::text from {schema}.{table} t order by 1"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        all.extend(
            dump.into_iter()
                .map(|(row,)| format!("{schema}.{table}: {row}")),
        );
    }
    all
}

/// Every file under `dir`, from it.
fn files_under(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(listed) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in listed.flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            } else {
                found.push(
                    entry
                        .path()
                        .strip_prefix(dir)
                        .unwrap()
                        .display()
                        .to_string(),
                );
            }
        }
    }
    found
}

async fn settled(
    runs: &Arc<Runs>,
    saved: &folder_render::FolderRenderer,
    on: TryOn,
) -> FolderRenderView {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let view = try_on::folder_view_in(runs, saved, on.clone(), Ask::Look).await;
        if view.running.is_none() || Instant::now() > deadline {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn a_folder_renderer_tried_on_a_chosen_folder_and_on_pasted_text_keeps_nothing() {
    let pool = scratch("try_on_folder").await;

    let tools = tempfile::tempdir().unwrap();
    let script = tools.path().join("site.sh");
    std::fs::write(
        &script,
        "mkdir -p \"$2/img\" && { echo '# Site'; ls \"$1\"; } > \"$2/index.md\" && printf a > \"$2/img/a.txt\"\n",
    )
    .unwrap();
    let saved = folder_render::save(
        &pool,
        &[FolderRendererDraft {
            id: None,
            name: "Site".to_owned(),
            command: format!("sh {} {{path}} {{outdir}}", script.display()),
            holds: vec![],
            folders: vec![],
            output: FolderOutput::Detect,
            timeout_secs: Some(20),
        }],
    )
    .await
    .unwrap();
    let site = saved[0].clone();
    assert_eq!(site.output, FolderOutput::Folder);
    let before = rows(&pool).await;

    let chosen = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(chosen.path().join("flows/.git")).unwrap();
    std::fs::write(chosen.path().join("flows/.git/HEAD"), "ref").unwrap();
    std::fs::write(chosen.path().join("flows/orders.lcproj"), "good").unwrap();
    let on_folder = TryOn::Path {
        path: chosen.path().join("flows").to_string_lossy().into_owned(),
    };
    let on_text = TryOn::Text {
        name: "orders.lcproj".to_owned(),
        text: "pasted".to_owned(),
    };

    // Through the command's own path: the saved renderer by its id, in the tries' registry.
    let first = try_on::folder_view(&pool, &site.id, on_folder.clone(), Ask::Open)
        .await
        .unwrap();
    assert!(first.running.is_some(), "the press runs it: {first:?}");
    let tried = try_on::runs();
    let drawn = settled(&tried, &site, on_folder).await;
    assert!(drawn.failure.is_none(), "{:?}", drawn.failure);
    let token = drawn.drawn.as_ref().expect("a drawing").token.clone();
    let made = knobas_app::folder_render::made::made(&token).expect("read back by its token");
    let index = std::fs::read_to_string(made.dir.join("index.md")).unwrap();
    assert!(index.contains("orders.lcproj"), "{index}");
    assert!(
        !index.contains(".git"),
        "a chosen folder's .git is left out: {index}"
    );
    assert!(
        made.dir.starts_with(try_on::root()),
        "kept under the tries' folder: {}",
        made.dir.display()
    );
    // Not the app's registry, whose store and disk cache a repository's renders go to (other tests here set them).
    assert!(Runs::global().made(&token).is_none() && Runs::global().output(&token).is_none());
    assert!(
        Runs::global()
            .store()
            .is_none_or(|store| !made.dir.starts_with(store))
    );

    try_on::folder_view_in(&tried, &site, on_text.clone(), Ask::Open).await;
    let pasted = settled(&tried, &site, on_text).await;
    let token = pasted.drawn.as_ref().expect("a drawing").token.clone();
    let made = knobas_app::folder_render::made::made(&token).unwrap();
    assert!(
        std::fs::read_to_string(made.dir.join("index.md"))
            .unwrap()
            .contains("orders.lcproj")
    );

    assert!(
        Runs::global().made(&token).is_none(),
        "the app's registry holds no try"
    );
    // The tries' registry keeps nothing on disk past its own folder: every file it wrote is under the tries' root.
    let kept = files_under(&try_on::root().join("made"));
    assert!(
        !kept.is_empty(),
        "the generated folders of the tries are where the tries keep them"
    );
    assert_eq!(rows(&pool).await, before, "no row written");
}

#[tokio::test]
async fn a_pipeline_tried_on_a_chosen_file_runs_its_steps_and_keeps_nothing() {
    let pool = scratch("try_on_piped").await;
    let before = rows(&pool).await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("order.b64");
    std::fs::write(&file, "PG9yZGVyLz4=").unwrap();
    let (name, bytes) = try_on::bytes_of(&TryOn::Path {
        path: file.to_string_lossy().into_owned(),
    })
    .await
    .unwrap();
    let run = knobas_app::file_pipeline::run_bytes(
        &[PipeStep::Base64Decode],
        bytes,
        &name,
        knobas_app::file_pipeline::Limits::default(),
    )
    .await;
    let knobas_app::file_pipeline::PipeRun::Done { name, output } = run else {
        panic!("the steps ran: {run:?}")
    };
    assert_eq!(name, "order");
    assert_eq!(
        output,
        PipeOutput::Text {
            text: "<order/>".to_owned()
        }
    );
    assert_eq!(rows(&pool).await, before, "no row written");
}

#[derive(Default)]
struct Collect(Mutex<Vec<Batch>>);

impl Sink for Collect {
    fn send(&self, batch: &Batch) -> bool {
        self.0.lock().unwrap().push(batch.clone());
        true
    }
}

impl Collect {
    async fn wait(&self) -> Ended {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(ended) = self.0.lock().unwrap().iter().find_map(|b| b.ended.clone()) {
                return ended;
            }
            assert!(Instant::now() < deadline, "the run did not end");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn texts(&self) -> Vec<String> {
        let mut lines = std::collections::BTreeMap::new();
        for batch in self.0.lock().unwrap().iter() {
            for line in batch.lines.iter().chain(batch.changed.iter()) {
                lines.insert(line.n, line.text.clone());
            }
        }
        lines.into_values().collect()
    }
}

#[tokio::test]
async fn a_command_tried_on_pasted_text_reads_it_and_leaves_no_file_and_no_row() {
    let pool = scratch("try_on_command").await;
    let profile = tempfile::tempdir().unwrap();
    let runs = Arc::new(knobas_app::oneoff::runs::Runs::new());
    runs.place(profile.path().join("command-runs"));
    knobas_app::oneoff::save(
        &pool,
        &[
            Draft {
                id: Some("cat".to_owned()),
                name: "cat".to_owned(),
                command: "cat".to_owned(),
                stdin: Stdin::File,
                files: vec!["*.md".to_owned()],
                ..Draft::default()
            },
            Draft {
                id: Some("ls".to_owned()),
                name: "ls".to_owned(),
                command: "ls {dir}".to_owned(),
                ..Draft::default()
            },
        ],
    )
    .await
    .unwrap();
    let before = rows(&pool).await;

    // Pasted text, on stdin: the throwaway file is read, then removed.
    let sink = Arc::new(Collect::default());
    let started = knobas_app::oneoff::try_start_in(
        &runs,
        &pool,
        "cat",
        TryOn::Text {
            name: "notes.md".to_owned(),
            text: "# pasted\n".to_owned(),
        },
        sink.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(sink.wait().await, Ended::Exit { code: 0 });
    assert_eq!(sink.texts(), vec!["# pasted".to_owned()]);
    assert!(
        Path::new(&started.cwd).starts_with(try_on::root()),
        "{}",
        started.cwd
    );
    assert!(
        !Path::new(&started.cwd).exists(),
        "the throwaway folder is gone once it ended"
    );

    // A chosen folder: the folder command runs on it, in its place.
    let chosen = tempfile::tempdir().unwrap();
    std::fs::write(chosen.path().join("a.txt"), "a").unwrap();
    let sink = Arc::new(Collect::default());
    knobas_app::oneoff::try_start_in(
        &runs,
        &pool,
        "ls",
        TryOn::Path {
            path: chosen.path().to_string_lossy().into_owned(),
        },
        sink.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(sink.wait().await, Ended::Exit { code: 0 });
    assert_eq!(sink.texts(), vec!["a.txt".to_owned()]);
    assert!(
        chosen.path().join("a.txt").exists(),
        "a chosen folder is never removed"
    );

    // A file command on a folder is refused, and a pasted one leaves nothing when refused.
    assert!(
        knobas_app::oneoff::try_start_in(
            &runs,
            &pool,
            "cat",
            TryOn::Path {
                path: chosen.path().to_string_lossy().into_owned()
            },
            Arc::new(Collect::default()),
        )
        .await
        .is_err()
    );

    let history = std::fs::read_to_string(profile.path().join("command-runs").join("runs.json"))
        .unwrap_or_default();
    assert!(
        !history.contains(try_on::TRY_ENTITY),
        "no try in the list of runs: {history}"
    );
    assert_eq!(rows(&pool).await, before, "no row written");
}
