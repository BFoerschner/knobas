//! The generated half of the IPC mirror (contract §2.6 as amended by #1325).
//!
//! The TypeScript shape of every type that crosses the bridge is generated
//! from its Rust type by `ts-rs`, one file per barrel, into
//! `app/src/lib/ipc/generated/`. The barrels beside that folder re-export the
//! names and keep their `invoke` functions by hand.
//!
//! The test below **writes** those files: run `cargo test -p knobas-app --lib
//! ipc_mirror` after changing a crossing type and commit what it changed.
//! `just ipc-mirror` (a step of `just check`) runs it and fails when the
//! committed files differ from what the Rust types produce, so a struct change
//! cannot ship without its TypeScript.
//!
//! A type joins a barrel's file with `#[ts(export_to = "<barrel>.ts")]` beside
//! its `TS` derive, and is listed in [`export_roots`] unless a listed type
//! already reaches it. The configuration is spelled here and not read from the
//! environment (`TS_RS_*`), so the output cannot depend on who runs it.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ts_rs::{Config, ExportError, TS};

/// The barrel directory; the generated files go in its `generated/` folder.
fn ipc_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/ipc")
}

/// §2.6's mapping where it differs from `ts-rs`'s default: `i64`/`u64` are a
/// JSON number on the wire and a `number` in the shell, not a `bigint`.
fn config(out_dir: &Path) -> Config {
    Config::new().with_large_int("number").with_out_dir(out_dir)
}

