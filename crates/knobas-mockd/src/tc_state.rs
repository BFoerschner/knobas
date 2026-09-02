//! The TeamCity half of [`MockState`](crate::state::MockState): the Tidewater
//! fixture's builds, transcribed into TeamCity shapes and made mutable.
//!
//! ## The transcription rules
//!
//! Two rows here are a **cross-stream contract**, not an internal choice —
//! stream C derives its own expectations from the same
//! `fixtures/tidewater/work.json` and fails loudly on a mismatch:
//!
//! * **`build.id` = `build.number` = the fixture's `num`** (412, 1187, 1188),
//!   `id` as an integer and `number` as the same value spelled as a string,
//!   which is TeamCity's own typing. The ids are monotonic, which is the only
//!   reason `sinceBuild` means anything.
//! * **`buildType.id` = the fixture's `cfg`**, byte-for-byte: `Payout_Build`,
//!   `Payout_IntegrationTests`, `Ledger_Deploy_Staging`. No case folding, no
//!   separator normalisation.
//!
//! Everything else is mockd's internal business:
//!
//! | TeamCity | Rule |
//! |---|---|
//! | `projectId` / `projectName` | the `cfg` prefix before the first `_` (`Payout`, `Ledger`). |
//! | buildType `name` | the `cfg` after the first `_`, remaining `_` → space (`Build`, `IntegrationTests`, `Deploy Staging`). |
//! | `state` | `running` ⇒ `running`; `failed`/`success` ⇒ `finished`. |
//! | `status` | `failed` ⇒ `FAILURE`; `success` and `running` ⇒ `SUCCESS` (a running build reports the status *so far*). |
//! | `startDate` | the fixture's `when`. `finishDate` = `when + duration` (`"4 m 12 s"` parsed) or `when + 60 s` when the fixture gives none; absent while running. |
//! | `statusText` | the first line of `log` where the fixture has one, else `"Success"` / `"Running"`. |
//! | `running-info` | `percentageComplete` from the fixture's `step` (`step 3/5 …` ⇒ 60), `currentStageText` = the step verbatim. |
//! | buildType `description` | `None`: the dataset describes no configuration, and mockd does not invent prose any more than it invents a triggerer. `MockState::describe_build_type` is how a test that needs one gets one. |
//! | `triggered` | the fixture's `triggered_by` resolved through `fixture().person`: `type: "user"` with that person's `username`/`name`, or `type: "vcs"` and no `user` where the fixture names nobody. |
//! | `canceled` / `failedToStart` | `false`: the dataset has no vocabulary for either, and a mock that invented one would put a cancellation into every test that reads the fixture. [`MockState::cancel_build`](crate::state::MockState::cancel_build) and [`MockState::fail_build_to_start`](crate::state::MockState::fail_build_to_start) are how a test gets one. |
//! | `defaultBranch` | [`is_default_branch`] of the fixture's `branch`: `main` is the default branch of every configuration (the VCS roots `testenv/seed-teamcity-builds.sh` creates say `refs/heads/main`), so 412 is a default-branch build and 1187/1188 on `feature/PAY-231-sepa-retry` are not. What TeamCity's default filter reads (issue #266). |

use chrono::{DateTime, Duration, Utc};
use knobas_source_mock::fixture;

/// The default branch of every fixture configuration, as the seeded VCS roots
/// spell it without the `refs/heads/` prefix.
pub const DEFAULT_BRANCH: &str = "main";

/// Is `name` the default branch, in any of the spellings a caller may use?
///
/// `main`, `refs/heads/main`, and TeamCity's own `<default>` -- the name a
/// real server shows for a build queued with no branch at all -- are one
/// branch. Everything else is a feature branch the default filter hides
/// (issue #266).
pub fn is_default_branch(name: &str) -> bool {
    let name = name.strip_prefix("refs/heads/").unwrap_or(name);
    name == DEFAULT_BRANCH || name == "<default>"
}

