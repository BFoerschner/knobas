//! The source views this adapter draws (ADR-0020, spec #660).
//!
//! One kind so far: a **build configuration** (#664) opens on its overview --
//! where it lives, whether it is paused, how its recent builds went, and the
//! builds themselves -- with *Parameters*, *Steps*, *Triggers* and *VCS roots*
//! beside it. A build answers `None` until its own ticket (#665), and its
//! detail draws the mirror's generic projection as it always has.
//!
//! Two requests draw the whole view: the configuration, with one explicit
//! `fields=` that answers every tab but the builds, and its recent builds.
//! A tab asked for on its own reads only what it needs. Every read is live and
//! nothing is stored (rule 1); a refusal is the error it maps to, so the
//! detail says *Showing the mirror* and why (rule 2).
//!
//! **A password parameter's value never enters a view.** TeamCity masks it on
//! every read, and the view does not rely on that: a parameter whose spec says
//! `password` is drawn with an empty value whatever the server sent.

use chrono::{DateTime, Utc};
use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, Setting, SourceView, Tone, ViewRow, ViewSection};

use crate::client::HttpRest;
use crate::map;
use crate::rest::{Build, ConfigRecord, Properties, Step, Trigger, VcsRootEntry, parse_ts};

/// A build configuration's tabs, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Parameters,
    Steps,
    Triggers,
    VcsRoots,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Overview,
        Tab::Parameters,
        Tab::Steps,
        Tab::Triggers,
        Tab::VcsRoots,
    ];

    /// The section id a deferred read would name; stable.
    fn id(self) -> &'static str {
        match self {
            Tab::Overview => "overview",
            Tab::Parameters => "parameters",
            Tab::Steps => "steps",
            Tab::Triggers => "triggers",
            Tab::VcsRoots => "vcs_roots",
        }
    }
}

/// How many builds *Recent builds* lists, and the success rate is over.
const RECENT_BUILDS: u32 = 20;

/// The [`WriteOp`](knobas_source::WriteOp) identifier a parameter's edit will
/// go through (spec #660). Named on the settings block now so the block's
/// shape does not change when #668 makes values editable; every value is
/// `editable: false` until then, so nothing can dispatch it.
const WRITE_OP_SET_PARAMETER: &str = "set_parameter";

/// What the view needs from the source: its namespace, the base URL a build's
/// page is composed from, and the client.
pub(crate) struct Viewer<'a> {
    pub source_id: &'a str,
    pub base_url: &'a str,
    pub rest: &'a HttpRest,
}

/// The view of `key` (the id's key half), or `None` for a kind this adapter
/// draws nothing for.
///
/// # Errors
///
/// [`SourceError::Protocol`] for a key that is neither kind, a configuration
/// id that could not have come from TeamCity, or a section the view does not
/// have; otherwise whatever TeamCity's answer maps to.
pub(crate) async fn draw(
    viewer: &Viewer<'_>,
    entity: &str,
    key: &str,
    section: Option<&str>,
) -> Result<Option<SourceView>, SourceError> {
    if let Some(id) = key.strip_prefix("buildType:") {
        let id = config_id(entity, id)?;
        return build_config(viewer, entity, id, section).await.map(Some);
    }
    if key.starts_with("build:") {
        return Ok(None);
    }
    Err(SourceError::protocol(format!(
        "teamcity: {entity:?} is neither a build nor a build configuration"
    )))
}

/// A configuration id as TeamCity issues them -- letters, digits and `_` --
/// and so nothing that could leave the path segment or the locator it goes in.
fn config_id<'k>(entity: &str, id: &'k str) -> Result<&'k str, SourceError> {
    let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then_some(id).ok_or_else(|| {
        SourceError::protocol(format!(
            "teamcity: {entity:?} does not name a build configuration TeamCity could have issued"
        ))
    })
}