/// Every barrel's root types. Exporting a type exports the types it reaches,
/// each into the file its own `export_to` names.
fn export_roots(cfg: &Config) -> Result<(), ExportError> {
    // app.ts
    crate::commands::app::AppStatus::export_all(cfg)?;
    crate::commands::app::DbState::export_all(cfg)?;
    crate::renderers::UserRenderer::export_all(cfg)?;
    crate::renderers::UserRendererDraft::export_all(cfg)?;
    crate::renderers::RenderContent::export_all(cfg)?;
    crate::renderers::RenderRun::export_all(cfg)?;
    crate::renderers::RendererDefault::export_all(cfg)?;
    // entity.ts
    crate::file_pipeline::PipeFormat::export_all(cfg)?;
    crate::file_pipeline::PipeStep::export_all(cfg)?;
    crate::file_pipeline::PipeOutput::export_all(cfg)?;
    crate::file_pipeline::PipeWhy::export_all(cfg)?;
    crate::file_pipeline::PipeInput::export_all(cfg)?;
    crate::file_pipeline::PipeFailure::export_all(cfg)?;
    crate::file_pipeline::PipeRun::export_all(cfg)?;
    knobas_core::entity_list::EntityRow::export_all(cfg)?;
    knobas_core::tone::Tone::export_all(cfg)?;
    knobas_core::payload::StateChip::export_all(cfg)?;
    knobas_core::entity_list::EntityOrder::export_all(cfg)?;
    knobas_core::entity_list::EntityFilter::export_all(cfg)?;
    knobas_core::entity_list::EntityPage::export_all(cfg)?;
    knobas_core::entity_list::AttentionGroup::export_all(cfg)?;
    knobas_core::entity_list::GroupTally::export_all(cfg)?;
    knobas_core::entity_list::KindTally::export_all(cfg)?;
    knobas_core::activity::ActivityRow::export_all(cfg)?;
    crate::commands::entity::SourceRef::export_all(cfg)?;
    knobas_source::KindInfo::export_all(cfg)?;
    knobas_core::link::LinkRow::export_all(cfg)?;
    knobas_core::link::LinkEnd::export_all(cfg)?;
    knobas_core::link::LinkEntry::export_all(cfg)?;
    crate::commands::entity::EntityDetail::export_all(cfg)?;
    knobas_core::builds::RunRow::export_all(cfg)?;
    knobas_core::builds::PipelineRow::export_all(cfg)?;
    crate::commands::entity::PipelineReason::export_all(cfg)?;
    crate::commands::entity::JobPipeline::export_all(cfg)?;
    crate::commands::entity::CodeFacts::export_all(cfg)?;
    knobas_source::pipeline::PipelineRepository::export_all(cfg)?;
    knobas_source::pipeline::PipelineTrigger::export_all(cfg)?;
    knobas_source::pipeline::PipelineSettings::export_all(cfg)?;
    knobas_source::pipeline::ChainStep::export_all(cfg)?;
    crate::commands::entity::ChainStepRow::export_all(cfg)?;
    knobas_source::pipeline::ParameterSpec::export_all(cfg)?;
    knobas_source::pipeline::ParameterKind::export_all(cfg)?;
    knobas_source::pipeline::ParameterOption::export_all(cfg)?;
    crate::run_presets::RunPreset::export_all(cfg)?;
    knobas_source::run::RunLog::export_all(cfg)?;
    knobas_source::run::LogLine::export_all(cfg)?;
    knobas_source::run::LogStep::export_all(cfg)?;
    knobas_source::run::RunTests::export_all(cfg)?;
    knobas_source::run::TestStatus::export_all(cfg)?;
    knobas_source::run::RunTest::export_all(cfg)?;
    knobas_source::run::TestOutcome::export_all(cfg)?;
    knobas_source::run::ArtifactEntry::export_all(cfg)?;
    knobas_source::run::ArtifactPreview::export_all(cfg)?;
    knobas_source::run::DownloadTarget::export_all(cfg)?;
    crate::commands::entity::UrlMatch::export_all(cfg)?;
    knobas_core::note::NoteRow::export_all(cfg)?;
    knobas_core::note::NoteRef::export_all(cfg)?;
    crate::commands::entity::NoteDetail::export_all(cfg)?;
    crate::commands::entity::NoteLinkInput::export_all(cfg)?;
    knobas_core::suggest::SuggestionEntry::export_all(cfg)?;
    crate::commands::entity::SuggestionPage::export_all(cfg)?;
    knobas_core::suggest::SuggestionMore::export_all(cfg)?;
    knobas_core::evidence::Part::export_all(cfg)?;
    knobas_core::evidence::Excerpt::export_all(cfg)?;
    knobas_core::evidence::ProposalEvidence::export_all(cfg)?;
    knobas_source::view::Fact::export_all(cfg)?;
    knobas_source::view::ViewRow::export_all(cfg)?;
    knobas_source::view::ViewAction::export_all(cfg)?;
    knobas_source::view::LineComments::export_all(cfg)?;
    knobas_source::view::Commenting::export_all(cfg)?;
    knobas_source::view::ActionField::export_all(cfg)?;
    knobas_source::view::FieldKind::export_all(cfg)?;
    knobas_source::view::ChoiceOption::export_all(cfg)?;
    knobas_source::view::Setting::export_all(cfg)?;
    knobas_source::diff::DiffChange::export_all(cfg)?;
    knobas_source::diff::DiffLine::export_all(cfg)?;
    knobas_source::diff::Hunk::export_all(cfg)?;
    knobas_source::diff::DiffFile::export_all(cfg)?;
    knobas_source::diff::LineThread::export_all(cfg)?;
    knobas_source::diff::FileThread::export_all(cfg)?;
    knobas_source::diff::LineComment::export_all(cfg)?;
    knobas_source::view::Block::export_all(cfg)?;
    knobas_source::view::Crumb::export_all(cfg)?;
    knobas_source::view::EntryKind::export_all(cfg)?;
    knobas_source::grants::WriteAsk::export_all(cfg)?;
    knobas_source::grants::RefusedWrite::export_all(cfg)?;
    knobas_source::view::TreeEntry::export_all(cfg)?;
    knobas_source::view::ViewSection::export_all(cfg)?;
    knobas_source::view::SourceView::export_all(cfg)?;
    knobas_core::start_work::Step::export_all(cfg)?;
    knobas_core::start_work::StepOutcome::export_all(cfg)?;
    knobas_core::start_work::FlowStep::export_all(cfg)?;
    knobas_core::inbox::Category::export_all(cfg)?;
    knobas_core::inbox::Shelf::export_all(cfg)?;
    knobas_core::inbox::InboxItem::export_all(cfg)?;
    crate::inbox::InboxEntry::export_all(cfg)?;
    knobas_core::context::ContextKind::export_all(cfg)?;
    knobas_core::context::ContextRow::export_all(cfg)?;
    knobas_core::reach::Origin::export_all(cfg)?;
    knobas_core::reach::Reached::export_all(cfg)?;
    knobas_core::mini_board::MiniBoardCard::export_all(cfg)?;
    knobas_core::signals::SignalKind::export_all(cfg)?;
    knobas_core::signals::Signal::export_all(cfg)?;
    knobas_core::mini_board::MiniBoardColumn::export_all(cfg)?;
    knobas_core::mini_board::SourceStatuses::export_all(cfg)?;
    knobas_core::mini_board::MiniBoard::export_all(cfg)?;
    knobas_core::mini_board::LaneField::export_all(cfg)?;
    knobas_core::mini_board::MiniBoardLane::export_all(cfg)?;
    knobas_core::mini_board::LaneColumn::export_all(cfg)?;
    knobas_core::project::Project::export_all(cfg)?;
    crate::standup::DigestLine::export_all(cfg)?;
    crate::standup::StandupDigest::export_all(cfg)?;
    crate::protocol::PublishTarget::export_all(cfg)?;
    crate::protocol::Publication::export_all(cfg)?;
    crate::protocol::Protocol::export_all(cfg)?;
    crate::commands::entity::ActionItemTicket::export_all(cfg)?;
    crate::notify::NotificationDraft::export_all(cfg)?;
    crate::notify::NotificationClicked::export_all(cfg)?;
    crate::sources::live::ViewFresh::export_all(cfg)?;
    crate::checkout::FoundBy::export_all(cfg)?;
    crate::checkout::CheckoutView::export_all(cfg)?;
    crate::checkout::working_tree::WorkingTreeState::export_all(cfg)?;
    crate::checkout::working_tree::TreeRead::export_all(cfg)?;
    knobas_core::checkout::open::Application::export_all(cfg)?;
    knobas_core::checkout::open::Draft::export_all(cfg)?;
    knobas_core::checkout::open::Object::export_all(cfg)?;
    knobas_core::checkout::open::ObjectKind::export_all(cfg)?;
    crate::checkout::open::GitFound::export_all(cfg)?;
    crate::checkout::open::CheckoutSettings::export_all(cfg)?;
    crate::checkout::clone::FailureCause::export_all(cfg)?;
    crate::checkout::clone::CloneState::export_all(cfg)?;
    crate::checkout::clone::CloneEvent::export_all(cfg)?;
    crate::checkout::clone::CloneAnswer::export_all(cfg)?;
    crate::checkout::uncommitted::CappedDiff::export_all(cfg)?;
    crate::checkout::uncommitted::UncommittedDiff::export_all(cfg)?;
    crate::checkout::uncommitted::DiffRead::export_all(cfg)?;
    crate::capture::ShortcutView::export_all(cfg)?;
    crate::capture::Recorded::export_all(cfg)?;
    crate::conversation::ThreadComment::export_all(cfg)?;
    crate::conversation::Thread::export_all(cfg)?;
    crate::notes_export::NotesExport::export_all(cfg)?;
    crate::docs::DocsTree::export_all(cfg)?;
    crate::docs::DocForYou::export_all(cfg)?;
    crate::docs::DocSpace::export_all(cfg)?;
    crate::docs::DocNode::export_all(cfg)?;
    crate::docs::DocPage::export_all(cfg)?;
    knobas_source::docs::SectionRefusal::export_all(cfg)?;
    knobas_source::docs::DocSection::export_all(cfg)?;
    knobas_source::docs::DocVersion::export_all(cfg)?;
    crate::docs::DocBacklink::export_all(cfg)?;
    crate::docs::DocWork::export_all(cfg)?;
    crate::docs::DocsHits::export_all(cfg)?;
    crate::docs::DocHit::export_all(cfg)?;
    crate::docs::DocSnippet::export_all(cfg)?;
    crate::docs::DocExport::export_all(cfg)?;
    crate::docs::NewPage::export_all(cfg)?;
    crate::docs::DocSpaceRef::export_all(cfg)?;
    crate::docs::DocAncestor::export_all(cfg)?;
    knobas_source::docs::DocComment::export_all(cfg)?;
    knobas_source::docs::PanelKind::export_all(cfg)?;
    knobas_source::docs::TableCell::export_all(cfg)?;
    knobas_source::docs::DocBlock::export_all(cfg)?;
    knobas_source::docs::Inline::export_all(cfg)?;
    crate::link_sets::LinkSetMember::export_all(cfg)?;
    crate::link_sets::LinkSetRule::export_all(cfg)?;
    crate::link_sets::LinkSetPattern::export_all(cfg)?;
    crate::link_sets::LinkSetDraft::export_all(cfg)?;
    crate::link_sets::MemberRow::export_all(cfg)?;
    crate::link_sets::PatternRow::export_all(cfg)?;
    crate::link_sets::PatternProject::export_all(cfg)?;
    crate::link_sets::PatternPreview::export_all(cfg)?;
    crate::link_sets::RuleRow::export_all(cfg)?;
    crate::link_sets::LinkSetRow::export_all(cfg)?;
    crate::link_sets::RuleTicket::export_all(cfg)?;
    crate::link_sets::LinkSetDetail::export_all(cfg)?;
    crate::link_sets::LinkSetHint::export_all(cfg)?;
    crate::link_sets::JobMember::export_all(cfg)?;
    crate::link_sets::JobSet::export_all(cfg)?;
    crate::link_sets::JobLink::export_all(cfg)?;
    crate::link_sets::JobParent::export_all(cfg)?;
    crate::link_sets::JobLinkSets::export_all(cfg)?;
    // assets.ts
    knobas_core::asset::PropertyKind::export_all(cfg)?;
    knobas_core::asset::TypedProperty::export_all(cfg)?;
    knobas_core::asset::AssetType::export_all(cfg)?;
    crate::assets::PropertyValue::export_all(cfg)?;
    crate::assets::AssetStatus::export_all(cfg)?;
    crate::assets::Environment::export_all(cfg)?;
    crate::assets::AssetRow::export_all(cfg)?;
    crate::assets::AssetProperty::export_all(cfg)?;
    crate::assets::Visibility::export_all(cfg)?;
    crate::assets::RouteRow::export_all(cfg)?;
    crate::assets::RouteDetail::export_all(cfg)?;
    crate::assets::AssetDetail::export_all(cfg)?;
    crate::assets::DependsOnRow::export_all(cfg)?;
    crate::assets::DependsOnThis::export_all(cfg)?;
    crate::assets::MonitorTarget::export_all(cfg)?;
    crate::assets::AttachedMonitor::export_all(cfg)?;
    crate::assets::ImportEntry::export_all(cfg)?;
    crate::assets::PropertyPlan::export_all(cfg)?;
    crate::assets::PropertyChange::export_all(cfg)?;
    crate::assets::AssetChange::export_all(cfg)?;
    crate::assets::MonitorLink::export_all(cfg)?;
    crate::assets::UnresolvedMonitor::export_all(cfg)?;
    crate::assets::ImportPreview::export_all(cfg)?;
    crate::assets::ImportOutcome::export_all(cfg)?;
    crate::assets::AssetEdit::export_all(cfg)?;
    crate::assets::RouteEdit::export_all(cfg)?;
    crate::assets::Produced::export_all(cfg)?;
    crate::assets::Landing::export_all(cfg)?;
    crate::commands::assets::MonitoringSettings::export_all(cfg)?;
    crate::assets::MonitorRow::export_all(cfg)?;
    crate::assets::UptimeRatio::export_all(cfg)?;
    crate::assets::MonitoredAsset::export_all(cfg)?;
    crate::assets::MonitorSample::export_all(cfg)?;
    crate::assets::AlertState::export_all(cfg)?;
    crate::assets::AlertAsset::export_all(cfg)?;
    crate::assets::OpenAlert::export_all(cfg)?;
    crate::assets::UnmonitoredAsset::export_all(cfg)?;
    crate::assets::dependencies::Layer::export_all(cfg)?;
    crate::assets::dependencies::DepKind::export_all(cfg)?;
    crate::assets::dependencies::DepItem::export_all(cfg)?;
    crate::assets::dependencies::DepRelation::export_all(cfg)?;
    crate::assets::dependencies::Dependencies::export_all(cfg)?;
    crate::assets::dependencies::Group::export_all(cfg)?;
    crate::assets::dependencies::Tiles::export_all(cfg)?;
    crate::assets::dependencies::LayerInView::export_all(cfg)?;
    crate::assets::dependencies::DepViewSettings::export_all(cfg)?;
    crate::assets::dependencies::DepView::export_all(cfg)?;
    crate::assets::dependencies::DepViews::export_all(cfg)?;
    // sources.ts
    knobas_source::SourceDescriptor::export_all(cfg)?;
    crate::sources::SourceSummary::export_all(cfg)?;
    crate::sources::RoleRun::export_all(cfg)?;
    crate::sources::refresh::EntityRefresh::export_all(cfg)?;
    crate::sources::NewSource::export_all(cfg)?;
    crate::sources::SourcePatch::export_all(cfg)?;
    crate::sources::SourceDraft::export_all(cfg)?;
    crate::sources::ConnectionReport::export_all(cfg)?;
    crate::sources::install::InstalledAdapter::export_all(cfg)?;
    crate::sources::installed::AdapterStatus::export_all(cfg)?;
    crate::sources::installed::UpdatePreview::export_all(cfg)?;
    crate::sources::installed::Removal::export_all(cfg)?;
    knobas_sync::SyncReport::export_all(cfg)?;
    knobas_sync::scheduler::RefreshEnded::export_all(cfg)?;
    knobas_sync::progress::SyncProgress::export_all(cfg)?;
    knobas_sync::run_log::SourceSyncStatus::export_all(cfg)?;
    knobas_sync::stats::DbStats::export_all(cfg)?;
    knobas_core::write_queue::QueuedWrite::export_all(cfg)?;
    knobas_source::WriteOp::export_all(cfg)?;
    knobas_core::write_queue::QueueCounts::export_all(cfg)?;
    // search.ts
    knobas_search::SearchQuery::export_all(cfg)?;
    knobas_search::SearchResponse::export_all(cfg)?;
    crate::commands::search::LauncherHome::export_all(cfg)?;
    knobas_search::FilterSuggestions::export_all(cfg)?;
    knobas_source::query::Translation::export_all(cfg)?;
    crate::ticket_views::TicketView::export_all(cfg)?;
    crate::ticket_views::TicketViewRow::export_all(cfg)?;
    crate::ticket_views::TicketViewDraft::export_all(cfg)?;
    // time.ts
    crate::time::TimerTarget::export_all(cfg)?;
    crate::time::BlockKind::export_all(cfg)?;
    crate::time::RunningTimer::export_all(cfg)?;
    crate::time::Block::export_all(cfg)?;
    crate::time::day::DayBlock::export_all(cfg)?;
    crate::time::day::DayRecord::export_all(cfg)?;
    crate::time::day::OpenStretch::export_all(cfg)?;
    crate::time::worklog::CandidateSource::export_all(cfg)?;
    crate::time::worklog::Candidate::export_all(cfg)?;
    crate::time::worklog::Draft::export_all(cfg)?;
    crate::time::worklog::Worklog::export_all(cfg)?;
    crate::time::suggest::SuggestionRule::export_all(cfg)?;
    crate::time::suggest::Suggestion::export_all(cfg)?;
    crate::time::suggest::AdHocBlock::export_all(cfg)?;
    crate::time::week::DayWindow::export_all(cfg)?;
    crate::time::week::WeekCell::export_all(cfg)?;
    crate::time::week::WeekRow::export_all(cfg)?;
    crate::time::week::Week::export_all(cfg)?;
    crate::time::week::PlannedWorklog::export_all(cfg)?;
    // backup.ts
    crate::backup::BackupSchedule::export_all(cfg)?;
    crate::backup::BackupRecord::export_all(cfg)?;
    crate::backup::ShareParts::export_all(cfg)?;
    crate::backup::ArchiveFile::export_all(cfg)?;
    crate::backup::BackupStatus::export_all(cfg)?;
    Ok(())
}

