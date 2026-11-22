//! A test binary's own server goes when the binary does, however it goes.
//!
//! Each case starts this very executable again as a child that runs only
//! [`probe`]: a test that takes the by-hand path of `test_util` (no
//! `KNOBAS_TEST_DB_URL`), so it runs `initdb`, starts a postmaster in
//! `$TMPDIR/knobas-test-<child pid>` and the supervisor beside it. The child
//! says when its database answers; the case then ends it -- a normal return, a
//! panic, an abort, a `SIGKILL` mid-run -- and asserts that within a few
//! seconds nothing whose command line names that scratch root is still
//! running (neither the postmaster, whose `-D` is under it, nor the
//! supervisor), and that the root and its lock are gone.
//!
//! Before the supervisor the postmaster outlived every one of these:
//! `test_util` keeps the server in a `static`, statics are never dropped, and
//! only the *next* run's reaper stopped it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How the child is told which way to end; unset, [`probe`] does nothing.
const MODE_VAR: &str = "KNOBAS_SUPERVISOR_PROBE";

/// The line the child prints once its database answers.
const READY: &str = "knobas-supervisor-probe-ready";

/// How long a child may take to bring its server up. `initdb` plus a start
/// plus the migrations, on a loaded machine.
const READY_TIMEOUT: Duration = Duration::from_secs(180);

/// How long the supervisor has to stop the server and remove the root once
/// the child is gone.
const GONE_TIMEOUT: Duration = Duration::from_secs(30);

/// The child's half: a database on a server of this process's own, then --
/// once the parent has seen that server and its supervisor running, and says
/// so with a line on stdin -- the end it asked for. In the `SIGKILL` case no
/// line comes: the child is killed while it waits for one.
#[tokio::test]
#[ignore = "the child the supervisor cases start; does nothing on its own"]
async fn probe() {
    let Ok(mode) = std::env::var(MODE_VAR) else {
        return;
    };
    let pool = knobas_db::test_util::test_pool().await;
    sqlx::query("select 1").execute(&pool).await.unwrap();
    let mut out = std::io::stdout();
    writeln!(out, "{READY}").unwrap();
    out.flush().unwrap();
    let mut go = String::new();
    std::io::stdin().read_line(&mut go).unwrap();
    match mode.as_str() {
        "exit" => {}
        "panic" => panic!("the probe panics, as a failing test does"),
        "abort" => std::process::abort(),
        // Killed in `read_line`; here only if the parent went first.
        "kill" => {}
        other => panic!("unknown probe mode {other}"),
    }
}

#[test]
fn a_normal_exit_stops_the_server() {
    let status = run_probe("exit", false);
    assert!(status.success(), "the probe passed: {status}");
}

#[test]
fn a_panicking_test_stops_the_server() {
    let status = run_probe("panic", false);
    assert_eq!(
        status.code(),
        Some(101),
        "libtest's failure status: {status}"
    );
}

#[test]
fn an_abort_stops_the_server() {
    use std::os::unix::process::ExitStatusExt;
    let status = run_probe("abort", false);
    assert_eq!(status.signal(), Some(6), "SIGABRT: {status}");
}

/// The case the fix is for: nothing in the binary runs at all.
#[test]
fn a_sigkill_mid_run_stops_the_server() {
    use std::os::unix::process::ExitStatusExt;
    let status = run_probe("kill", true);
    assert_eq!(status.signal(), Some(9), "SIGKILL: {status}");
}

/// Start the probe in `mode`, wait for its database, `SIGKILL` it if `kill`,
/// wait for it, and assert its server and scratch root are gone. Returns how
/// it ended.
fn run_probe(mode: &str, kill: bool) -> ExitStatus {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["supervisor::probe", "--exact", "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(MODE_VAR, mode)
        .env_remove(knobas_db::test_util::GATE_URL_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the probe");
    let root = std::env::temp_dir().join(format!("knobas-test-{}", child.id()));
    let name = root.file_name().unwrap().to_str().unwrap().to_owned();

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
            // At the end of libtest's own `test supervisor::probe ... `.
            Ok(line) if line.ends_with(READY) => break,
            Ok(_) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the probe never reported its database ({error})");
            }
        }
    }

    let postmaster = postmaster_pid(&root.join("data"));
    assert!(alive(postmaster), "the probe's postmaster runs");
    let running = naming(&name);
    assert!(
        running
            .iter()
            .any(|line| line.contains("knobas-test-supervisor")),
        "a supervisor runs beside the server: {running:#?}"
    );
    assert!(
        running.iter().any(|line| line.contains("-D")),
        "the postmaster's command line names the root: {running:#?}"
    );

    if kill {
        child.kill().expect("SIGKILL the probe");
    } else {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "go").expect("tell the probe to end");
    }
    let status = child.wait().expect("the probe ends");

    let lock = lock_of(&root);
    let deadline = Instant::now() + GONE_TIMEOUT;
    while (alive(postmaster) || root.exists() || lock.exists() || !naming(&name).is_empty())
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !alive(postmaster),
        "postmaster {postmaster} outlived the probe ({mode})"
    );
    let left = naming(&name);
    assert!(
        left.is_empty(),
        "still running after the probe ended ({mode}): {left:#?}"
    );
    assert!(
        !root.exists(),
        "the root outlived the probe: {}",
        root.display()
    );
    assert!(
        !lock.exists(),
        "the lock outlived the probe: {}",
        lock.display()
    );
    status
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

/// The command lines of running processes that name the scratch directory
/// `name` -- as a path segment, so `knobas-test-42` is not `knobas-test-421`.
fn naming(name: &str) -> Vec<String> {
    let ps = Command::new("ps")
        .args(["-A", "-o", "args="])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter(|line| {
            line.match_indices(name).any(|(at, _)| {
                !line[at + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_digit())
            })
        })
        .map(str::to_owned)
        .collect()
}

/// The ownership lock beside a scratch root.
fn lock_of(root: &Path) -> PathBuf {
    let mut path = root.as_os_str().to_os_string();
    path.push(".lock");
    PathBuf::from(path)
}