async fn build_config(
    viewer: &Viewer<'_>,
    entity: &str,
    id: &str,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let wanted = match section {
        None => None,
        Some(asked) => Some(Tab::ALL.into_iter().find(|t| t.id() == asked).ok_or_else(|| {
            SourceError::protocol(format!(
                "teamcity: a build configuration's view has no section {asked:?} (asked for \
                 {entity:?})"
            ))
        })?),
    };
    let record = viewer.rest.build_config(id).await?;
    let builds = if matches!(wanted, None | Some(Tab::Overview)) {
        viewer.rest.recent_builds(id, RECENT_BUILDS).await?
    } else {
        Vec::new()
    };
    let now = Utc::now();
    let sections = Tab::ALL
        .into_iter()
        .filter(|t| wanted.is_none_or(|w| w == *t))
        .map(|t| match t {
            Tab::Overview => overview(viewer, &record, &builds, now),
            Tab::Parameters => parameters(entity, &record),
            Tab::Steps => steps(&record),
            Tab::Triggers => triggers(&record),
            Tab::VcsRoots => vcs_roots(&record),
        })
        .collect();
    Ok(SourceView { sections })
}

fn tab(tab: Tab, title: &str, count: Option<usize>, blocks: Vec<Block>) -> ViewSection {
    ViewSection {
        id: tab.id().to_owned(),
        title: title.to_owned(),
        count: count.map(|n| n as u64),
        deferred: false,
        blocks,
    }
}

fn fact(label: &str, value: impl Into<String>, tone: Option<Tone>) -> Fact {
    Fact {
        label: label.to_owned(),
        value: value.into(),
        tone,
    }
}

// --- Overview -----------------------------------------------------------

fn overview(
    viewer: &Viewer<'_>,
    record: &ConfigRecord,
    builds: &[Build],
    now: DateTime<Utc>,
) -> ViewSection {
    let mut facts = Vec::new();
    if let Some(project) =
        nonblank(record.project_name.as_deref()).or_else(|| nonblank(record.project_id.as_deref()))
    {
        facts.push(fact("Project", project, None));
    }
    if record.paused == Some(true) {
        facts.push(fact("Paused", "yes", Some(Tone::Warn)));
    }
    if let Some(last) = builds.iter().find(|b| Outcome::of(b).finished()) {
        let o = Outcome::of(last);
        facts.push(fact(
            "Last build",
            format!("{} {}", number(last), o.word()),
            Some(o.tone()),
        ));
    }
    // Over the finished builds that say something about health: a canceled
    // one was stopped, not judged.
    let judged: Vec<_> = builds
        .iter()
        .map(Outcome::of)
        .filter(|o| o.finished() && *o != Outcome::Canceled)
        .collect();
    if !judged.is_empty() {
        let passed = judged.iter().filter(|o| **o == Outcome::Success).count();
        facts.push(fact(
            "Success rate",
            format!(
                "{} % of {} finished",
                passed * 100 / judged.len(),
                judged.len()
            ),
            None,
        ));
    }

    let mut blocks = vec![Block::Facts { facts }];
    if let Some(description) = nonblank(record.description.as_deref()) {
        blocks.push(Block::Text {
            text: description.to_owned(),
            markdown: false,
        });
    }
    if !builds.is_empty() {
        blocks.push(Block::Entities {
            title: Some("Recent builds".to_owned()),
            rows: builds
                .iter()
                .map(|b| build_row(viewer, &record.id, b, now))
                .collect(),
        });
    }
    tab(Tab::Overview, "Overview", None, blocks)
}

/// What happened to a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Queued,
    Running,
    Success,
    Failed,
    Error,
    Canceled,
}

impl Outcome {
    /// TeamCity's state and status, by the rule `map::status_element` writes
    /// down for the mirror: a finished `UNKNOWN` is a canceled build (issue
    /// #105). That function renders a line for the search index; this one
    /// picks a word and a tone, so the rule is restated rather than shared.
    fn of(build: &Build) -> Outcome {
        match (build.state.as_deref(), build.status.as_deref()) {
            (Some("running"), _) => Outcome::Running,
            (Some("queued"), _) => Outcome::Queued,
            (_, Some("SUCCESS")) => Outcome::Success,
            (_, Some("FAILURE")) => Outcome::Failed,
            (_, Some("ERROR")) => Outcome::Error,
            _ => Outcome::Canceled,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Outcome::Queued => "queued",
            Outcome::Running => "running",
            Outcome::Success => "success",
            Outcome::Failed => "failed",
            Outcome::Error => "error",
            Outcome::Canceled => "canceled",
        }
    }

