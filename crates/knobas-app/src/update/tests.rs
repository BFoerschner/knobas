//! The updater's order on recording fakes (decisions D9 to D12).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_update::channel::{Channel, Endpoints, Fetch, FetchError};
use knobas_update::install::windows::Launch;
use knobas_update::keys::Keys;
use knobas_update::standing::{ByHandWhy, InstallKind, Standing};
use knobas_update::store::{Attempt, Files};

use super::*;

const FILE: &str = "knobas_3.2.9-ux28_x64-setup.exe";
const ONE: &str = include_str!("../../../knobas-update/tests/fixtures/one.pub");
const TWO: &str = include_str!("../../../knobas-update/tests/fixtures/two.pub");
const BODY: &[u8] =
    include_bytes!("../../../knobas-update/tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe");
const SIG: &str =
    include_str!("../../../knobas-update/tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe.sig");
const LATEST: &str = "https://github.com/BFoerschner/knobas/releases/latest/download/latest.json";
const LIST: &str = "https://api.github.com/repos/BFoerschner/knobas/releases?per_page=10";

/// A fetcher answering from a table, counting the requests.
#[derive(Default)]
struct Table {
    answers: Mutex<HashMap<String, String>>,
    asked: Mutex<Vec<String>>,
    /// Held while a test wants a check to stay in flight.
    gate: tokio::sync::Mutex<()>,
}

#[async_trait::async_trait]
impl Fetch for Table {
    async fn get(&self, url: &str, _cap: u64) -> Result<Vec<u8>, FetchError> {
        self.asked.lock().unwrap().push(url.to_owned());
        let _held = self.gate.lock().await;
        self.answers
            .lock()
            .unwrap()
            .get(url)
            .map(|body| body.clone().into_bytes())
            .ok_or(FetchError::NotFound)
    }
}

fn manifest(schema: i64) -> String {
    serde_json::json!({
        "version": "3.2.9-ux28",
        "pub_date": "2026-11-22T09:41:07Z",
        "platforms": {
            "windows-x86_64-nsis": {
                "url": format!("https://github.com/BFoerschner/knobas/releases/download/v3.2.9-ux28/{FILE}"),
                "signature": SIG.trim(),
            }
        },
        "knobas": { "schema": schema, "sizes": { "windows-x86_64-nsis": BODY.len() } }
    })
    .to_string()
}

/// A host recording what the updater did, in order.
#[derive(Default)]
struct Recording {
    did: Mutex<Vec<String>>,
    backup: Mutex<Option<Backup>>,
    states: Mutex<Vec<UpdateState>>,
}

#[async_trait::async_trait]
impl Host for Recording {
    fn emit(&self, state: &UpdateState) {
        self.states.lock().unwrap().push(state.clone());
    }
    async fn backup(&self) -> Backup {
        self.did.lock().unwrap().push("backup".to_owned());
        self.backup
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Backup::Taken("knobas-backup.knobas".to_owned()))
    }
    fn quit(&self, how: Quit) {
        self.did.lock().unwrap().push(format!("quit {how:?}"));
    }
}

impl Recording {
    fn did(&self) -> Vec<String> {
        self.did.lock().unwrap().clone()
    }
}

static STARTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record_start(launch: &Launch) -> std::io::Result<()> {
    STARTED
        .lock()
        .unwrap()
        .push(launch.program.display().to_string());
    Ok(())
}

struct Rig {
    root: tempfile::TempDir,
    fetch: Arc<Table>,
    host: Arc<Recording>,
}

impl Rig {
    fn new() -> Self {
        let fetch = Arc::new(Table::default());
        fetch
            .answers
            .lock()
            .unwrap()
            .insert(LATEST.to_owned(), manifest(102));
        Self {
            root: tempfile::tempdir().unwrap(),
            fetch,
            host: Arc::new(Recording::default()),
        }
    }

