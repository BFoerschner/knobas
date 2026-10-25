//! The source views this adapter draws (ADR-0020, spec #660).
//!
//! Two kinds. A **build configuration** (#664) opens on its overview --
//! where it lives, whether it is paused, how its recent builds went, and the
//! builds themselves -- with *Parameters*, *Steps*, *Triggers* and *VCS roots*
//! beside it. A **build** (#665) opens on what happened -- outcome, status
//! text, branch, who started it, when and for how long, on which agent, and
//! its configuration as an address -- with *Changes*, *Tests*, *Log*,
//! *Artifacts* and *Parameters* beside it; *Tests* and *Log* are deferred
//! and read when their tab opens.
//!
//! A configuration's whole view is two requests: the configuration, with one
//! explicit `fields=` that answers every tab but the builds, and its recent
//! builds. A build's is three: the build (overview, parameters and the failed
//! count), its changes and its artifacts. A tab asked for on its own reads
//! only what it needs. Every read is live and nothing is stored (rule 1); a
//! refusal is the error it maps to, so the detail says *Showing the mirror*
//! and why (rule 2).
//!
//! **No secret enters a view.** A parameter whose spec says `password` is
//! drawn with an empty value whatever the server sent; a value TeamCity
//! masked (`*******`) is drawn as a secret too; a URL is drawn without the
//! `user:password@` it may carry, or not at all where that cannot be told.
//! A build's change names its commit's repository the same way.

use chrono::{DateTime, Utc};
use knobas_source::SourceError;
use knobas_source::view::{
    ActionField, Block, CommitHint, Fact, FieldKind, Setting, SourceView, Tone, ViewAction,
    ViewRow, ViewSection,
};

use crate::client::{FAILED_TESTS, HttpRest};
use crate::ids::{config_id, path_escaped};
use crate::map;
use crate::rest::{
    Build, BuildRecord, Change, ConfigRecord, FileEntry, Properties, Revision, RootInstance, Step,
    TestOccurrence, Trigger, VcsRootEntry, parse_ts,
};

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
    if let Some(id) = key.strip_prefix("build:") {
        let id = build_id(entity, id)?;
        return build(viewer, entity, id, section).await.map(Some);
    }
    Err(SourceError::protocol(format!(
        "teamcity: {entity:?} is neither a build nor a build configuration"
    )))
}