    /// Colour only where something is wrong or moving.
    fn tone(self) -> Tone {
        match self {
            Outcome::Running => Tone::Info,
            Outcome::Success => Tone::Ok,
            Outcome::Failed | Outcome::Error => Tone::Err,
            Outcome::Queued | Outcome::Canceled => Tone::Muted,
        }
    }

    fn finished(self) -> bool {
        !matches!(self, Outcome::Queued | Outcome::Running)
    }
}

/// `#1187` -- the build's number, which is what TeamCity's own UI shows, or
/// its id where it has none yet (a queued build takes its number on start).
fn number(build: &Build) -> String {
    match nonblank(build.number.as_deref()) {
        Some(n) => format!("#{n}"),
        None => format!("id {}", build.id),
    }
}

/// One recent build: its number as the chip, coloured by outcome, its status
/// text as the title, and `branch · duration · age` as the quiet line.
fn build_row(viewer: &Viewer<'_>, config: &str, build: &Build, now: DateTime<Utc>) -> ViewRow {
    let o = Outcome::of(build);
    let started = build.start_date.as_deref().and_then(parse_ts);
    let finished = build.finish_date.as_deref().and_then(parse_ts);
    let queued = build.queued_date.as_deref().and_then(parse_ts);
    let duration = match (started, finished) {
        (Some(s), Some(f)) => Some(f - s),
        (Some(s), None) if o == Outcome::Running => Some(now - s),
        _ => None,
    };
    let meta: Vec<String> = [
        nonblank(build.branch_name.as_deref()).map(str::to_owned),
        duration.map(span),
        finished.or(started).or(queued).map(|t| age(now - t)),
    ]
    .into_iter()
    .flatten()
    .collect();
    ViewRow {
        entity_id: Some(format!("{}:{}", viewer.source_id, map::build_key(build.id))),
        title: nonblank(build.status_text.as_deref())
            .unwrap_or(o.word())
            .to_owned(),
        meta: (!meta.is_empty()).then(|| meta.join(" · ")),
        tone: Some(o.tone()),
        chip: Some(number(build)),
        url: map::build_web_url(viewer.base_url, Some(config), build.id),
        commit: None,
    }
}

/// A build's length: `38s`, `4m 18s`, `1h 02m`.
fn span(d: chrono::TimeDelta) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
    }
}

/// How long ago: `just now`, `12m ago`, `3h ago`, `5d ago`.
fn age(d: chrono::TimeDelta) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86_400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

// --- Parameters ---------------------------------------------------------

/// Own and inherited, in TeamCity's order (by name), every one read-only
/// until #668. A `password` spec makes the value secret and empty.
fn parameters(entity: &str, record: &ConfigRecord) -> ViewSection {
    let values: Vec<Setting> = record
        .parameters
        .property
        .iter()
        .map(|p| {
            let secret = p
                .spec
                .as_ref()
                .and_then(|t| t.raw_value.as_deref())
                .is_some_and(|raw| raw.trim_start().starts_with("password"));
            Setting {
                name: p.name.clone(),
                value: if secret {
                    String::new()
                } else {
                    p.value.clone().unwrap_or_default()
                },
                inherited: p.inherited,
                secret,
                editable: false,
                description: None,
            }
        })
        .collect();
    let count = values.len();
    tab(
        Tab::Parameters,
        "Parameters",
        Some(count),
        vec![Block::Settings {
            title: None,
            entity: entity.to_owned(),
            write_op: WRITE_OP_SET_PARAMETER.to_owned(),
            values,
        }],
    )
}