    fn config(&self, current: &str, standing: Standing, keys: Keys) -> Config {
        Config {
            current: current.to_owned(),
            current_schema: 101,
            standing,
            platform: match standing {
                Standing::InPlace {
                    kind: InstallKind::App,
                } => Some("darwin-aarch64-app".to_owned()),
                Standing::ByHand { kind: None, .. } => None,
                _ => Some("windows-x86_64-nsis".to_owned()),
            },
            checks_itself: true,
            demo: false,
            files: Files::new(self.root.path().join("data/update")),
            cache: self.cache(),
            endpoints: Endpoints::default(),
            keys,
            fetch: Arc::clone(&self.fetch) as Arc<dyn Fetch>,
            installed: None,
            started_at: Utc::now(),
            auto_install: false,
            stall: Duration::from_secs(5),
            start_installer: record_start,
        }
    }

    fn cache(&self) -> PathBuf {
        self.root.path().join("cache/update")
    }

    fn updater(&self, current: &str, standing: Standing) -> Arc<Updater> {
        Updater::new(
            self.config(current, standing, keys()),
            Arc::clone(&self.host) as Arc<dyn Host>,
        )
    }

    /// The download left by an earlier run, beside the state that offered it.
    fn downloaded(&self) {
        std::fs::create_dir_all(self.cache()).unwrap();
        std::fs::write(self.cache().join(FILE), BODY).unwrap();
    }
}

fn keys() -> Keys {
    Keys::from_texts(&[("primary", ONE), ("spare", TWO)]).unwrap()
}

fn nsis() -> Standing {
    Standing::InPlace {
        kind: InstallKind::Nsis,
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} never happened");
}

#[tokio::test]
async fn a_check_offers_the_newer_version_and_marks_the_cog() {
    let rig = Rig::new();
    let updater = rig.updater("3.2.9-ux27", nsis());
    let state = updater.check().await.unwrap();
    let offer = state.offer.expect("an offer");
    assert_eq!(offer.version, "3.2.9-ux28");
    assert_eq!(offer.bytes, Some(BODY.len() as u64));
    assert_eq!(offer.changes_schema, Some(true));
    assert!(state.marked);
    assert_eq!(state.phase, Phase::Idle);
    assert!(state.check_failed.is_none());
}

#[tokio::test]
async fn this_version_is_up_to_date_and_unmarked() {
    let rig = Rig::new();
    let state = rig.updater("3.2.9-ux28", nsis()).check().await.unwrap();
    assert_eq!(state.newest.unwrap().version, "3.2.9-ux28");
    assert!(state.offer.is_none());
    assert!(!state.marked);
}

