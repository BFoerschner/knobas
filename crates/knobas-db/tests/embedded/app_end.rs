//! The app's server goes when knobas does, however knobas goes (2026-11-24).
//!
//! On Linux a logout ends the display, and GTK ends knobas from inside
//! (Xlib's I/O-error handler calls `exit(1)`), before its quit has stopped
//! PostgreSQL or in the middle of it. The postmaster was left running: a
//! `SIGTERM` followed at once by the display's end did it three runs in
//! three. Each case here starts this very executable again as a child that
//! runs only [`probe`]: an [`EmbeddedDb`] on a profile folder of the parent's,
//! held and never stopped, as the app holds its own. The case ends the child
//! without a quit -- `exit(1)` from a thread that is not the one holding the
//! database, as Xlib's handler does, or a `SIGKILL` -- and asserts that the
//! postmaster is gone within seconds, that its data directory is intact, and
//! that the next start opens it.
//!
//! Before the server had a supervisor both cases left the postmaster
//! running until the next start adopted it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use knobas_db::{DbConfig, EmbeddedDb};

/// How the child is told which way to end; unset, [`probe`] does nothing.
const MODE_VAR: &str = "KNOBAS_APP_END_PROBE";
/// The profile folder the child starts its server in.
const ROOT_VAR: &str = "KNOBAS_APP_END_ROOT";
/// The line the child prints once its database answers.
const READY: &str = "knobas-app-end-probe-ready";

/// `initdb` plus a start, on a loaded machine.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
/// How long the supervisor has to stop the server once the child is gone.
const GONE_TIMEOUT: Duration = Duration::from_secs(30);

/// The child's half: the app's server, held, then -- once the parent has
/// seen it running and says so with a line on stdin -- the end it asked for.
/// In the `SIGKILL` case no line comes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the child the app-end cases start; does nothing on its own"]
async fn probe() {
    let (Ok(mode), Ok(root)) = (std::env::var(MODE_VAR), std::env::var(ROOT_VAR)) else {
        return;
    };
    let db = EmbeddedDb::start(DbConfig {
        root_dir: PathBuf::from(root),
        existing_url: None,
    })
    .await
    .expect("the probe's server starts");
    sqlx::query("select 1").execute(db.pool()).await.unwrap();
    let mut out = std::io::stdout();
    writeln!(out, "{READY}").unwrap();
    out.flush().unwrap();
    let ended = std::thread::spawn(move || {
        let mut go = String::new();
        std::io::stdin().read_line(&mut go).unwrap();
        if mode == "exit" {
            // What Xlib's I/O-error handler does when the display has gone:
            // from another thread, with the database still held, no quit.
            std::process::exit(1);
        }
    });
    tokio::task::spawn_blocking(move || ended.join())
        .await
        .unwrap()
        .unwrap();
    // Reached only if the parent went first.
    std::mem::forget(db);
}

#[test]
fn an_exit_without_a_quit_stops_the_server() {
    run_probe("exit");
}

#[test]
fn a_sigkill_stops_the_server() {
    run_probe("kill");
}

/// A clean stop ends the supervisor with the server: nothing of the
/// profile's is left running.
#[tokio::test(flavor = "multi_thread")]
async fn a_clean_stop_leaves_no_supervisor() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    assert!(
        !supervisors_of(&data).is_empty(),
        "a supervisor runs beside the server"
    );
    db.stop().await.unwrap();
    let left = supervisors_of(&data);
    assert!(left.is_empty(), "still running after the stop: {left:#?}");
}

/// Start the probe in `mode` on a profile folder of its own, wait for its
/// database, end it without a quit, and assert the server is gone, its data
/// intact and the next start good.
fn run_probe(mode: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let data = root.join("data");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["app_end::probe", "--exact", "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(MODE_VAR, mode)
        .env(ROOT_VAR, &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the probe");
    wait_ready(&mut child);

    let postmaster = postmaster_pid(&data);
    // Stops it by hand if the case fails, so a failure leaves nothing running.
    let _cleanup = StopOnDrop(postmaster);
    assert!(alive(postmaster), "the probe's postmaster runs");
    let supervisors = supervisors_of(&data);
    assert!(
        !supervisors.is_empty(),
        "a supervisor runs beside the server"
    );

    if mode == "kill" {
        child.kill().expect("SIGKILL the probe");
    } else {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "go").expect("tell the probe to end");
    }
    let status = child.wait().expect("the probe ends");
    if mode == "exit" {
        assert_eq!(status.code(), Some(1), "the probe's exit(1): {status}");
    }

    let deadline = Instant::now() + GONE_TIMEOUT;
    while (alive(postmaster) || !supervisors_of(&data).is_empty()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !alive(postmaster),
        "postmaster {postmaster} outlived the probe ({mode})"
    );
    let left = supervisors_of(&data);
    assert!(left.is_empty(), "still running ({mode}): {left:#?}");
    assert!(
        !data.join("postmaster.pid").exists(),
        "a clean stop removes postmaster.pid ({mode})"
    );
    assert!(data.join("PG_VERSION").is_file(), "the data is kept");

    // The next start opens it: a fast stop, not a crash to recover from.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = EmbeddedDb::start(DbConfig {
            root_dir: root.clone(),
            existing_url: None,
        })
        .await
        .expect("the next start");
        sqlx::query("select 1").execute(db.pool()).await.unwrap();
        db.stop().await.unwrap();
    });
}

fn wait_ready(child: &mut Child) {
    let (lines_tx, lines_rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = lines_tx.send(line);
        }
    });
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines_rx.recv_timeout(left) {
            // At the end of libtest's own `test app_end::probe ... `.
            Ok(line) if line.ends_with(READY) => return,
            Ok(_) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the probe never reported its database ({error})");
            }
        }
    }
}

/// A failed case's postmaster, stopped as a quit would (`SIGINT`, the fast
/// stop) when the case ends.
struct StopOnDrop(u32);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if alive(self.0) {
            let _ = Command::new("kill")
                .args(["-INT", &self.0.to_string()])
                .status();
        }
    }
}

/// The postmaster's pid: the first line of its `postmaster.pid`.
fn postmaster_pid(data: &Path) -> u32 {
    let pid_file = std::fs::read_to_string(data.join("postmaster.pid")).expect("postmaster.pid");
    pid_file
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
        .expect("a pid on postmaster.pid's first line")
}

/// Whether a process `pid` exists.
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The command lines of running supervisors of the server on `data`.
fn supervisors_of(data: &Path) -> Vec<String> {
    let ps = Command::new("ps")
        .args(["-A", "-ww", "-o", "args="])
        .output()
        .expect("ps runs");
    let data = data.to_string_lossy();
    String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter(|line| line.contains(knobas_db::supervisor::NAME) && line.contains(&*data))
        .map(str::to_owned)
        .collect()
}