// --- Steps --------------------------------------------------------------

fn steps(record: &ConfigRecord) -> ViewSection {
    let list = &record.steps.step;
    let blocks = list.iter().flat_map(step_blocks).collect();
    tab(Tab::Steps, "Steps", Some(list.len()), blocks)
}

/// A step's name and runner, then its script where the runner has one --
/// drawn as a log block, monospace and whole, because a script is lines.
fn step_blocks(step: &Step) -> Vec<Block> {
    let mut facts = vec![fact(
        "Step",
        nonblank(step.name.as_deref())
            .or(nonblank(step.id.as_deref()))
            .unwrap_or("unnamed"),
        None,
    )];
    if let Some(runner) = nonblank(step.runner.as_deref()) {
        facts.push(fact("Runner", runner_name(runner), None));
    }
    if step.disabled {
        facts.push(disabled());
    }
    let mut blocks = vec![Block::Facts { facts }];
    if let Some(script) = script_of(&step.properties) {
        blocks.push(Block::Log {
            text: script,
            truncated: false,
        });
    }
    blocks
}

/// The command a step runs: a command-line runner's custom script, or an
/// executable with its parameters.
fn script_of(properties: &Properties) -> Option<String> {
    if let Some(script) = properties.get("script.content") {
        return Some(script.trim_end().to_owned());
    }
    let executable = properties.get("command.executable")?;
    Some(match properties.get("command.parameters") {
        Some(args) => format!("{executable} {args}"),
        None => executable.to_owned(),
    })
}

/// The runner names TeamCity's own UI shows, for the bundled runners; any
/// other runner type verbatim.
fn runner_name(runner: &str) -> &str {
    match runner {
        "simpleRunner" => "Command Line",
        "Maven2" => "Maven",
        "gradle-runner" => "Gradle",
        "Ant" => "Ant",
        "dotnet" => ".NET",
        "python-runner" => "Python",
        "nodejs-runner" => "Node.js",
        "DockerCommand" => "Docker",
        "kotlinScript" => "Kotlin Script",
        other => other,
    }
}

// --- Triggers -----------------------------------------------------------

fn triggers(record: &ConfigRecord) -> ViewSection {
    let list = &record.triggers.trigger;
    let blocks = list
        .iter()
        .map(|t| Block::Facts {
            facts: trigger_facts(t),
        })
        .collect();
    tab(Tab::Triggers, "Triggers", Some(list.len()), blocks)
}

/// A trigger's type and the properties that say when it fires and for what.
/// Only properties picked by name are drawn, never the rest, as for a VCS
/// root: a type with no arm here shows its type and whether it is disabled.
fn trigger_facts(trigger: &Trigger) -> Vec<Fact> {
    let kind = nonblank(trigger.kind.as_deref())
        .or(nonblank(trigger.id.as_deref()))
        .unwrap_or("unknown");
    let p = &trigger.properties;
    let mut facts = vec![fact("Trigger", trigger_name(kind), None)];
    if trigger.disabled {
        facts.push(disabled());
    }
    let mut push = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            facts.push(fact(label, value, None));
        }
    };
    let own = |name: &str| p.get(name).map(str::to_owned);
    match kind {
        "vcsTrigger" => {
            push("Branch filter", own("branchFilter"));
            push("Rules", own("triggerRules"));
        }
        "schedulingTrigger" => {
            push("When", schedule(p));
            push("Branch filter", own("branchFilter"));
            push("Rules", own("triggerRules"));
        }
        "buildDependencyTrigger" => {
            push("After", own("dependsOn"));
            push(
                "Only after success",
                own("afterSuccessfulBuildOnly")
                    .filter(|v| v == "true")
                    .map(|_| "yes".to_owned()),
            );
            push("Branch filter", own("branchFilter"));
        }
        // A type with no arm here -- a plugin's, or a retry trigger -- may
        // keep a secret under any name, so none of its properties is drawn.
        _ => {}
    }
    facts
}