/// A build id as TeamCity issues them: a positive integer.
fn build_id(entity: &str, id: &str) -> Result<i64, SourceError> {
    id.parse::<i64>()
        .ok()
        .filter(|n| *n > 0 && id.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| {
            SourceError::protocol(format!(
                "teamcity: {entity:?} does not name a build TeamCity could have issued"
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
    // What the user may write (#2040), read only where the answer draws a
    // write: the whole view's toolbar, and the parameters. A read that fails
    // is no verdict, and the view draws as before.
    let grants = if matches!(wanted, None | Some(Tab::Parameters)) {
        Some(crate::grants::Grants::read(viewer.rest).await)
    } else {
        None
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
    // The toolbar comes with the whole view; a section read answers its section.
    let actions = if wanted.is_none() {
        vec![run_action(&builds)]
    } else {
        Vec::new()
    };
    let mut view = SourceView { sections, actions };
    if let (Some(Ok(grants)), Some(project)) = (grants, nonblank(record.project_id.as_deref())) {
        view.refuse_writes(|op| grants.refusal(op, None, project));
        let overrides = crate::grants::RUN_PARAMETERS;
        if let Some(reason) = grants.refusal(crate::WRITE_OP_RUN_BUILD, Some(overrides), project) {
            for run in view.actions.iter_mut().filter(|a| a.with_settings) {
                run_without_settings(run, &reason);
            }
        }
    }
    Ok(view)
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

/// *Run…* (#669), the configuration's main write, in the toolbar (#702): a
/// [`run_build`](crate::WRITE_OP_RUN_BUILD) asking for a branch, typed, with
/// the branches the recent builds ran on -- newest first and each once -- as
/// suggestions. The branch starts empty, which is the default branch; the
/// dialog also offers the *Parameters* tab's values to override for this run
/// (`with_settings`), unless the user may not send them
/// ([`run_without_settings`], #2055).
///
/// **Never the default branch by name.** TeamCity resolves an explicit
/// `branchName: main` against the branch specification to a logical branch
/// that is *not* `<default>` (testenv/README.md), so suggesting it would
/// offer a run of the default branch that is not one. Always enabled: a paused
/// configuration still runs on request, and a refusal is TeamCity's to give.
fn run_action(builds: &[Build]) -> ViewAction {
    let mut suggestions: Vec<String> = Vec::new();
    for build in builds {
        let Some(branch) = nonblank(build.branch_name.as_deref()) else {
            continue;
        };
        if build.default_branch != Some(true) && !suggestions.iter().any(|o| o == branch) {
            suggestions.push(branch.to_owned());
        }
    }
    ViewAction {
        fields: vec![ActionField {
            placeholder: Some("default branch".to_owned()),
            ..ActionField::new("branch", "Branch", FieldKind::Text { suggestions })
        }],
        confirm: Some(
            "Queue one build of this configuration. Leave the branch empty for the default \
             branch; only the values you change are sent, for this build alone."
                .to_owned(),
        ),
        primary: true,
        with_settings: true,
        ..ViewAction::new(crate::WRITE_OP_RUN_BUILD, "Run…")
    }
}

/// *Run…* for a user who may not override parameters (#2055): the branch
/// alone, and the sentence says why, since TeamCity refuses a run carrying
/// overrides from such a user.
fn run_without_settings(run: &mut ViewAction, reason: &str) {
    run.with_settings = false;
    run.confirm = Some(format!(
        "Queue one build of this configuration. Leave the branch empty for the default branch. \
         The parameters are sent as they are: {reason}."
    ));
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
        Outcome::of_state(build.state.as_deref(), build.status.as_deref())
    }

    fn of_state(state: Option<&str>, status: Option<&str>) -> Outcome {
        match (state, status) {
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

/// Own and inherited, in TeamCity's order (by name), editable through
/// `set_parameter` (#668), which this adapter declares: an own value is
/// changed, an inherited one gets an own override, and a password is written
/// without its old value ever being drawn. A value drawn other than it is
/// held is not editable. Drawn by [`drawn_value`],
/// the rule the build view uses too.
fn parameters(entity: &str, record: &ConfigRecord) -> ViewSection {
    let values = parameter_settings(record);
    let count = values.len();
    tab(
        Tab::Parameters,
        "Parameters",
        Some(count),
        vec![Block::Settings {
            title: None,
            entity: entity.to_owned(),
            write_op: crate::WRITE_OP_SET_PARAMETER.to_owned(),
            values,
        }],
    )
}

/// A configuration's parameters as [`parameters`] draws them, for the view's
/// tab and the Builds frame's pipeline settings (#851) alike.
fn parameter_settings(record: &ConfigRecord) -> Vec<Setting> {
    record
        .parameters
        .property
        .iter()
        .map(|p| {
            let (value, secret) = drawn_value(&p.name, p.value.as_deref(), is_password(p));
            // A value drawn other than it is held -- a URL whose credential
            // was cut out -- is not offered: an edit of what was drawn would
            // write the credential away.
            let editable = secret || p.value.as_deref().unwrap_or_default() == value;
            Setting {
                name: p.name.clone(),
                value,
                inherited: p.inherited,
                secret,
                editable,
                description: None,
            }
        })
        .collect()
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

// --- The Builds frame's pipeline settings (#851) ---------------------------

/// A configuration's settings as the Builds frame draws them (ADR-0022): its
/// VCS roots as repositories, its triggers, and its parameters. Read off the
/// record the view reads, through the same scrubbing: a root's URL without
/// its credential, a trigger's properties picked by name, a password
/// parameter without its value.
pub(crate) fn pipeline_settings(
    record: &ConfigRecord,
) -> knobas_source::pipeline::PipelineSettings {
    use knobas_source::pipeline::{PipelineRepository, PipelineSettings, PipelineTrigger};
    let repositories = record
        .vcs_root_entries
        .entry
        .iter()
        .map(|entry| {
            let root = entry.root.as_ref();
            PipelineRepository {
                name: root
                    .and_then(|r| nonblank(r.name.as_deref()).or(nonblank(r.id.as_deref())))
                    .or(nonblank(entry.id.as_deref()))
                    .unwrap_or("unnamed")
                    .to_owned(),
                url: root
                    .and_then(|r| r.properties.get("url"))
                    .and_then(without_userinfo),
                default_branch: root
                    .and_then(|r| r.properties.get("branch"))
                    .map(str::to_owned),
                entity_id: None,
            }
        })
        .collect();
    let triggers = record
        .triggers
        .trigger
        .iter()
        .map(|trigger| {
            // The view's facts lead with the trigger's name and say when it
            // is off; the frame carries both as fields of their own.
            let mut facts = trigger_facts(trigger);
            let name = facts.remove(0).value;
            facts.retain(|f| f.label != "Disabled");
            PipelineTrigger {
                name,
                facts,
                disabled: trigger.disabled,
            }
        })
        .collect();
    PipelineSettings {
        repositories,
        triggers,
        parameters: parameter_settings(record),
        parameter_write_op: Some(crate::WRITE_OP_SET_PARAMETER.to_owned()),
        parameter_specs: record
            .parameters
            .property
            .iter()
            .map(|p| {
                crate::spec::parse(
                    &p.name,
                    p.spec.as_ref().and_then(|t| t.raw_value.as_deref()),
                )
            })
            .collect(),
        // The chain is a listing of its own; `TeamCitySource::pipeline_settings` fills it.
        chain: Vec::new(),
        steps: step_names(record),
    }
}

/// The enabled steps' names, in order (#1016): a run's log numbers only the
/// enabled ones. One unnamed step names none, because its log heading is the
/// runner's name, which the record does not carry.
fn step_names(record: &ConfigRecord) -> Vec<String> {
    record
        .steps
        .step
        .iter()
        .filter(|step| !step.disabled)
        .map(|step| nonblank(step.name.as_deref()).map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
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

// --- A build (#665) ------------------------------------------------------

/// A build's tabs, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildTab {
    Overview,
    Changes,
    Tests,
    Log,
    Artifacts,
    Parameters,
}

impl BuildTab {
    const ALL: [BuildTab; 6] = [
        BuildTab::Overview,
        BuildTab::Changes,
        BuildTab::Tests,
        BuildTab::Log,
        BuildTab::Artifacts,
        BuildTab::Parameters,
    ];

    /// The section id a deferred read names; stable.
    fn id(self) -> &'static str {
        match self {
            BuildTab::Overview => "overview",
            BuildTab::Changes => "changes",
            BuildTab::Tests => "tests",
            BuildTab::Log => "log",
            BuildTab::Artifacts => "artifacts",
            BuildTab::Parameters => "parameters",
        }
    }

    fn title(self) -> &'static str {
        match self {
            BuildTab::Overview => "Overview",
            BuildTab::Changes => "Changes",
            BuildTab::Tests => "Tests",
            BuildTab::Log => "Log",
            BuildTab::Artifacts => "Artifacts",
            BuildTab::Parameters => "Parameters",
        }
    }

    /// Read on first open rather than with the view: the failed tests and
    /// the log are the expensive reads.
    fn deferred(self) -> bool {
        matches!(self, BuildTab::Tests | BuildTab::Log)
    }
}

/// How many lines of a build's log *Log* shows: its end.
pub(crate) const LOG_TAIL_LINES: usize = 400;

/// The view of build `id`: every tab, or the one `section` names.
///
/// The build record is read for the whole view and for the tabs it answers
/// (*Overview*, *Parameters*, *Artifacts*' download paths); *Changes*,
/// *Tests* and *Log* read only their own resource.
async fn build(
    viewer: &Viewer<'_>,
    entity: &str,
    id: i64,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let wanted = match section {
        None => None,
        Some(asked) => Some(
            BuildTab::ALL
                .into_iter()
                .find(|t| t.id() == asked)
                .ok_or_else(|| {
                    SourceError::protocol(format!(
                        "teamcity: a build's view has no section {asked:?} (asked for {entity:?})"
                    ))
                })?,
        ),
    };
    let rest = viewer.rest;
    let wants = |tab: BuildTab| wanted.is_none_or(|w| w == tab);
    // The record answers the overview, the parameters, the failed count
    // and an artifact's download path; the other tabs read only their own.
    let record = if wanted.is_none()
        || matches!(
            wanted,
            Some(BuildTab::Overview | BuildTab::Parameters | BuildTab::Artifacts)
        ) {
        Some(rest.build_record(id).await?)
    } else {
        None
    };
    let changes = if wants(BuildTab::Changes) {
        rest.build_changes(id).await?.change
    } else {
        Vec::new()
    };
    let artifacts = if wants(BuildTab::Artifacts) {
        rest.artifacts(id).await?.file
    } else {
        Vec::new()
    };
    let now = Utc::now();

    let mut sections = Vec::new();
    for tab in BuildTab::ALL.into_iter().filter(|t| wants(*t)) {
        // A deferred tab in the whole view is its stub: title and count.
        let stub = wanted.is_none() && tab.deferred();
        let (count, blocks) = match tab {
            BuildTab::Overview => (
                None,
                record
                    .as_ref()
                    .map(|r| build_overview(viewer, r, now))
                    .unwrap_or_default(),
            ),
            BuildTab::Changes => (
                Some(changes.len()),
                vec![Block::Entities {
                    title: None,
                    rows: changes.iter().map(|c| change_row(viewer, c)).collect(),
                }],
            ),
            BuildTab::Tests if stub => (
                Some(
                    record
                        .as_ref()
                        .and_then(|r| r.test_occurrences.as_ref())
                        .and_then(|t| t.failed)
                        .unwrap_or(0) as usize,
                ),
                Vec::new(),
            ),
            BuildTab::Tests => {
                let failed = rest.failed_tests(id).await?.test_occurrence;
                // A full page may not be all of them: the build's own count
                // is the total, as the stub shows it (#683).
                let (total, config) = if failed.len() >= FAILED_TESTS as usize {
                    let read;
                    let record = match &record {
                        Some(r) => r,
                        None => {
                            read = rest.build_record(id).await?;
                            &read
                        }
                    };
                    let total = record
                        .test_occurrences
                        .as_ref()
                        .and_then(|t| t.failed)
                        // Never fewer than were listed, whatever TeamCity counts.
                        .map_or(failed.len(), |n| (n as usize).max(failed.len()));
                    (total, build_type_id(record).map(str::to_owned))
                } else {
                    (failed.len(), None)
                };
                let mut blocks = failed_test_blocks(&failed);
                if total > failed.len() {
                    blocks.push(left_out_tests(
                        viewer,
                        config.as_deref(),
                        id,
                        failed.len(),
                        total,
                    ));
                }
                (Some(total), blocks)
            }
            BuildTab::Log if stub => (None, Vec::new()),
            BuildTab::Log => {
                let (text, truncated) = rest.log_tail(id, LOG_TAIL_LINES).await?;
                (None, vec![Block::Log { text, truncated }])
            }
            BuildTab::Artifacts => {
                let config = record.as_ref().and_then(build_type_id);
                (
                    Some(artifacts.len()),
                    vec![Block::Entities {
                        title: None,
                        rows: artifacts
                            .iter()
                            .map(|f| artifact_row(viewer, config, id, f))
                            .collect(),
                    }],
                )
            }
            BuildTab::Parameters => {
                let values = record.as_ref().map(build_parameters).unwrap_or_default();
                (
                    Some(values.len()),
                    vec![Block::Settings {
                        title: None,
                        entity: entity.to_owned(),
                        // Nothing changes a build's parameters: it ran with them.
                        write_op: String::new(),
                        values,
                    }],
                )
            }
        };
        sections.push(ViewSection {
            id: tab.id().to_owned(),
            title: tab.title().to_owned(),
            count: count.map(|n| n as u64),
            deferred: stub,
            blocks,
        });
    }
    Ok(SourceView::new(sections))
}

fn build_type_id(record: &BuildRecord) -> Option<&str> {
    nonblank(record.build_type_id.as_deref())
        .or_else(|| record.build_type.as_ref().map(|t| t.id.as_str()))
}

/// What happened, where, and for how long; then the configuration, as an
/// address.
fn build_overview(viewer: &Viewer<'_>, record: &BuildRecord, now: DateTime<Utc>) -> Vec<Block> {
    let o = Outcome::of_state(record.state.as_deref(), record.status.as_deref());
    let mut facts = vec![fact("Outcome", o.word(), Some(o.tone()))];
    let mut push = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            facts.push(fact(label, value, None));
        }
    };
    push(
        "Status",
        nonblank(record.status_text.as_deref()).map(str::to_owned),
    );
    if o == Outcome::Running {
        push(
            "Now",
            record
                .running_info
                .as_ref()
                .and_then(|r| nonblank(r.current_stage_text.as_deref()))
                .map(str::to_owned),
        );
    }
    push(
        "Number",
        nonblank(record.number.as_deref()).map(|n| format!("#{n}")),
    );
    push(
        "Branch",
        nonblank(record.branch_name.as_deref()).map(str::to_owned),
    );
    push(
        "Triggered by",
        record.triggered.as_ref().and_then(|t| {
            t.user
                .as_ref()
                .and_then(|u| nonblank(u.username.as_deref()))
                .or_else(|| nonblank(t.kind.as_deref()).map(trigger_kind))
                .map(str::to_owned)
        }),
    );
    push("Queued", record.queued_date.as_deref().map(stamp));
    push("Started", record.start_date.as_deref().map(stamp));
    push("Finished", record.finish_date.as_deref().map(stamp));
    let started = record.start_date.as_deref().and_then(parse_ts);
    let finished = record.finish_date.as_deref().and_then(parse_ts);
    push(
        "Duration",
        match (started, finished) {
            (Some(s), Some(f)) => Some(span(f - s)),
            (Some(s), None) if o == Outcome::Running => Some(span(now - s)),
            _ => None,
        },
    );
    push(
        "Agent",
        record
            .agent
            .as_ref()
            .and_then(|a| nonblank(a.name.as_deref()))
            .map(str::to_owned),
    );

    let mut blocks = vec![Block::Facts { facts }];
    if let Some(config) = build_type_id(record) {
        let nested = record.build_type.as_ref();
        let name = nested
            .and_then(|t| nonblank(t.name.as_deref()))
            .unwrap_or(config);
        let title = match nested.and_then(|t| nonblank(t.project_name.as_deref())) {
            Some(project) => format!("{project} / {name}"),
            None => name.to_owned(),
        };
        blocks.push(Block::Entities {
            title: Some("Configuration".to_owned()),
            rows: vec![ViewRow {
                entity_id: Some(format!(
                    "{}:{}",
                    viewer.source_id,
                    map::build_config_key(config)
                )),
                title,
                meta: None,
                tone: None,
                chip: None,
                url: Some(map::build_config_web_url(viewer.base_url, config)),
                commit: None,
            }],
        });
    }
    let revisions: Vec<ViewRow> = record.revisions.revision.iter().map(revision_row).collect();
    if !revisions.is_empty() {
        blocks.push(Block::Entities {
            title: Some(
                if revisions.len() == 1 {
                    "Revision"
                } else {
                    "Revisions"
                }
                .to_owned(),
            ),
            rows: revisions,
        });
    }
    blocks
}

/// The commit a build ran on in one VCS root (#683): the root's name, the
/// short sha as the chip, the branch as the quiet line -- and the same commit
/// hint a change carries, so a build that lists no change (a configuration's
/// first build, a re-run) still opens the commit it built.
fn revision_row(revision: &Revision) -> ViewRow {
    let sha = nonblank(revision.version.as_deref());
    let short = sha.map(short_sha);
    let root = revision.vcs_root_instance.as_ref();
    ViewRow {
        entity_id: None,
        title: root
            .and_then(|r| nonblank(r.name.as_deref()))
            .map(str::to_owned)
            .or_else(|| short.clone())
            .unwrap_or_else(|| "Revision".to_owned()),
        meta: nonblank(revision.vcs_branch_name.as_deref())
            .map(|b| b.strip_prefix("refs/heads/").unwrap_or(b).to_owned()),
        tone: None,
        chip: short,
        url: None,
        commit: commit_hint(root, sha),
    }
}

/// Who or what started a build, where no person did.
fn trigger_kind(kind: &str) -> &str {
    match kind {
        "vcs" => "VCS trigger",
        "schedule" => "schedule",
        "buildType" => "finish-build trigger",
        "restApi" => "REST API",
        other => other,
    }
}

/// A TeamCity stamp as `2026-10-12 19:51 +00:00`, in the offset the server
/// wrote; the raw text where it does not parse.
fn stamp(raw: &str) -> String {
    DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z")
        .map(|t| t.format("%Y-%m-%d %H:%M %:z").to_string())
        .unwrap_or_else(|_| raw.to_owned())
}

/// One commit a build built: its subject line, its short sha as the chip,
/// `author · time` as the quiet line, TeamCity's change page -- and the
/// repository and sha, which the app resolves against the mirror (#665).
fn change_row(viewer: &Viewer<'_>, change: &Change) -> ViewRow {
    let sha = nonblank(change.version.as_deref());
    let short = sha.map(short_sha);
    let subject = change
        .comment
        .as_deref()
        .and_then(|c| c.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(str::to_owned);
    let meta: Vec<String> = [
        nonblank(change.username.as_deref()).map(str::to_owned),
        change.date.as_deref().map(stamp),
    ]
    .into_iter()
    .flatten()
    .collect();
    ViewRow {
        entity_id: None,
        title: subject
            .or_else(|| short.clone())
            .unwrap_or_else(|| format!("change {}", change.id)),
        meta: (!meta.is_empty()).then(|| meta.join(" · ")),
        tone: None,
        chip: short,
        url: Some(format!(
            "{}/change/{}",
            map::root(viewer.base_url),
            change.id
        )),
        commit: commit_hint(change.vcs_root_instance.as_ref(), sha),
    }
}

/// A commit named by its repository and sha, for the app to resolve against
/// the mirror (#665): the root's `url` without its credential. `None` where
/// either is missing.
fn commit_hint(root: Option<&RootInstance>, sha: Option<&str>) -> Option<CommitHint> {
    let repo_url = root
        .and_then(|r| r.properties.get("url"))
        .and_then(without_userinfo)?;
    Some(CommitHint {
        repo_url,
        sha: sha?.to_owned(),
    })
}

/// A sha as a chip shows it: its first twelve characters.
fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// Per failed test: its name as a row, chipped `new`, `failed` or `muted`,
/// then its details as text where it has any. None at all is an empty list,
/// which the shell draws as *None.*
fn failed_test_blocks(failed: &[TestOccurrence]) -> Vec<Block> {
    if failed.is_empty() {
        return vec![Block::Entities {
            title: None,
            rows: Vec::new(),
        }];
    }
    let mut blocks = Vec::new();
    for test in failed {
        let (chip, tone) = if test.muted {
            ("muted", Tone::Muted)
        } else if test.new_failure {
            ("new", Tone::Err)
        } else {
            ("failed", Tone::Err)
        };
        blocks.push(Block::Entities {
            title: None,
            rows: vec![ViewRow {
                entity_id: None,
                title: nonblank(test.name.as_deref())
                    .unwrap_or("unnamed test")
                    .to_owned(),
                meta: None,
                tone: Some(tone),
                chip: Some(chip.to_owned()),
                url: None,
                commit: None,
            }],
        });
        if let Some(details) = nonblank(test.details.as_deref()) {
            blocks.push(Block::Text {
                text: details.trim_end().to_owned(),
                markdown: false,
            });
        }
    }
    blocks
}

/// The muted last row of a *Tests* tab that lists fewer failures than the
/// build has (#683): how many of how many, and TeamCity's tests page for the
/// rest -- the shape Gitea's cut lists have.
fn left_out_tests(
    viewer: &Viewer<'_>,
    config: Option<&str>,
    id: i64,
    listed: usize,
    total: usize,
) -> Block {
    Block::Entities {
        title: None,
        rows: vec![ViewRow {
            entity_id: None,
            title: format!("The first {listed} of {total} failed tests; the rest are on TeamCity."),
            meta: None,
            tone: Some(Tone::Muted),
            chip: None,
            url: map::build_web_url(viewer.base_url, config, id)
                .map(|u| format!("{u}?buildTab=tests")),
            commit: None,
        }],
    }
}

/// One top-level artifact: a file with its size and its download, or a
/// directory with how much it holds.
fn artifact_row(viewer: &Viewer<'_>, config: Option<&str>, id: i64, file: &FileEntry) -> ViewRow {
    let (title, meta, url) = match &file.children {
        Some(children) => (
            format!("{}/", file.name),
            children.count.map(|n| format!("{n} files")),
            None,
        ),
        None => (
            file.name.clone(),
            file.size.map(size_of),
            config.map(|config| {
                format!(
                    "{}/repository/download/{config}/{id}:id/{}",
                    map::root(viewer.base_url),
                    path_escaped(&file.name)
                )
            }),
        ),
    };
    ViewRow {
        entity_id: None,
        title,
        meta,
        tone: None,
        chip: None,
        url,
        commit: None,
    }
}

/// A size as `150 B`, `1.5 KB`, `5.1 MB`, `2.0 GB`.
fn size_of(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    #[allow(clippy::cast_precision_loss)] // a size to one decimal
    let mut size = n as f64 / 1024.0;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// The parameters a build ran with, in TeamCity's order, none of them
/// editable. Secret where the configuration's spec says `password`, where
/// TeamCity masked the value, or where a URL's credential cannot be cut out;
/// a URL is drawn without its credential.
fn build_parameters(record: &BuildRecord) -> Vec<Setting> {
    let passwords: Vec<&str> = record
        .build_type
        .iter()
        .flat_map(|t| &t.parameters.property)
        .filter(|p| is_password(p))
        .map(|p| p.name.as_str())
        .collect();
    record
        .properties
        .property
        .iter()
        .map(|p| {
            let (value, secret) = drawn_value(
                &p.name,
                p.value.as_deref(),
                passwords.contains(&p.name.as_str()),
            );
            Setting {
                name: p.name.clone(),
                value,
                inherited: false,
                secret,
                editable: false,
                description: None,
            }
        })
        .collect()
}

/// A parameter's value as a view draws it, and whether it is a secret -- the
/// one rule both views use. Secret, and drawn empty, where `password` says the
/// spec is a password, where the name is one TeamCity stores securely, where
/// TeamCity masked the value, or where a URL's credential cannot be cut out;
/// any other URL is drawn without its credential.
pub(crate) fn drawn_value(name: &str, raw: Option<&str>, password: bool) -> (String, bool) {
    let raw = raw.unwrap_or_default();
    let masked = raw.len() >= 3 && raw.bytes().all(|b| b == b'*');
    let shown = if raw.contains("://") {
        without_userinfo(raw)
    } else {
        Some(raw.to_owned())
    };
    match shown {
        Some(shown) if !(password || is_secure_name(name) || masked) => (shown, false),
        _ => (String::new(), true),
    }
}

/// A parameter whose spec says `password`: its value is never drawn.
pub(crate) fn is_password(p: &crate::rest::Property) -> bool {
    p.spec
        .as_ref()
        .and_then(|t| t.raw_value.as_deref())
        .is_some_and(|raw| raw.trim_start().starts_with("password"))
}

/// A property TeamCity stores securely (`secure:password`, and on a build
/// `vcsroot.<root>.secure:password`): never drawn.
fn is_secure_name(name: &str) -> bool {
    name.starts_with("secure:") || name.contains(".secure:")
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

    /// The Builds frame's settings (#851): a root as a repository, its URL
    /// without the credential and its default branch; a trigger by its name
    /// with the properties picked by name, switched off said once; a password
    /// parameter without its value. Read off the record
    /// `BUILD_CONFIG_VIEW_FIELDS` asks for.
    #[test]
    fn a_configurations_settings_fill_the_pipeline_frame_scrubbed() {
        let record: ConfigRecord = serde_json::from_value(serde_json::json!({
            "id": "Payout_IntegrationTests",
            "parameters": { "property": [
                { "name": "env", "value": "staging", "inherited": false,
                  "type": { "rawValue": "select data_1='staging' data_2='prod' label_2='Production'" } },
                { "name": "deploy.token", "value": "s3cret", "inherited": true,
                  "type": { "rawValue": "password" } }
            ]},
            "triggers": { "trigger": [
                { "id": "t1", "type": "vcsTrigger", "properties": { "property": [
                    { "name": "branchFilter", "value": "+:*" } ]}},
                { "id": "t2", "type": "schedulingTrigger", "disabled": true,
                  "properties": { "property": [
                    { "name": "schedulingPolicy", "value": "daily" },
                    { "name": "hour", "value": "2" } ]}}
            ]},
            "vcs-root-entries": { "vcs-root-entry": [
                { "id": "Payout_Git", "vcs-root": { "id": "Payout_Git", "name": "payout-service",
                  "vcsName": "jetbrains.git", "properties": { "property": [
                    { "name": "url", "value": "https://knobas:s3cret@gitea.example/tidewater/payout-service.git" },
                    { "name": "branch", "value": "refs/heads/main" },
                    { "name": "secure:password", "value": null } ]}}}
            ]}
        }))
        .expect("a configuration record");
        let settings = pipeline_settings(&record);
        assert_eq!(settings.repositories.len(), 1);
        let repo = &settings.repositories[0];
        assert_eq!(repo.name, "payout-service");
        assert_eq!(
            repo.url.as_deref(),
            Some("https://gitea.example/tidewater/payout-service.git")
        );
        assert_eq!(repo.default_branch.as_deref(), Some("refs/heads/main"));
        assert_eq!(repo.entity_id, None, "the app's to fill, not the adapter's");

        // A trigger as (name, switched off, its facts as label/value pairs).
        type Drawn<'a> = (&'a str, bool, Vec<(&'a str, &'a str)>);
        let triggers: Vec<Drawn<'_>> = settings
            .triggers
            .iter()
            .map(|t| {
                (
                    t.name.as_str(),
                    t.disabled,
                    t.facts
                        .iter()
                        .map(|f| (f.label.as_str(), f.value.as_str()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            triggers,
            [
                ("VCS", false, vec![("Branch filter", "+:*")]),
                ("Schedule", true, vec![("When", "daily at 02:00")]),
            ]
        );

        let params: Vec<(&str, &str, bool)> = settings
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p.value.as_str(), p.secret))
            .collect();
        assert_eq!(
            params,
            [("env", "staging", false), ("deploy.token", "", true)],
            "in the source's order, a password without its value"
        );
        assert_eq!(
            settings.parameter_write_op.as_deref(),
            Some(crate::WRITE_OP_SET_PARAMETER)
        );

        // One spec per parameter, in the same order (#852): the select's
        // values, and the password as text, its secret kept by `secret`.
        use knobas_source::pipeline::{ParameterKind, ParameterOption};
        let specs: Vec<(&str, &ParameterKind)> = settings
            .parameter_specs
            .iter()
            .map(|p| (p.name.as_str(), &p.kind))
            .collect();
        assert_eq!(
            specs,
            [
                (
                    "env",
                    &ParameterKind::Select {
                        options: vec![
                            ParameterOption {
                                value: "staging".into(),
                                label: None
                            },
                            ParameterOption {
                                value: "prod".into(),
                                label: Some("Production".into())
                            },
                        ]
                    }
                ),
                ("deploy.token", &ParameterKind::Text),
            ]
        );
    }

    /// The steps a run takes (#1016, R-9.10): the enabled ones by name, in
    /// the configuration's order, which is the order a run's log numbers
    /// them in. A step the source keeps unnamed names none of them: its log
    /// heading is the runner's, which the record does not say.
    #[test]
    fn a_configurations_steps_are_named_in_order_without_the_disabled_ones() {
        let record = |steps: serde_json::Value| -> ConfigRecord {
            serde_json::from_value(serde_json::json!({
                "id": "Payout_Build",
                "steps": { "step": steps }
            }))
            .expect("a configuration record")
        };
        let named = pipeline_settings(&record(serde_json::json!([
            { "id": "RUNNER_1", "name": "Checkout", "type": "simpleRunner" },
            { "id": "RUNNER_2", "name": "Lint", "type": "simpleRunner", "disabled": true },
            { "id": "RUNNER_3", "name": "cargo test", "type": "simpleRunner" },
            { "id": "RUNNER_4", "name": "Publish", "type": "simpleRunner" }
        ])));
        assert_eq!(named.steps, ["Checkout", "cargo test", "Publish"]);

        let one_unnamed = pipeline_settings(&record(serde_json::json!([
            { "id": "RUNNER_1", "name": "Checkout", "type": "simpleRunner" },
            { "id": "RUNNER_2", "name": " ", "type": "simpleRunner" }
        ])));
        assert!(one_unnamed.steps.is_empty(), "{:?}", one_unnamed.steps);

        assert!(
            pipeline_settings(&record(serde_json::json!([])))
                .steps
                .is_empty()
        );
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
            default_branch: None,
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
