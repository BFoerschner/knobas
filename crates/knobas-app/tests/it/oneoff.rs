//! Commands (ADR-0060) at the seam, with real programs: saved to a real
//! `knobas.setting`, run in a real checkout on this disk.
//!
//! Witnessed: a file whose name holds a space, a quote, `$(` and `;` reaches
//! `printf` whole; a script with a path is refused at save; `pwd` and `env`
//! as the run sees them; a program found only through *Add to PATH*; `sleep`
//! stopped at its timeout and by a stop, with the child it started; a fifth
//! run refused; a run's lines, places and findings; nothing of a run's folder
//! left after the quit.

#![cfg(unix)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knobas_app::checkout::set_override;
use knobas_app::oneoff::history::RunRow;
use knobas_app::oneoff::run::{Batch, Ended, Target};
use knobas_app::oneoff::runs::{Runs, Sink};
use knobas_core::checkout::oneoff::{Draft, EnvVar, StartIn};
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// A repository the mirror holds, its checkout `dir`.
async fn repo(pool: &PgPool, dir: &Path, token: &str) -> String {
    let id = format!("gitea-oo:tidewater/run-{token}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'repo',$2)")
        .bind(&id)
        .bind(format!("tidewater/run-{token}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1,'gitea-oo','repo',$2,'','{}'::jsonb,null)",
    )
    .bind(&id)
    .bind(format!("tidewater/run-{token}"))
    .execute(pool)
    .await
    .unwrap();
    set_override(pool, &id, Some(&dir.to_string_lossy()))
        .await
        .unwrap();
    id
}

fn draft(id: &str, command: &str) -> Draft {
    Draft {
        id: Some(id.to_owned()),
        name: id.to_owned(),
        command: command.to_owned(),
        ..Draft::default()
    }
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
    fn ended(&self) -> Option<Ended> {
        self.0.lock().unwrap().iter().find_map(|b| b.ended.clone())
    }

    fn texts(&self) -> Vec<String> {
        let batches = self.0.lock().unwrap();
        let mut lines: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
        for batch in batches.iter() {
            for line in batch.lines.iter().chain(batch.changed.iter()) {
                lines.insert(line.n, line.text.clone());
            }
        }
        lines.into_values().collect()
    }

    async fn wait(&self, within: Duration) -> Ended {
        let deadline = Instant::now() + within;
        loop {
            if let Some(ended) = self.ended() {
                return ended;
            }
            assert!(
                Instant::now() < deadline,
                "the run did not end within {within:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

async fn run(
    runs: &Arc<Runs>,
    pool: &PgPool,
    id: &str,
    command: &str,
    target: Target,
) -> (Arc<Collect>, knobas_app::oneoff::run::Started) {
    let sink = Arc::new(Collect::default());
    let started = knobas_app::oneoff::start_in(runs, pool, id, command, target, sink.clone())
        .await
        .unwrap_or_else(|e| panic!("{command} must start: {}", e.message));
    (sink, started)
}

fn alive(pid: i32) -> bool {
    rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).unwrap()).is_ok()
}

#[tokio::test]
async fn an_evil_file_name_reaches_the_program_whole() {
    let pool = scratch("oneoff_evil").await;
    let dir = tempfile::tempdir().unwrap();
    let evil = "a b 'q\" $(x); rm -rf ~.ts";
    std::fs::write(dir.path().join(evil), "x").unwrap();
    let id = repo(&pool, dir.path(), "evil").await;
    knobas_app::oneoff::save(&pool, &[draft("printf", "printf %s\\n {relfile} {file}")])
        .await
        .unwrap();
    let runs = Arc::new(Runs::new());
    let (sink, started) = run(
        &runs,
        &pool,
        &id,
        "printf",
        Target::File {
            path: evil.to_owned(),
            lines: None,
            at: None,
        },
    )
    .await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 0 }
    );
    let real = dir.path().join(evil).to_string_lossy().into_owned();
    assert_eq!(sink.texts(), [evil.to_owned(), real.clone()]);
    assert_eq!(started.argv, ["printf", "%s\\n", evil, &real]);
    // Nothing ran: no folder named ~ appeared, nothing was removed.
    assert!(dir.path().join(evil).exists());
}

#[tokio::test]
async fn a_script_with_a_path_is_refused_at_save() {
    let pool = scratch("oneoff_save").await;
    let error = knobas_app::oneoff::save(&pool, &[draft("lint", "sh -c 'eslint {file} | head'")])
        .await
        .unwrap_err();
    assert!(error.message.contains("quoted script"), "{}", error.message);
    assert!(knobas_app::oneoff::list(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn pwd_and_env_as_the_run_sees_them() {
    let pool = scratch("oneoff_env").await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("pkg/web")).unwrap();
    std::fs::write(dir.path().join("pkg/web/a.ts"), "x").unwrap();
    let bin = dir.path().join("tools");
    std::fs::create_dir(&bin).unwrap();
    let only_here = bin.join("only-here");
    std::fs::write(&only_here, "#!/bin/sh\necho found-through-add-to-path\n").unwrap();
    std::fs::set_permissions(
        &only_here,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let id = repo(&pool, dir.path(), "env").await;
    knobas_app::oneoff::set_shell_path(&pool, false)
        .await
        .unwrap();
    knobas_app::oneoff::save(
        &pool,
        &[
            draft("pwd", "pwd"),
            Draft {
                start_in: StartIn::Target,
                ..draft("pwd-in", "sh -c pwd _ {file}")
            },
            Draft {
                env: vec![EnvVar {
                    name: "MY_LINE".to_owned(),
                    value: "mine".to_owned(),
                }],
                ..draft("env", "env")
            },
            Draft {
                path: vec!["{root}/tools".to_owned()],
                ..draft("only", "only-here")
            },
        ],
    )
    .await
    .unwrap();
    // SAFETY: this test's process sets a variable no other test reads.
    unsafe { std::env::set_var("KNOBAS_SECRET_FOR_TEST", "leak") };
    let runs = Arc::new(Runs::new());
    let real_root = std::fs::canonicalize(dir.path()).unwrap();

    let (sink, _) = run(&runs, &pool, &id, "pwd", Target::Repository).await;
    sink.wait(Duration::from_secs(10)).await;
    assert_eq!(std::fs::canonicalize(&sink.texts()[0]).unwrap(), real_root);

    let file = Target::File {
        path: "pkg/web/a.ts".to_owned(),
        lines: None,
        at: None,
    };
    let (sink, started) = run(&runs, &pool, &id, "pwd-in", file).await;
    sink.wait(Duration::from_secs(10)).await;
    assert_eq!(
        std::fs::canonicalize(&sink.texts()[0]).unwrap(),
        real_root.join("pkg/web")
    );
    assert!(started.cwd.ends_with("pkg/web"));

    let (sink, _) = run(&runs, &pool, &id, "env", Target::Repository).await;
    sink.wait(Duration::from_secs(10)).await;
    let env = sink.texts();
    assert!(env.iter().any(|l| l == "MY_LINE=mine"));
    assert!(env.iter().any(|l| l == "FORCE_COLOR=1"));
    assert!(!env.iter().any(|l| l.starts_with("KNOBAS_")), "{env:?}");

    let (sink, _) = run(&runs, &pool, &id, "only", Target::Repository).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 0 }
    );
    assert_eq!(sink.texts(), ["found-through-add-to-path"]);
}

/// A script that starts a child which sleeps, and says the child's pid.
fn sleeper(dir: &Path) -> String {
    let script = dir.join("sleeper.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 300 &\necho child=$!\nwait\n").unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    script.to_string_lossy().into_owned()
}

async fn child_of(sink: &Collect) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = sink
            .texts()
            .iter()
            .find_map(|l| l.strip_prefix("child=")?.parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "the script never said its child");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_timeout_and_a_stop_kill_the_child_it_started() {
    let pool = scratch("oneoff_kill").await;
    let dir = tempfile::tempdir().unwrap();
    let script = sleeper(dir.path());
    let id = repo(&pool, dir.path(), "kill").await;
    knobas_app::oneoff::save(
        &pool,
        &[
            Draft {
                timeout_secs: Some(1),
                ..draft("slow", &script)
            },
            draft("long", &script),
        ],
    )
    .await
    .unwrap();
    let runs = Arc::new(Runs::new());

    let started = Instant::now();
    let (sink, _) = run(&runs, &pool, &id, "slow", Target::Repository).await;
    let child = child_of(&sink).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::TimedOut { seconds: 1 }
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !alive(child),
        "the timed-out run's child {child} still runs"
    );

    let (sink, started) = run(&runs, &pool, &id, "long", Target::Repository).await;
    let child = child_of(&sink).await;
    runs.stop(&started.run);
    assert_eq!(sink.wait(Duration::from_secs(10)).await, Ended::Stopped);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!alive(child), "the stopped run's child {child} still runs");
}