fn trigger_name(kind: &str) -> &str {
    match kind {
        "vcsTrigger" => "VCS",
        "schedulingTrigger" => "Schedule",
        "buildDependencyTrigger" => "Finish build",
        "retryBuildTrigger" => "Retry build",
        other => other,
    }
}

/// A schedule as words: `daily at 03:00`, `weekly on Sunday at 22:30`, or
/// the cron fields where the policy is one.
fn schedule(p: &Properties) -> Option<String> {
    let at = || {
        let hour: u32 = p.get("hour")?.parse().ok()?;
        let minute: u32 = p.get("minute").unwrap_or("0").parse().ok()?;
        Some(format!("{hour:02}:{minute:02}"))
    };
    match p.get("schedulingPolicy")? {
        "daily" => Some(match at() {
            Some(t) => format!("daily at {t}"),
            None => "daily".to_owned(),
        }),
        "weekly" => {
            let day = p.get("dayOfWeek").unwrap_or("?");
            Some(match at() {
                Some(t) => format!("weekly on {day} at {t}"),
                None => format!("weekly on {day}"),
            })
        }
        "cron" => {
            let fields: Vec<&str> = ["sec", "min", "hour", "dm", "month", "dw", "year"]
                .iter()
                .map(|f| p.get(&format!("cronExpression_{f}")).unwrap_or("*"))
                .collect();
            Some(format!("cron {}", fields.join(" ")))
        }
        other => Some(other.to_owned()),
    }
}

// --- VCS roots ----------------------------------------------------------

fn vcs_roots(record: &ConfigRecord) -> ViewSection {
    let list = &record.vcs_root_entries.entry;
    let blocks = list
        .iter()
        .map(|e| Block::Facts {
            facts: root_facts(e),
        })
        .collect();
    tab(Tab::VcsRoots, "VCS roots", Some(list.len()), blocks)
}

/// A root's name, its VCS, where it fetches from, its default branch and the
/// checkout rules this configuration applies to it. Never its credentials.
fn root_facts(entry: &VcsRootEntry) -> Vec<Fact> {
    let root = entry.root.as_ref();
    let name = root
        .and_then(|r| nonblank(r.name.as_deref()).or(nonblank(r.id.as_deref())))
        .or(nonblank(entry.id.as_deref()))
        .unwrap_or("unnamed");
    let mut facts = vec![fact("Root", name, None)];
    if let Some(vcs) = root.and_then(|r| nonblank(r.vcs_name.as_deref())) {
        facts.push(fact(
            "Type",
            match vcs {
                "jetbrains.git" => "Git",
                "mercurial" => "Mercurial",
                "svn" => "Subversion",
                "perforce" => "Perforce",
                other => other,
            },
            None,
        ));
    }
    if let Some(root) = root {
        if let Some(url) = root.properties.get("url").and_then(without_userinfo) {
            facts.push(fact("URL", url, None));
        }
        if let Some(branch) = root.properties.get("branch") {
            facts.push(fact("Default branch", branch, None));
        }
    }
    if let Some(rules) = nonblank(entry.checkout_rules.as_deref()) {
        facts.push(fact("Checkout rules", rules, None));
    }
    facts
}

/// A step or trigger that is switched off: said, quietly.
fn disabled() -> Fact {
    fact("Disabled", "yes", Some(Tone::Muted))
}

/// A URL with any `user:password@` taken out: a git URL may carry the
/// credential it fetches with, and a view says where, never with what.
///
/// `None` -- draw no URL -- where the credential's end cannot be told. An
/// unescaped `/` in a password or token ends the authority early and leaves
/// the rest of the credential, `@` and all, in what reads as the path
/// (`https://u:pa/ss@host/p.git`). TeamCity stores a root's URL verbatim, so
/// that form can arrive; an `@` after the authority is therefore never drawn,
/// though a path could in principle hold one.
fn without_userinfo(url: &str) -> Option<String> {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, url),
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    if path.contains('@') {
        return None;
    }
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    Some(match scheme {
        Some(scheme) => format!("{scheme}://{host}{path}"),
        None => format!("{host}{path}"),
    })
}