/// TeamCity timestamps are `20260822T114500+0000` — compact, no separators.
pub const TC_DATE_FMT: &str = "%Y%m%dT%H%M%S%z";

/// Renders `t` the way TeamCity does.
pub fn tc_date(t: DateTime<Utc>) -> String {
    t.format(TC_DATE_FMT).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcStatus {
    Success,
    Failure,
    /// What TeamCity reports for a **canceled** build, which has no state of
    /// its own: a canceled build is `state: "finished"` with this status, and
    /// `statusText` is where the server says which -- 20 of 20 sampled
    /// read-only on JetBrains' public instance. Transcribed rather than
    /// improved on; a mock that gave cancellation its own state would teach an
    /// adapter a shape no TeamCity serves.
    Unknown,
}

impl TcStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failure => "FAILURE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcState {
    Queued,
    Running,
    Finished,
}

impl TcState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcBuildType {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub project_name: String,
    /// Prose a human wrote about the configuration. `None` for every fixture
    /// configuration -- the dataset gives none, and mockd does not invent one
    /// -- so a test that needs one sets it with
    /// [`describe_build_type`](crate::state::MockState::describe_build_type),
    /// the same way stream C's window test creates the second build the
    /// fixture does not have.
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcBuild {
    pub id: u64,
    pub build_type_id: String,
    pub number: String,
    pub status: TcStatus,
    pub state: TcState,
    pub branch_name: String,
    pub start_date: DateTime<Utc>,
    pub finish_date: Option<DateTime<Utc>>,
    pub status_text: String,
    pub percentage_complete: Option<u8>,
    pub current_stage_text: Option<String>,
    /// [`knobas_source_mock::Person::id`] of whoever started it, straight from
    /// the fixture. `None` is a build the dataset attributes to no person,
    /// which TeamCity serves as a VCS trigger with no `triggered.user` --
    /// never an invented one.
    pub triggered_by: Option<String>,
    /// Somebody stopped this build. **Not a state**: TeamCity finishes it with
    /// [`TcStatus::Unknown`], and this flag is what its default filter and the
    /// `canceled:` locator dimension read (issue #105).
    ///
    /// No fixture build is one -- the dataset describes a company's work, not
    /// its cancellations -- so
    /// [`cancel_build`](crate::state::MockState::cancel_build) is how a test
    /// gets one, the same way `describe_build_type` supplies the description
    /// the fixture does not have.
    pub canceled: bool,
    /// The build never ran: no agent, a failed dependency, a bad
    /// configuration. TeamCity finishes it `FAILURE` and serves
    /// `failedToStart: true`, and its default filter hides it exactly as it
    /// hides a canceled build.
    pub failed_to_start: bool,
    /// Did this build run on its configuration's default branch? Served as
    /// `defaultBranch`, and what the default filter's third facet reads: a
    /// locator whose state set includes `finished` answers only default-branch
    /// builds unless `branch:default:any` (or `defaultFilter:false`) is on it
    /// -- measured on a TeamCity we own, 2026-09-02 (issue #266).
    pub default_branch: bool,
}

/// The build configurations the fixture's builds refer to, ascending by id.
///
/// Ascending by id and not by first appearance: the fixture lists builds newest
/// first, so first-appearance order would make the response depend on a detail
/// of the dataset's *presentation* rather than its content.
pub(crate) fn build_types() -> Vec<TcBuildType> {
    let mut ids: Vec<String> = fixture().builds.iter().map(|b| b.cfg.clone()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|id| {
            let (project, rest) = id.split_once('_').unwrap_or((id.as_str(), ""));
            TcBuildType {
                name: rest.replace('_', " "),
                project_id: project.to_owned(),
                project_name: project.to_owned(),
                description: None,
                id,
            }
        })
        .collect()
}

/// The fixture's builds, ascending by id.
pub(crate) fn builds() -> Vec<TcBuild> {
    let mut out: Vec<TcBuild> = fixture().builds.iter().map(transcribe).collect();
    out.sort_by_key(|b| b.id);
    out
}

fn transcribe(b: &knobas_source_mock::Build) -> TcBuild {
    let running = b.status == "running";
    let state = if running {
        TcState::Running
    } else {
        TcState::Finished
    };
    let status = if b.status == "failed" {
        TcStatus::Failure
    } else {
        TcStatus::Success
    };
    let status_text = b
        .log
        .as_deref()
        .and_then(|l| l.lines().next())
        .map(str::to_owned)
        .unwrap_or_else(|| if running { "Running" } else { "Success" }.to_owned());
    let finish_date = (!running).then(|| {
        b.when
            + b.duration
                .as_deref()
                .and_then(parse_duration)
                .unwrap_or_else(|| Duration::seconds(60))
    });
    TcBuild {
        id: u64::from(b.num),
        build_type_id: b.cfg.clone(),
        number: b.num.to_string(),
        status,
        state,
        branch_name: b.branch.clone(),
        start_date: b.when,
        finish_date,
        status_text,
        percentage_complete: running.then(|| percentage(b.step.as_deref())).flatten(),
        current_stage_text: running.then(|| b.step.clone()).flatten(),
        triggered_by: b.triggered_by.clone(),
        // The fixture's vocabulary is `running`/`failed`/`success`; neither
        // class exists in it, and inventing one here would put a cancellation
        // into every test that reads the dataset.
        canceled: false,
        failed_to_start: false,
        default_branch: is_default_branch(&b.branch),
    }
}

/// `"4 m 12 s"` ⇒ 252 s. `None` for anything the fixture has not used, so a new
/// spelling is a visible `None` rather than a silently wrong number.
fn parse_duration(raw: &str) -> Option<Duration> {
    let mut total = 0i64;
    let mut pending: Option<i64> = None;
    for tok in raw.split_whitespace() {
        match tok {
            "h" => total += pending.take()? * 3600,
            "m" => total += pending.take()? * 60,
            "s" => total += pending.take()?,
            n => {
                if pending.replace(n.parse().ok()?).is_some() {
                    return None;
                }
            }
        }
    }
    pending.is_none().then(|| Duration::seconds(total))
}

/// `"step 3/5 `cargo test`"` ⇒ 60. Derived rather than hard-coded so a fixture
/// that moves the build along moves the percentage with it.
fn percentage(step: Option<&str>) -> Option<u8> {
    let (done, total) = step?
        .split_whitespace()
        .find_map(|t| t.split_once('/'))
        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))?;
    (total > 0).then(|| u8::try_from(done * 100 / total).unwrap_or(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_branch_is_main_in_every_spelling() {
        assert!(is_default_branch("main"));
        assert!(is_default_branch("refs/heads/main"));
        assert!(is_default_branch("<default>"));
        assert!(!is_default_branch("feature/PAY-231-sepa-retry"));
        assert!(!is_default_branch("refs/heads/fix/PAY-228-partial-refund-drift"));
        let flags: Vec<(u64, bool)> = builds().iter().map(|b| (b.id, b.default_branch)).collect();
        assert_eq!(flags, [(412, true), (1187, false), (1188, false)]);
    }

    #[test]
    fn durations_parse_or_refuse() {
        assert_eq!(parse_duration("4 m 12 s"), Some(Duration::seconds(252)));
        assert_eq!(parse_duration("1 h"), Some(Duration::hours(1)));
        assert_eq!(parse_duration("4m12s"), None);
        assert_eq!(parse_duration("12"), None);
    }

    #[test]
    fn percentages_come_from_the_step() {
        assert_eq!(percentage(Some("step 3/5 `cargo test`")), Some(60));
        assert_eq!(percentage(Some("compiling")), None);
        assert_eq!(percentage(None), None);
    }
}