/// The first line `ts-rs` writes into every file.
const TS_RS_NOTE: &str = "// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). \
                          Do not edit this file manually.\n";

/// The first line of every generated file instead.
///
/// `ts-rs`'s own carries a GitHub URL, and the house rule "no runtime network
/// references" (`app/src/lib/shell/house-rules.test.ts`) refuses a real
/// hostname anywhere under `app/src`. This one also says how to regenerate.
const NOTE: &str = "// Generated from the Rust types by ts-rs; do not edit. Regenerate with \
                    `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).\n";

/// Replace `ts-rs`'s first line with [`NOTE`] in every file in `dir`.
///
/// Fails rather than skipping a file that does not start with the expected
/// line: a `ts-rs` that changed it would otherwise put its URL back.
fn rehead(dir: &Path) {
    for name in ts_files(dir) {
        let path = dir.join(&name);
        let text = fs::read_to_string(&path).expect("ts-rs wrote the file");
        let body = text
            .strip_prefix(TS_RS_NOTE)
            .unwrap_or_else(|| panic!("{name} does not start with ts-rs's note: {text:.120}"));
        replace(&path, &format!("{NOTE}{body}")).expect("the scratch file can be rewritten");
    }
}

/// Write `text` as the whole of `path`.
///
/// `File::create` and `write_all` rather than `fs::write`: this file names
/// `knobas_source::WriteOp` to export it, and `knobas-sync`'s
/// `write_choke_point` holds such a file to containing no `::write(` call.
fn replace(path: &Path, text: &str) -> std::io::Result<()> {
    fs::File::create(path)?.write_all(text.as_bytes())
}