#[tokio::test]
async fn a_fifth_run_is_refused_and_the_same_run_is_found_again() {
    let pool = scratch("oneoff_five").await;
    let dir = tempfile::tempdir().unwrap();
    for f in ["a", "b", "c", "d", "e"] {
        std::fs::write(dir.path().join(f), "x").unwrap();
    }
    let id = repo(&pool, dir.path(), "five").await;
    knobas_app::oneoff::save(&pool, &[draft("wait", "sh -c 'sleep 30' _ {file}")])
        .await
        .unwrap();
    let runs = Arc::new(Runs::new());
    let file = |f: &str| Target::File {
        path: f.to_owned(),
        lines: None,
        at: None,
    };
    let mut started = Vec::new();
    for f in ["a", "b", "c", "d"] {
        started.push(run(&runs, &pool, &id, "wait", file(f)).await.1);
    }
    let again = run(&runs, &pool, &id, "wait", file("a")).await.1;
    assert!(again.already);
    assert_eq!(again.run, started[0].run);
    let refused = knobas_app::oneoff::start_in(
        &runs,
        &pool,
        &id,
        "wait",
        file("e"),
        Arc::new(Collect::default()),
    )
    .await
    .unwrap_err();
    assert!(
        refused.message.contains("four commands are running"),
        "{}",
        refused.message
    );
    for s in &started {
        runs.stop(&s.run);
    }
}

