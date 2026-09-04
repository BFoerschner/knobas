//! What the app crate's live suites share: a scheduler wired the way the app
//! wires it, and the day the digest is read for.
//!
//! # Why this is one module and not four copies
//!
//! `tests/atlassian_live.rs` wrote the argument, about `day_window`, before
//! there were three of it:
//!
//! > Free rather than a closure re-declared inside each digest test: two of
//! > them carried the same ten lines and this branch's own would have been a
//! > third, and two digest tests that disagreed about where a day begins would
//! > both stay green while measuring different things.
//!
//! The hazard does not stop at the file boundary. By #397 `day_window` stood in
//! `atlassian_live.rs`, `start_work_live.rs` and `teamcity_seeded_live.rs`, and
//! the `Connections`/`Quiet`/`Ending`/`sync` scaffolding in all four live suites
//! including `confluence_live.rs` -- so four suites could disagree about where a
//! day begins, or about what "the run ended" means, and all four would stay
//! green while measuring different things. Issue #398 is that sentence applied
//! to itself.
//!
//! `crates/knobas-source-gitea/tests/live_env/mod.rs` is the precedent for the
//! shape: a directory under `tests/`, which cargo does **not** pick up as a test
//! target of its own, declared `mod live_digest;` by each suite that wants it.
//!
//! # What is *not* here
//!
//! **Failure messages that name a source stay in the suite that knows the
//! source.** `teamcity_seeded_live.rs` explains an unattributed build by naming
//! `triggered.user.username`, the field its adapter maps; `start_work_live.rs`
//! has no such field to name. One flattened message would cost the reader the
//! reason, so the mirror-attribution assertion stays inline in both. Only
//! [`on_digest`] is shared, and it carries the subject noun through so its own
//! message stays each suite's own.
//!
//! **`atlassian_live.rs`'s own two digest assertions stay inline**, and they
//! are the one place the shape [`on_digest`] carries is left standing twice.
//! They match on three fields where it matches on four: neither of them
//! constrains `kind` at all, so routing them through [`on_digest`] would newly
//! assert what their `kind` *is*. That is a change to what a test claims, which
//! is the one thing a move may not do -- so the duplication is the cheaper of
//! the two. If either ever grows a `kind`, it belongs here.
//!
//! Nor is anything here that only one suite has: `atlassian_live.rs` keeps its
//! `Events` sink (it collects health events, which is a claim, not scaffolding)
//! and its `backfill`, which triggers a backfill rather than a manual run and
//! takes its sink from [`Ending::for_run`] like everything else.

// Compiled separately into each live test binary, and each uses a different
// part of it -- so without this, `clippy --all-targets -- -D warnings` fails on
// whatever one of them happens not to call.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::standup::DigestLine;
use knobas_sync::SyncTrigger;
use knobas_sync::scheduler::{RunConnections, SyncEvents};

/// Connections a run gets: the scratch database's own, not the shared one's.
pub struct Connections(pub knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Events nobody is listening for. The scheduler reports; there is no window.
pub struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// A progress sink that reports one thing: the run reached an end, either one.
pub struct Ending {
    done: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Ending {
    /// A sink for one run, and the receiver that fires when that run ends.
    ///
    /// The two are handed out together, and the field between them is private,
    /// because they are one thing: a sink holding another run's sender reports
    /// the wrong run's ending, and a caller assembling the pair by hand is a
    /// fifth copy of the wiring this module exists to have one of.
    pub fn for_run() -> (Arc<Self>, tokio::sync::oneshot::Receiver<()>) {
        let (done, wait) = tokio::sync::oneshot::channel();
        (
            Arc::new(Self {
                done: Mutex::new(Some(done)),
            }),
            wait,
        )
    }
}

impl knobas_sync::progress::ProgressSink for Ending {
    fn report(&self, progress: knobas_sync::progress::SyncProgress) {
        use knobas_sync::progress::SyncPhase;
        if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
            return;
        }
        if let Some(sender) = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
    }
}

/// Sync one source and wait for the run to end, whichever way it ends.
pub async fn sync(state: &SourcesState, source: &str) {
    let (sink, wait) = Ending::for_run();
    state
        .scheduler
        .trigger(source, SyncTrigger::Manual, Some(sink))
        .await
        .expect("the run starts");
    wait.await.expect("the run reports its ending");
}

/// One whole UTC day, as the digest's readers ask for it.
///
/// Which day it is where the reader sits is a fact only the webview holds, so
/// `standup_digest_inner` is handed the windows rather than working them out
/// (`knobas_app::time::day`).
pub fn day_window(on: chrono::NaiveDate) -> knobas_app::time::week::DayWindow {
    knobas_app::time::week::DayWindow {
        day: on,
        from: on.and_hms_opt(0, 0, 0).expect("midnight").and_utc(),
        to: on
            .succ_opt()
            .expect("the next day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    }
}

/// Assert that `lines` hold the item `address` names, from `source`, of `kind`,
/// under `verb` -- the four-tuple containment issue #389's two live digest
/// tests make.
///
/// Four fields rather than the entity id alone, because three of them are what
/// the view renders the line *as*: a line that reached the list under the wrong
/// source or the wrong verb is a different claim about the same item, and an
/// assertion that only found the id would pass on it.
///
/// `what` is the suite's own noun for the item ("the pull request this flow
/// opened", "fixture build ..."), so a failure still says which fixture is
/// missing and not merely which id.
pub fn on_digest(
    lines: &[DigestLine],
    what: &str,
    day: chrono::NaiveDate,
    address: &str,
    source: &str,
    kind: &str,
    verb: &str,
) {
    let listed: Vec<(Option<&str>, &str, Option<&str>, &str)> = lines
        .iter()
        .map(|line| {
            (
                line.entity_id.as_deref(),
                line.source.as_str(),
                line.kind.as_deref(),
                line.verb.as_str(),
            )
        })
        .collect();
    assert!(
        listed.contains(&(Some(address), source, Some(kind), verb)),
        "{what} is not on the digest for {day}, which is the day the mirror dates it on: \
         {listed:?}"
    );
}