fn nonblank(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn spans_and_ages_read_at_a_glance() {
        assert_eq!(span(TimeDelta::seconds(38)), "38s");
        assert_eq!(span(TimeDelta::seconds(258)), "4m 18s");
        assert_eq!(span(TimeDelta::seconds(3725)), "1h 02m");
        assert_eq!(
            span(TimeDelta::seconds(-5)),
            "0s",
            "a clock skew is not a negative build"
        );
        assert_eq!(age(TimeDelta::seconds(20)), "just now");
        assert_eq!(age(TimeDelta::seconds(720)), "12m ago");
        assert_eq!(age(TimeDelta::seconds(3 * 3600 + 5)), "3h ago");
        assert_eq!(age(TimeDelta::days(5)), "5d ago");
    }

    /// A git URL may carry a credential (`https://user:token@host/…`); the
    /// view draws where a root fetches from, never with what.
    #[test]
    fn a_roots_url_is_drawn_without_its_credential() {
        let drawn = |url: &str| without_userinfo(url);
        assert_eq!(
            drawn("https://knobas:s3cret@gitea.example/tidewater/p.git").as_deref(),
            Some("https://gitea.example/tidewater/p.git")
        );
        assert_eq!(
            drawn("ssh://git@gitea.example:2222/tidewater/p.git").as_deref(),
            Some("ssh://gitea.example:2222/tidewater/p.git")
        );
        assert_eq!(
            drawn("http://gitea:3000/tidewater/p.git").as_deref(),
            Some("http://gitea:3000/tidewater/p.git")
        );
        // scp-like and odd forms keep the host and drop what is before `@`.
        assert_eq!(
            drawn("git@gitea.example:t/p.git").as_deref(),
            Some("gitea.example:t/p.git")
        );
        // An unescaped `/` in the credential ends the authority early; the
        // rest of it, and its `@`, land in what reads as the path. Where an
        // `@` follows the authority no URL is drawn -- not even one whose `@`
        // really is in the path, since the two cannot be told apart.
        for ambiguous in [
            "https://knobas:pa/ss@gitea.example/tidewater/p.git",
            "https://tok/en@gitea.example/tidewater/p.git",
            "https://gitea.example/a@b/p.git",
        ] {
            assert_eq!(drawn(ambiguous), None, "{ambiguous}");
        }
    }

    #[test]
    fn a_schedule_is_drawn_in_words() {
        let props = |pairs: &[(&str, &str)]| Properties {
            property: pairs
                .iter()
                .map(|(n, v)| crate::rest::Property {
                    name: (*n).to_owned(),
                    value: Some((*v).to_owned()),
                    ..Default::default()
                })
                .collect(),
        };
        assert_eq!(
            schedule(&props(&[
                ("schedulingPolicy", "weekly"),
                ("dayOfWeek", "Sunday"),
                ("hour", "22"),
                ("minute", "30")
            ]))
            .as_deref(),
            Some("weekly on Sunday at 22:30")
        );
        assert_eq!(
            schedule(&props(&[
                ("schedulingPolicy", "cron"),
                ("cronExpression_min", "15"),
                ("cronExpression_hour", "*/2")
            ]))
            .as_deref(),
            Some("cron * 15 */2 * * * *")
        );
        assert_eq!(schedule(&props(&[])), None);
    }

    #[test]
    fn a_queued_build_has_no_number_yet_and_says_so() {
        let queued = Build {
            id: 99,
            number: None,
            build_type_id: None,
            state: Some("queued".to_owned()),
            status: None,
            status_text: None,
            branch_name: None,
            queued_date: None,
            start_date: None,
            finish_date: None,
            build_type: None,
            running_info: None,
            triggered: None,
        };
        assert_eq!(number(&queued), "id 99");
        let o = Outcome::of(&queued);
        assert_eq!(
            (o.word(), o.tone(), o.finished()),
            ("queued", Tone::Muted, false)
        );
    }
}