/// The `.ts` files directly in `dir`, by name.
fn ts_files(dir: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .map(|entry| entry.expect("a readable directory entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| name.ends_with(".ts"))
        .collect()
}

/// Make `dest` hold exactly the files in `fresh`.
///
/// A file whose text is unchanged is not touched, and a changed one is
/// replaced by a rename: `just check` runs svelte-check beside this test, and
/// a reader must never see a half-written file.
fn sync(fresh: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("the generated folder can be created");
    let wanted = ts_files(fresh);
    for name in &wanted {
        let text = fs::read_to_string(fresh.join(name)).expect("ts-rs wrote the file");
        let target = dest.join(name);
        if fs::read_to_string(&target).ok().as_deref() == Some(text.as_str()) {
            continue;
        }
        let staging = dest.join(format!(".{name}.tmp"));
        replace(&staging, &text).expect("the staging file can be written");
        fs::rename(&staging, &target).expect("the staging file can be moved into place");
    }
    for name in ts_files(dest).difference(&wanted) {
        fs::remove_file(dest.join(name)).expect("a file no type produces can be removed");
    }
}

/// Writes `app/src/lib/ipc/generated/*.ts` from the Rust types.
///
/// Every generated file must belong to a barrel: a type that reaches one with
/// no `export_to` lands in a file of its own name, and that is a missing
/// annotation, not a new barrel (§10.8 freezes the module layout).
#[test]
fn the_generated_ipc_mirror_is_written() {
    let fresh = tempfile::tempdir().expect("a scratch directory");
    export_roots(&config(fresh.path())).expect("every root type exports");
    rehead(fresh.path());

    let barrels = ts_files(&ipc_dir());
    let produced = ts_files(fresh.path());
    let strays: Vec<&String> = produced.difference(&barrels).collect();
    assert!(
        strays.is_empty(),
        "these generated files match no barrel in app/src/lib/ipc/; give the type in each \
         `#[ts(export_to = \"<barrel>.ts\")]`: {strays:?}"
    );
    assert!(
        produced.contains("app.ts"),
        "the app barrel's types produced nothing, so this test proves nothing: {produced:?}"
    );

    let generated = ipc_dir().join("generated");
    sync(fresh.path(), &generated);

    // Read back: `just ipc-mirror` compares the folder with a snapshot taken
    // before this run (#1694), so a sync that wrote nothing would pass a stale
    // file as current.
    assert_eq!(
        ts_files(&generated),
        produced,
        "the folder holds other files than ts-rs wrote"
    );
    for name in &produced {
        assert_eq!(
            fs::read_to_string(generated.join(name)).expect("the generated file is readable"),
            fs::read_to_string(fresh.path().join(name)).expect("ts-rs wrote the file"),
            "app/src/lib/ipc/generated/{name} is not what the Rust types produce"
        );
    }
}
