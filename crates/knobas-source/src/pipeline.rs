//! The Builds frame's two live reads of a pipeline (ADR-0022, issue #851).
//!
//! The frame draws a pipeline and its runs over declared paths
//! ([`KindPaths`](crate::KindPaths), #848). Two things a pipeline page needs
//! are not in any record the mirror holds:
//!
//! * **Its settings**: the repository it builds, its default branch, its
//!   triggers and its parameters. They live in the configuration's settings,
//!   which carry credentials (a VCS root's password, a `password` parameter),
//!   so they are read live, scrubbed by the adapter, and never stored. The
//!   answer is [`PipelineSettings`], a shape the shell draws without knowing
//!   the source, the way [`docs::DocContent`](crate::docs::DocContent) is for
//!   a page.
//! * **Its older runs**: the mirror holds a window of each pipeline's newest
//!   runs (the sync budget). History beyond it is asked of the source when the
//!   reader scrolls past the end, and answered as [`SyncItem`](crate::SyncItem)s
//!   of the run kind, so the app reads them through the same declared paths as
//!   a mirrored run and the shell cannot tell the two apart.
//!
//! **Frozen once landed.** A field is a §10.8 entry, as a declared path is.
//!
//! Every string is **source text**: the shell interpolates it and never
//! renders it as markup.

use serde::{Deserialize, Serialize};

use crate::view::{Fact, Setting};

/// How many older runs one read asks for at most.
pub const OLDER_RUNS_MAX: u32 = 100;

/// A pipeline's settings, read live (#851).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineSettings {
    /// The repositories the pipeline checks out, in the source's order. Most
    /// pipelines build one.
    pub repositories: Vec<PipelineRepository>,
    /// What starts a run on its own, in the source's order.
    pub triggers: Vec<PipelineTrigger>,
    /// The parameters every run starts from, in the source's order. A secret's
    /// value is never sent (`Setting::secret`).
    pub parameters: Vec<Setting>,
    /// The write op a parameter's default is changed through, where the
    /// source declares one (`set_parameter`); `None` draws them read-only.
    pub parameter_write_op: Option<String>,
    /// How each parameter is asked for in the Run… dialog (#852, R-9.7): one
    /// per entry of `parameters`, by name, in the same order. Absent from an
    /// older peer's answer, which reads as every parameter plain text.
    #[serde(default)]
    pub parameter_specs: Vec<ParameterSpec>,
    /// The chain the pipeline is a step of (#857, R-9.13): every pipeline it
    /// is joined to by the source's dependencies (TeamCity's snapshot
    /// dependencies), this one included, in the order they run. Empty for a
    /// pipeline in no chain, and absent from an older peer's answer.
    #[serde(default)]
    pub chain: Vec<ChainStep>,
}

/// One pipeline of a chain (#857).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainStep {
    /// The pipeline's [`EntityRef`](knobas_core::entity::EntityRef) in string
    /// form, of the same source.
    pub pipeline: String,
    /// What the source calls the pipeline.
    pub name: String,
}

/// One step of the chain a run is in (#857), as [`Source::run_chain`]
/// answers it: the step, and its run on the run's branch.
///
/// [`Source::run_chain`]: crate::Source::run_chain
#[derive(Debug, Clone)]
pub struct RunChainStep {
    pub step: ChainStep,
    /// The step's run: the run asked about for its own step, the run it built
    /// on for an earlier step, else the step's newest run on the same branch.
    /// The [`SyncItem`](crate::SyncItem) the sync would have made of it, so
    /// the app reads it through the run kind's declared paths. `None` where
    /// the step has not run on the branch.
    pub run: Option<crate::SyncItem>,
}

/// How one parameter is asked for when a run is started (#852): the
/// source's own spec of it, parsed. A parameter the source says nothing
/// about, or says something the adapter cannot read, is [`ParameterKind::Text`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterSpec {
    /// The parameter's full name, as in [`PipelineSettings::parameters`].
    pub name: String,
    /// What the source shows instead of the name, where it sets one.
    pub label: Option<String>,
    /// What the source says about the value, such as what empty means.
    pub hint: Option<String>,
    pub kind: ParameterKind,
}

/// The field a parameter is asked with. Tagged on `type`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParameterKind {
    /// Free text: anything typed is the value.
    #[default]
    Text,
    /// One of a fixed list, in the source's order.
    Select { options: Vec<ParameterOption> },
    /// On or off, each with the value it sends.
    Checkbox { checked: String, unchecked: String },
}

/// One value a [`ParameterKind::Select`] offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterOption {
    /// What the run sends.
    pub value: String,
    /// What the source shows for it, where it differs from the value.
    pub label: Option<String>,
}

/// One repository a pipeline builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineRepository {
    /// What the source calls the checkout.
    pub name: String,
    /// Where it fetches from, never with a credential. `None` where the
    /// adapter could not take the credential out, or the source names none.
    pub url: Option<String>,
    /// The branch a run builds when it names none.
    pub default_branch: Option<String>,
    /// The mirrored repository this URL is, filled by the app (the adapter
    /// cannot see another source's mirror) and `None` where the mirror holds
    /// none. An adapter leaves it `None`.
    #[serde(default)]
    pub entity_id: Option<String>,
}

/// One trigger of a pipeline: its kind in the source's word and what it
/// fires on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineTrigger {
    /// The kind of trigger, in the source's word (`VCS`, `Schedule`).
    pub name: String,
    /// When and for what it fires, as labelled values (`When`: `daily at
    /// 02:00`). Only what the adapter knows to be safe to show.
    pub facts: Vec<Fact>,
    /// Switched off at the source.
    pub disabled: bool,
}