/// The brief: until Björn's keys are committed, nothing is checked or downloaded.
#[tokio::test]
async fn a_build_without_keys_is_not_set_up_and_checks_nothing() {
    let rig = Rig::new();
    let updater = Updater::new(
        rig.config("3.2.9-ux27", nsis(), Keys::default()),
        rig.host.clone(),
    );
    let state = updater.state();
    assert!(!state.set_up);
    assert!(!state.checks_itself);
    assert_eq!(state.next_check_at, None);
    assert!(updater.check().await.is_err());
    assert!(updater.download().is_err());
    assert!(rig.fetch.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn changing_the_channel_drops_the_other_channels_offer_and_download() {
    let rig = Rig::new();
    let updater = rig.updater("3.2.9-ux27", nsis());
    updater.check().await.unwrap();
    rig.downloaded();
    let updater = rig.updater("3.2.9-ux27", nsis());
    assert_eq!(updater.state().phase, Phase::Ready);
    // The pre-release channel answers nothing with a manifest.
    rig.fetch
        .answers
        .lock()
        .unwrap()
        .insert(LIST.to_owned(), "[]".to_owned());
    let state = updater.set(None, Some(Channel::PreReleases)).await.unwrap();
    assert_eq!(state.channel, Channel::PreReleases);
    assert!(state.offer.is_none() && state.newest.is_none());
    assert_eq!(state.phase, Phase::Idle);
    assert!(
        !rig.cache().join(FILE).exists(),
        "the other channel's download is gone"
    );
    assert_eq!(
        rig.fetch.asked.lock().unwrap().last().map(String::as_str),
        Some(LIST)
    );
}

#[tokio::test]
async fn only_one_check_runs_at_a_time() {
    let rig = Rig::new();
    let updater = rig.updater("3.2.9-ux27", nsis());
    let held = rig.fetch.gate.lock().await;
    let (a, b) = (Arc::clone(&updater), Arc::clone(&updater));
    let first = tokio::spawn(async move { a.check().await });
    until("the first check asks", || {
        !rig.fetch.asked.lock().unwrap().is_empty()
    })
    .await;
    let second = tokio::spawn(async move { b.check().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(held);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(rig.fetch.asked.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn update_install_with_nothing_downloaded_is_refused() {
    let rig = Rig::new();
    let updater = rig.updater("3.2.9-ux27", nsis());
    assert!(updater.install().is_err());
    updater.check().await.unwrap();
    assert!(updater.install().is_err(), "an offer is not a download");
    assert!(rig.host.did().is_empty());
}

/// Decisions D12, and D11's order: the backup, then the attempt, then the installer handed to
/// the exit and the quit. Mutant: start the installer in `update_install`, before `exit`.
#[tokio::test]
async fn the_backup_is_taken_before_the_installer_is_staged() {
    let rig = Rig::new();
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    let updater = rig.updater("3.2.9-ux27", nsis());
    let before = STARTED.lock().unwrap().len();
    updater.install().unwrap();
    until("the quit", || rig.host.did().len() == 2).await;
    assert_eq!(rig.host.did(), ["backup", "quit Exit"]);
    assert_eq!(
        STARTED.lock().unwrap().len(),
        before,
        "nothing starts before the exit"
    );
    let launch = updater
        .take_launch()
        .expect("the installer is handed to the exit");
    assert!(launch.program.ends_with(FILE));
    assert!(launch.program.is_file(), "the staged copy is there");
    assert!(
        !launch.program.starts_with(rig.cache()),
        "the installer runs from the private copy"
    );
    // The private folder is in the attempt, and the next start removes it.
    let private = launch.program.parent().unwrap().to_path_buf();
    let attempt = rig.updater("3.2.9-ux27", nsis());
    // The next start, still the old version: the attempt reads as not installed.
    let last = attempt.state().last.unwrap();
    assert!(!last.installed);
    assert_eq!(last.backup.as_deref(), Some("knobas-backup.knobas"));
    assert!(
        !private.exists(),
        "the next start removed the private folder"
    );
}

/// Mutant: ignore the backup's error.
#[tokio::test]
async fn a_failed_backup_stops_an_update_that_changes_the_schema() {
    let rig = Rig::new();
    *rig.host.backup.lock().unwrap() = Some(Backup::Failed("pg_dump refused".to_owned()));
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    let updater = rig.updater("3.2.9-ux27", nsis());
    updater.install().unwrap();
    until("the failure", || {
        matches!(updater.state().phase, Phase::Failed { .. })
    })
    .await;
    assert!(matches!(
        updater.state().phase,
        Phase::Failed {
            stage: Stage::Backup,
            ..
        }
    ));
    assert_eq!(rig.host.did(), ["backup"], "nothing closed");
    assert!(updater.take_launch().is_none());
}

#[tokio::test]
async fn a_failed_backup_stops_an_update_that_changes_the_schema_and_not_one_that_does_not() {
    let rig = Rig::new();
    rig.fetch
        .answers
        .lock()
        .unwrap()
        .insert(LATEST.to_owned(), manifest(101));
    *rig.host.backup.lock().unwrap() = Some(Backup::Failed("pg_dump refused".to_owned()));
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    let updater = rig.updater("3.2.9-ux27", nsis());
    updater.install().unwrap();
    until("the quit", || rig.host.did().len() == 2).await;
    assert_eq!(rig.host.did(), ["backup", "quit Exit"]);
    // The next start takes the private folder away.
    drop(rig.updater("3.2.9-ux27", nsis()));
}

/// A Mac copy whose swap fails: the old app stays, nothing is closed, the attempt is taken back.
#[tokio::test]
async fn an_install_that_fails_in_step_4_closes_nothing() {
    let rig = Rig::new();
    let mac = Standing::InPlace {
        kind: InstallKind::App,
    };
    // A Mac copy reads the Mac's entry.
    let text = manifest(102).replace("windows-x86_64-nsis", "darwin-aarch64-app");
    rig.fetch
        .answers
        .lock()
        .unwrap()
        .insert(LATEST.to_owned(), text);
    rig.updater("3.2.9-ux27", mac).check().await.unwrap();
    rig.downloaded();
    let mut config = rig.config("3.2.9-ux27", mac, keys());
    config.installed = Some(rig.root.path().join("Applications/knobas.app"));
    let updater = Updater::new(config, rig.host.clone());
    updater.install().unwrap();
    until("the failure", || {
        matches!(updater.state().phase, Phase::Failed { .. })
    })
    .await;
    assert!(matches!(
        updater.state().phase,
        Phase::Failed {
            stage: Stage::Install,
            ..
        }
    ));
    assert_eq!(rig.host.did(), ["backup"], "nothing closed");
    assert!(
        Files::new(rig.root.path().join("data/update"))
            .state()
            .attempt
            .is_none()
    );
}

/// Three cases: the new version runs, the old one still does, a third one does.
#[tokio::test]
async fn the_attempts_outcome_is_read_from_the_running_version() {
    let attempt = |rig: &Rig| {
        let files = Files::new(rig.root.path().join("data/update"));
        let mut state = files.state();
        state.attempt = Some(Attempt {
            from: "3.2.9-ux27".to_owned(),
            to: "3.2.9-ux28".to_owned(),
            at: Utc::now(),
            backup: Some("b.knobas".to_owned()),
            staged: None,
            aside: None,
        });
        files.save_state(&state).unwrap();
    };

    let rig = Rig::new();
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    attempt(&rig);
    let state = rig.updater("3.2.9-ux28", nsis()).state();
    assert!(state.just_updated);
    assert!(state.last.as_ref().unwrap().installed);
    assert!(!rig.cache().exists(), "the download is removed");

    let rig = Rig::new();
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    attempt(&rig);
    let state = rig.updater("3.2.9-ux27", nsis()).state();
    assert!(!state.just_updated);
    assert!(!state.last.as_ref().unwrap().installed);
    assert_eq!(state.phase, Phase::Ready, "the download is kept");
    assert!(state.marked, "the dot stays");

    let rig = Rig::new();
    attempt(&rig);
    let state = rig.updater("3.2.10", nsis()).state();
    assert!(state.last.is_none() && !state.just_updated);
    assert!(
        Files::new(rig.root.path().join("data/update"))
            .state()
            .attempt
            .is_none()
    );
}

/// Mutant: show the dot from `offer` alone, ignoring *Later*.
#[tokio::test]
async fn later_takes_the_dot_off_and_keeps_the_download() {
    let rig = Rig::new();
    rig.updater("3.2.9-ux27", nsis()).check().await.unwrap();
    rig.downloaded();
    let updater = rig.updater("3.2.9-ux27", nsis());
    let state = updater.later();
    assert!(!state.marked);
    assert!(state.offer.is_some());
    assert_eq!(state.phase, Phase::Ready);
    assert!(rig.cache().join(FILE).exists());
    // The next start marks it again.
    assert!(rig.updater("3.2.9-ux27", nsis()).state().marked);
}

#[tokio::test]
async fn a_package_copy_is_offered_and_refuses_to_download() {
    let rig = Rig::new();
    let deb = Standing::ByHand {
        why: ByHandWhy::Package,
        kind: None,
    };
    let updater = rig.updater("3.2.9-ux27", deb);
    let state = updater.check().await.unwrap();
    assert!(state.offer.is_some());
    assert!(updater.download().is_err());
}

/// The order of the exit: the installer starts after `shutdown_app` stopped PostgreSQL.
#[tokio::test(flavor = "multi_thread")]
async fn at_exit_starts_the_handed_installer_once() {
    use tauri::Manager as _;
    let rig = Rig::new();
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let updater = rig.updater("3.2.9-ux27", nsis());
    app.manage(Arc::clone(&updater));
    *updater.launch.lock().unwrap() = Some(Launch {
        program: PathBuf::from("the-exit-test-installer.exe"),
        args: Vec::new(),
    });
    at_exit(app.handle());
    at_exit(app.handle());
    let started = STARTED.lock().unwrap();
    assert_eq!(
        started
            .iter()
            .filter(|p| p.as_str() == "the-exit-test-installer.exe")
            .count(),
        1
    );
}

/// What the log file would hold: the lines a test's own subscriber wrote.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Captured {
    /// Capture this thread's info lines, and those of the tasks it spawns, until the guard drops.
    fn start() -> (Self, tracing::subscriber::DefaultGuard) {
        let captured = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .finish();
        (captured, tracing::subscriber::set_default(subscriber))
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// A rig whose manifest offers `body` from a local server, at an address that carries a token.
async fn served(body: &[u8]) -> (wiremock::MockServer, Rig) {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(method("GET"))
        .and(path(format!("/releases/download/v3.2.9-ux28/{FILE}")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&server)
        .await;
    let rig = Rig::new();
    let text = manifest(102).replace(
        &format!("https://github.com/BFoerschner/knobas/releases/download/v3.2.9-ux28/{FILE}"),
        &format!(
            "{}/releases/download/v3.2.9-ux28/{FILE}?token=the-secret-in-the-address",
            server.uri()
        ),
    );
    rig.fetch
        .answers
        .lock()
        .unwrap()
        .insert(LATEST.to_owned(), text);
    (server, rig)
}

/// The ux30 boot check: the updater wrote no line for its download or its signature check. Now
/// the download's start and end (size, time), the signature's result (on landing and again
/// before the install) and the hand-off to the quit are each a line, and no address is.
#[tokio::test]
async fn the_download_its_signature_and_the_hand_off_are_logged_without_the_address() {
    let (_server, rig) = served(BODY).await;
    let updater = rig.updater("3.2.9-ux27", nsis());
    let (captured, _guard) = Captured::start();
    updater.check().await.unwrap();
    updater.download().unwrap();
    until("the download is ready", || {
        updater.state().phase == Phase::Ready
    })
    .await;
    updater.install().unwrap();
    until("the quit", || rig.host.did().len() == 2).await;

    let log = captured.text();
    let line = |what: &str| {
        log.lines()
            .find(|line| line.contains(what))
            .unwrap_or_else(|| panic!("no line says {what:?} in:\n{log}"))
            .to_owned()
    };
    let start = line("the update's download starts");
    assert!(start.contains("version=3.2.9-ux28"), "{start}");
    assert!(start.contains(&format!("file={FILE}")), "{start}");
    let end = line("the update's download ended");
    assert!(
        end.contains(&format!("bytes=Some({})", BODY.len())),
        "{end}"
    );
    assert!(end.contains("ms="), "{end}");
    line("the update's signature checks out against a trusted key");
    line("the update's signature checks out again before the install");
    let hand_off = line("the update is handed to the quit");
    assert!(hand_off.contains("to=3.2.9-ux28"), "{hand_off}");
    assert!(hand_off.contains("how=Exit"), "{hand_off}");
    assert!(!log.contains("the-secret-in-the-address"), "{log}");
    assert!(!log.contains("http"), "no address in the log:\n{log}");
}

/// A download whose bytes the signature does not cover says so in the log.
#[tokio::test]
async fn a_download_whose_signature_does_not_check_out_is_logged() {
    let mut forged = BODY.to_vec();
    forged[0] ^= 0xff;
    let (_server, rig) = served(&forged).await;
    let updater = rig.updater("3.2.9-ux27", nsis());
    let (captured, _guard) = Captured::start();
    updater.check().await.unwrap();
    updater.download().unwrap();
    until("the download fails", || {
        matches!(updater.state().phase, Phase::Failed { .. })
    })
    .await;
    let log = captured.text();
    assert!(
        log.lines().any(|line| line.contains("WARN")
            && line.contains("the update's signature does not check out")),
        "{log}"
    );
    assert!(!log.contains("checks out against a trusted key"), "{log}");
}

#[test]
fn an_address_in_a_transfer_error_keeps_only_its_host() {
    assert_eq!(
        without_urls(
            "error sending request for url (https://objects.example.com/a/b?X-Amz-Signature=abc)"
        ),
        "error sending request for url (https://objects.example.com/…"
    );
    assert_eq!(
        without_urls("http://user:pw@127.0.0.1:9/x?token=t refused"),
        "http://127.0.0.1:9/… refused"
    );
    assert_eq!(without_urls("timed out"), "timed out");
}