#[tokio::test]
async fn a_linter_s_places_and_findings_and_a_program_that_is_not_there() {
    let pool = scratch("oneoff_places").await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.ts"), "x").unwrap();
    let lint = dir.path().join("lint.sh");
    std::fs::write(
        &lint,
        "#!/bin/sh\nprintf '\\033[1msrc/a.ts:3:7: error: no semicolon [semi]\\033[0m\\n'\n\
         echo '/etc/passwd:1:1: not ours' >&2\necho 'src/missing.ts:1:1: gone'\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&lint, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let id = repo(&pool, dir.path(), "places").await;
    knobas_app::oneoff::save(
        &pool,
        &[
            draft("lint", &lint.to_string_lossy()),
            draft("gone", "no-such-program-knobas-test {file}"),
        ],
    )
    .await
    .unwrap();
    let runs = Arc::new(Runs::new());
    let (sink, started) = run(&runs, &pool, &id, "lint", Target::Repository).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 1 }
    );
    let batches = sink.0.lock().unwrap().clone();
    let last = batches.last().unwrap();
    assert_eq!(last.findings, 1);
    let lines: Vec<_> = batches
        .iter()
        .flat_map(|b| b.lines.iter().chain(b.changed.iter()))
        .cloned()
        .collect();
    let first = lines.iter().rev().find(|l| l.n == 0).unwrap();
    assert_eq!(first.places[0].path, "src/a.ts");
    assert_eq!(first.places[0].line, Some(3));
    assert_eq!(first.finding.as_ref().unwrap().rule, "semi");
    assert!(first.runs[0].style.bold);
    assert!(
        lines
            .iter()
            .filter(|l| l.n != 0)
            .all(|l| l.places.is_empty())
    );
    let rows: Vec<RunRow> = runs.rows(Some(&id));
    assert_eq!(rows[0].id, started.run);
    assert_eq!(rows[0].findings, 1);

    let (sink, _) = run(
        &runs,
        &pool,
        &id,
        "gone",
        Target::File {
            path: "src/a.ts".to_owned(),
            lines: None,
            at: None,
        },
    )
    .await;
    match sink.wait(Duration::from_secs(10)).await {
        Ended::NotStarted { reason } => assert!(reason.contains("was not found"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_file_outside_the_checkout_and_a_repository_not_cloned_start_nothing() {
    let pool = scratch("oneoff_outside").await;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("x.ts"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.ts"), dir.path().join("link.ts")).unwrap();
    let id = repo(&pool, dir.path(), "outside").await;
    knobas_app::oneoff::save(&pool, &[draft("cat", "cat {file}"), draft("ls", "ls")])
        .await
        .unwrap();
    let runs = Arc::new(Runs::new());
    for path in ["../x.ts", "link.ts", ".git/config", "nope.ts"] {
        let error = knobas_app::oneoff::start_in(
            &runs,
            &pool,
            &id,
            "cat",
            Target::File {
                path: path.to_owned(),
                lines: None,
                at: None,
            },
            Arc::new(Collect::default()),
        )
        .await
        .unwrap_err();
        assert!(
            error.message.contains("not in your working tree"),
            "{path}: {}",
            error.message
        );
    }
    // A command is run only on what it takes.
    let error = knobas_app::oneoff::start_in(
        &runs,
        &pool,
        &id,
        "cat",
        Target::Repository,
        Arc::new(Collect::default()),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("is not run on"), "{}", error.message);
    assert_eq!(runs.running(), 0);
}

#[tokio::test]
async fn nothing_of_a_run_is_left_under_the_profile_after_the_quit() {
    let pool = scratch("oneoff_quit").await;
    let dir = tempfile::tempdir().unwrap();
    let profile = tempfile::tempdir().unwrap();
    let script = sleeper(dir.path());
    let id = repo(&pool, dir.path(), "quit").await;
    knobas_app::oneoff::save(
        &pool,
        &[
            draft(
                "out",
                "sh -c 'printf \"%s-%s\\n\" SECRET OUTPUT; echo x > \"$1\"' _ {out}",
            ),
            draft("long", &script),
        ],
    )
    .await
    .unwrap();
    let runs = Arc::new(Runs::new());
    runs.place(profile.path().join("command-runs"));
    let (sink, _) = run(&runs, &pool, &id, "out", Target::Repository).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 0 }
    );
    assert_eq!(sink.texts(), ["SECRET-OUTPUT"]);
    let (sink, _) = run(&runs, &pool, &id, "long", Target::Repository).await;
    let child = child_of(&sink).await;
    runs.kill_all();
    sink.wait(Duration::from_secs(10)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!alive(child));
    let left: Vec<_> = std::fs::read_dir(profile.path().join("command-runs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["runs.json"], "only the list of runs stays");
    let list = std::fs::read_to_string(profile.path().join("command-runs/runs.json")).unwrap();
    assert!(!list.contains("SECRET-OUTPUT"), "the list holds no output");
}

#[tokio::test]
async fn stdin_holds_the_file_or_its_selected_lines_and_a_commit_its_sha() {
    let pool = scratch("oneoff_stdin").await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    let id = repo(&pool, dir.path(), "stdin").await;
    knobas_app::oneoff::save(
        &pool,
        &[
            Draft {
                stdin: knobas_core::checkout::oneoff::Stdin::File,
                ..draft("all", "cat")
            },
            Draft {
                stdin: knobas_core::checkout::oneoff::Stdin::Selection,
                ..draft("some", "cat")
            },
            draft("show", "printf %s {sha}"),
        ],
    )
    .await
    .unwrap();
    let runs = Arc::new(Runs::new());
    let file = |lines| Target::File {
        path: "a.txt".to_owned(),
        lines,
        at: None,
    };
    let (sink, _) = run(&runs, &pool, &id, "all", file(None)).await;
    sink.wait(Duration::from_secs(10)).await;
    assert_eq!(sink.texts(), ["one", "two", "three", "four"]);
    let (sink, _) = run(&runs, &pool, &id, "some", file(Some((2, 3)))).await;
    sink.wait(Duration::from_secs(10)).await;
    assert_eq!(sink.texts(), ["two", "three"]);
    let (sink, _) = run(
        &runs,
        &pool,
        &id,
        "show",
        Target::Commit {
            sha: "abc1234".to_owned(),
        },
    )
    .await;
    sink.wait(Duration::from_secs(10)).await;
    assert_eq!(sink.texts(), ["abc1234"]);
    // A git name the grammar refuses starts nothing and says which value.
    let (sink, _) = run(
        &runs,
        &pool,
        &id,
        "show",
        Target::Commit {
            sha: "--upload-pack=x".to_owned(),
        },
    )
    .await;
    match sink.wait(Duration::from_secs(10)).await {
        Ended::NotStarted { reason } => {
            assert!(reason.contains("not a name knobas passes"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_result_that_is_not_lines_out_outdir_and_a_stdout_that_is_not_text() {
    use knobas_app::oneoff::result::{Kind, RunResult};
    let pool = scratch("oneoff_result").await;
    let dir = tempfile::tempdir().unwrap();
    let profile = tempfile::tempdir().unwrap();
    let id = repo(&pool, dir.path(), "result").await;
    knobas_app::oneoff::save(
        &pool,
        &[
            draft("page", "sh -c 'echo making >&2; printf \"<!doctype html><p>hi\" > \"$1\"' _ {out}.html"),
            draft("png", "printf \\211PNG\\r\\n\\032\\n0000"),
            draft("many", "sh -c 'printf a > \"$1/a.txt\"; mkdir \"$1/sub\"; printf \"[]\" > \"$1/sub/b.json\"; ln -s /etc/passwd \"$1/link\"' _ {outdir}"),
            draft("fails", "sh -c 'printf a > \"$1/a.txt\"; exit 3' _ {outdir}"),
        ],
    )
    .await
    .unwrap();
    let runs = Arc::new(Runs::new());
    runs.place(profile.path().join("command-runs"));
    let last = |sink: &Collect| sink.0.lock().unwrap().last().unwrap().result.clone();

    let (sink, started) = run(&runs, &pool, &id, "page", Target::Repository).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 0 }
    );
    assert_eq!(
        last(&sink),
        Some(RunResult::One {
            kind: Kind::Page,
            size: 20,
            name: "output.html".to_owned(),
            overruled: None
        })
    );
    assert_eq!(sink.texts(), ["making"]);
    let handle = runs.get(&started.run).unwrap();
    assert_eq!(handle.result_bytes(None).unwrap(), b"<!doctype html><p>hi");

    let (sink, started) = run(&runs, &pool, &id, "png", Target::Repository).await;
    sink.wait(Duration::from_secs(10)).await;
    assert!(
        matches!(
            last(&sink),
            Some(RunResult::One {
                kind: Kind::Image { .. },
                ..
            })
        ),
        "{:?}",
        last(&sink)
    );
    assert!(
        sink.texts().is_empty(),
        "a stdout that is the result is no lines"
    );
    assert!(
        runs.get(&started.run)
            .unwrap()
            .result_bytes(None)
            .unwrap()
            .starts_with(b"\x89PNG")
    );

    let (sink, started) = run(&runs, &pool, &id, "many", Target::Repository).await;
    sink.wait(Duration::from_secs(10)).await;
    let Some(RunResult::Files {
        files,
        left_out_total,
        ..
    }) = last(&sink)
    else {
        panic!("{:?}", last(&sink))
    };
    assert_eq!(
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["a.txt", "sub/b.json"]
    );
    assert_eq!(left_out_total, 1);
    assert_eq!(
        runs.get(&started.run)
            .unwrap()
            .result_bytes(Some("sub/b.json"))
            .unwrap(),
        b"[]"
    );

    let (sink, _) = run(&runs, &pool, &id, "fails", Target::Repository).await;
    assert_eq!(
        sink.wait(Duration::from_secs(10)).await,
        Ended::Exit { code: 3 }
    );
    assert_eq!(
        last(&sink),
        None,
        "a run that failed keeps nothing of {{outdir}}"
    );
}
