/**
 * Entity and room reads — `crates/knobas-app/src/commands/entity/`.
 *
 * The types are generated from the Rust (`./generated/entity.ts`, contract
 * §2.6 as amended by #1325, switched by #1326): a field is explained in its
 * Rust doc comment and nowhere else. The `invoke` functions stay
 * hand-written, pinned to the Rust by a test in that module which
 * `include_str!`s this file, and so do {@link LanesAsked} and {@link OpenAt},
 * which bundle a function's arguments rather than mirror a Rust type.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ActionItemTicket,
  ArtifactEntry,
  ArtifactPreview,
  CaptureContext,
  CaptureShortcut,
  ChainStepRow,
  CheckoutApplicationDraft,
  CheckoutSettings,
  CheckoutView,
  OpenObject,
  CloneAnswer,
  CodeFacts,
  ContextRow,
  DiffRead,
  DocExport,
  DocPage,
  DocsHits,
  DocsTree,
  DownloadTarget,
  EntityDetail,
  EntityFilter,
  EntityPage,
  InboxCategory,
  InboxEntry,
  InboxShelf,
  JobLinkSets,
  JobPipeline,
  LaneField,
  LinkRow,
  LinkSetDetail,
  LinkSetDraft,
  LinkSetHint,
  LinkSetPattern,
  LinkSetPatternPreview,
  LinkSetRow,
  LinkSetRule,
  MiniBoard,
  NewPage,
  NoteDetail,
  NoteLinkInput,
  NotesExport,
  NotificationDraft,
  PipeRun,
  PipeStep,
  PipelineRow,
  PipelineSettings,
  Project,
  ProposalEvidence,
  Protocol,
  PublishTarget,
  PullRequestWorktree,
  RangeDiff,
  Reached,
  RefusedWrite,
  RuleTicket,
  RunLog,
  RunPreset,
  RunRow,
  RunTests,
  SourceView,
  StandupDigest,
  StartWorkStep,
  SuggestionPage,
  TestOutcome,
  Thread,
  UrlMatch,
  WriteAsk,
} from "./generated/entity";
// The write queue's own types live with the queue's other commands. A
// write-back is an operation on an entity, which is why `submitWrite` is here;
// what it hands over and what comes back are the queue's, and a second
// declaration of either would be two shapes for one wire format.
import type { QueuedWrite, WriteOpPayload } from "./sources";
// A day is a day wherever it is asked for: the digest takes the same
// `DayWindow` the timesheet does rather than declaring a second shape for one
// wire format, which is the rule the import above follows for the queue's.
import type { DayWindow } from "./time";

export type {
  ActionField,
  ActionItemTicket,
  ActivityRow,
  ArtifactEntry,
  ArtifactPreview,
  AttentionGroup,
  CappedDiff,
  CaptureContext,
  CaptureShortcut,
  ChainStep,
  ChainStepRow,
  CheckoutApplication,
  CheckoutApplicationDraft,
  CheckoutFoundBy,
  CheckoutSettings,
  CheckoutView,
  OpenObject,
  OpenObjectKind,
  ChoiceOption,
  CloneAnswer,
  CloneEvent,
  CloneFailureCause,
  CloneState,
  CodeFacts,
  Commenting,
  ContextKind,
  ContextRow,
  Crumb,
  DiffChange,
  DiffFile,
  DiffLine,
  DiffRange,
  DiffRead,
  DiffReadAt,
  DigestLine,
  DocAncestor,
  DocBacklink,
  DocBlock,
  DocComment,
  DocExport,
  DocForYou,
  DocHit,
  DocInline,
  DocNode,
  DocPage,
  DocSection,
  DocSnippet,
  DocSpace,
  DocSpaceRef,
  DocVersion,
  DocWork,
  DocWrite,
  DocsHits,
  DocsTree,
  DownloadTarget,
  EntityDetail,
  EntityFilter,
  EntityOrder,
  EntityPage,
  EntityRow,
  EntryKind,
  EvidenceExcerpt,
  EvidencePart,
  Fact,
  FieldKind,
  FileThread,
  GitFound,
  GroupTally,
  Hunk,
  InboxCategory,
  InboxEntry,
  InboxItem,
  InboxShelf,
  JobLink,
  JobLinkSets,
  JobMember,
  JobParent,
  JobPipeline,
  JobSet,
  KindInfo,
  KindTally,
  LaneColumn,
  LaneField,
  LineComment,
  LineComments,
  LineThread,
  LinkEnd,
  LinkEntry,
  LinkRow,
  LinkSetDetail,
  LinkSetDraft,
  LinkSetHint,
  LinkSetMember,
  LinkSetMemberKind,
  LinkSetMemberRow,
  LinkSetPattern,
  LinkSetPatternPreview,
  LinkSetPatternProject,
  LinkSetPatternRow,
  LinkSetRow,
  LinkSetRule,
  LinkSetRuleKind,
  LinkSetRuleRow,
  LogLine,
  LogStep,
  MiniBoard,
  MiniBoardCard,
  MiniBoardColumn,
  MiniBoardLane,
  NewPage,
  NoteDetail,
  NoteLinkInput,
  NoteRef,
  NoteRow,
  NotesExport,
  NotificationClicked,
  NotificationDraft,
  PanelKind,
  ParameterKind,
  ParameterOption,
  ParameterSpec,
  PipeFailure,
  PipeFormat,
  PipeInput,
  PipeOutput,
  PipeRun,
  PipeStep,
  PipeWhy,
  PipelineReason,
  PipelineRepository,
  PipelineRow,
  PipelineSettings,
  PipelineTrigger,
  Project,
  ProposalEvidence,
  Protocol,
  Publication,
  PublishTarget,
  PullRequestWorktree,
  RangeCommit,
  RangeDiff,
  ReachOrigin,
  Reached,
  RefusedWrite,
  RuleTicket,
  RunLog,
  RunPreset,
  RunRow,
  RunTest,
  RunTests,
  SectionRefusal,
  Setting,
  Signal,
  SignalKind,
  SourceRef,
  SourceStatuses,
  SourceView,
  StandupDigest,
  StartWorkOutcome,
  StartWorkStep,
  StartWorkStepKind,
  StateChip,
  SuggestionEntry,
  SuggestionMore,
  SuggestionPage,
  TableCell,
  TestOutcome,
  TestStatus,
  TestSummary,
  Thread,
  ThreadComment,
  Tone,
  TreeEntry,
  TreeRead,
  UncommittedDiff,
  UrlMatch,
  ViewAction,
  ViewBlock,
  ViewFresh,
  ViewRow,
  ViewSection,
  WorkingTreeState,
  WriteAsk,
} from "./generated/entity";

/**
 * The most rows `list_entities` and `room_suggestions` answer in one read:
 * `knobas_app::commands::ROW_CEILING` (#1643). A test in
 * `commands/entity/mod.rs` holds the two equal.
 */
export const ROW_CEILING = 500;

/**
 * The most rows one {@link listEntities} call gathers: two reads, the 1000
 * the code, repository and pull request views ask for (#1643).
 */
export const LIST_MOST = 2 * ROW_CEILING;

/**
 * One page of the room `filter` addresses, newest (or first) `limit` rows.
 *
 * The backend answers at most {@link ROW_CEILING} rows a read, so a `limit`
 * past it is read in reads of that size from `offset` on, up to
 * {@link LIST_MOST} rows (#1643): no one payload carries more than the
 * ceiling, and a view that asks for 1000 still gets 1000. `total`, `tally`
 * and `groups` are the first read's; a row a later read repeats (the room
 * moved between the reads) is kept once.
 */
export async function listEntities(
  filter: EntityFilter,
  limit: number,
  offset: number,
): Promise<EntityPage> {
  const want = Math.min(limit, LIST_MOST);
  const first = await invoke<EntityPage>("list_entities", { filter, limit: Math.min(want, ROW_CEILING), offset });
  const rows = [...first.rows];
  const seen = new Set(rows.map((row) => row.entity_id));
  let read = first.rows.length;
  let last = read;
  while (last === ROW_CEILING && read < want) {
    const page = await invoke<EntityPage>("list_entities", {
      filter,
      limit: Math.min(want - read, ROW_CEILING),
      offset: offset + read,
    });
    for (const row of page.rows) {
      if (!seen.has(row.entity_id)) {
        seen.add(row.entity_id);
        rows.push(row);
      }
    }
    last = page.rows.length;
    read += last;
  }
  return read === first.rows.length ? first : { ...first, rows };
}

/**
 * One entity, deleted or not.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for one nothing carries — a deep link into a corpus that has not
 * synced yet is a normal event, not a bug.
 */
export function getEntity(entityId: string): Promise<EntityDetail> {
  return invoke<EntityDetail>("get_entity", { entityId });
}

/**
 * Every pipeline the sources filling the CI role declare, by name (#848). The
 * read narrows itself to CI's fillers (#1295, §10.8's #1294 entry): a source
 * with a CI miss, one that claims no CI and a removed source's leftovers draw
 * none.
 */
export function pipelines(): Promise<PipelineRow[]> {
  return invoke<PipelineRow[]>("pipelines");
}

/**
 * One pipeline's runs, newest first, at most `limit` (100 when omitted).
 * Rejects with `invalid` for a non-id and `not_found` for no pipeline.
 */
export function pipelineRuns(pipelineId: string, limit?: number): Promise<RunRow[]> {
  return invoke<RunRow[]>("pipeline_runs", { pipelineId, limit: limit ?? null });
}

/**
 * The files of pull request `prId` the reader marked *Viewed* on its current
 * head (#842, R-8.17), in path order — `pr_viewed_files`. Knobas-local, kept
 * by pull request and the head commit its source declares: once the mirror
 * reads another head they read as none.
 */
export function prViewedFiles(prId: string, blobs: Record<string, string> = {}): Promise<string[]> {
  return invoke<string[]>("pr_viewed_files", { prId, blobs });
}

/** Mark one file of a pull request *Viewed* or not on its current head (#842), answering the files now viewed. */
export function setPrFileViewed(prId: string, path: string, viewed: boolean, blobs: Record<string, string> = {}): Promise<string[]> {
  return invoke<string[]>("set_pr_file_viewed", { prId, path, viewed, blobs });
}

/** Pin or unpin a pipeline (#849); answers whether it is now pinned. */
export function setPipelinePinned(pipelineId: string, pinned: boolean): Promise<boolean> {
  return invoke<boolean>("set_pipeline_pinned", { pipelineId, pinned });
}

/** The pipelines on the job of a ticket, by entity id (#850). */
export function jobPipelines(entityId: string): Promise<JobPipeline[]> {
  return invoke<JobPipeline[]>("job_pipelines", { entityId });
}

/** {@link CodeFacts} for each id the mirror holds live; the others are left out (#966). */
export function codeFacts(entityIds: string[]): Promise<CodeFacts[]> {
  return invoke<CodeFacts[]>("code_facts", { entityIds });
}

/**
 * Runs of a pipeline older than the run `before`, asked of the source: the
 * history beyond the mirror's window (#851), newest first, at most `limit`
 * (50 when omitted). Empty is the start of the history. Rejects as
 * {@link pipelineRuns} does, and with the source's own failure.
 */
export function olderPipelineRuns(pipelineId: string, before: string, limit?: number): Promise<RunRow[]> {
  return invoke<RunRow[]>("older_pipeline_runs", { pipelineId, before, limit: limit ?? null });
}

/** The chain a run is in, read live, in running order; empty for none (#857). */
export function runChain(runId: string): Promise<ChainStepRow[]> {
  return invoke<ChainStepRow[]>("run_chain", { runId });
}

/** A pipeline's Run… presets, in the order they were first saved (#852). */
export function runPresets(pipelineId: string): Promise<RunPreset[]> {
  return invoke<RunPreset[]>("run_presets", { pipelineId });
}

/** Save a Run… preset, replacing the pipeline's preset of that name (#852). */
export function saveRunPreset(preset: RunPreset): Promise<RunPreset> {
  return invoke<RunPreset>("save_run_preset", { preset });
}

/** Remove a pipeline's Run… preset by name; `not_found` for a name it holds none of (#996). */
export function deleteRunPreset(pipelineId: string, name: string): Promise<void> {
  return invoke<void>("delete_run_preset", { pipelineId, name });
}

/** One pipeline's settings, read live; `null` for a source with no pipelines (#851). */
export function pipelineSettings(pipelineId: string): Promise<PipelineSettings | null> {
  return invoke<PipelineSettings | null>("pipeline_settings", { pipelineId });
}

/**
 * A range of a run's whole log and its steps (#854): `count` lines (the cap
 * when omitted) from line `from`, or the last `count` when `from` is `null`.
 * `null` for a source that keeps no runs.
 */
export function runLog(runId: string, from: number | null = null, count: number | null = null): Promise<RunLog | null> {
  return invoke<RunLog | null>("run_log", { runId, from, count });
}

/** A run's tests, failed first (#855). */
export function runTests(runId: string): Promise<RunTests | null> {
  return invoke<RunTests | null>("run_tests", { runId });
}

/** How one test came out in the last ten runs of the run's pipeline, newest first (#855). */
export function testHistory(runId: string, testId: string): Promise<TestOutcome[] | null> {
  return invoke<TestOutcome[] | null>("test_history", { runId, testId });
}

/** One folder of a run's artifacts; `null` path is the root (#856). */
export function runArtifacts(runId: string, path: string | null = null): Promise<ArtifactEntry[] | null> {
  return invoke<ArtifactEntry[] | null>("run_artifacts", { runId, path });
}

/** The head of one text artifact, for *Preview* (#856). */
export function artifactPreview(runId: string, path: string): Promise<ArtifactPreview | null> {
  return invoke<ArtifactPreview | null>("artifact_preview", { runId, path });
}

/**
 * Stream a log, file, folder or every artifact into the downloads folder
 * with the stored credential (D8, #856); the saved file's path, or `null`
 * for a source that keeps no runs.
 */
export function downloadRunFile(runId: string, target: DownloadTarget): Promise<string | null> {
  return invoke<string | null>("download_run_file", { runId, target });
}

/**
 * The entity a pasted URL names, or `null` for a link the mirror does not
 * hold — `resolve_url` (#496, spec #491 stories 9-11 and 15-17).
 *
 * One read on the local mirror and **never a fetch from a source**: a paste
 * while the tunnel is down is a miss and not a hang (story 16). Matching is
 * exact on the stored `web_url` after both sides are normalised — fragment
 * out, the path's trailing slash out, scheme and host down-cased, query kept
 * verbatim — so a Confluence `viewpage.action?pageId=` link resolves and a
 * different `pageId` does not.
 *
 * Rejects with `invalid` for anything that is not an absolute `http`/`https`
 * URL. That is not a miss: a miss is *the mirror does not hold this link*, and
 * it is the only outcome that earns *Open in browser*, which refuses every
 * other scheme anyway (`lib/shell/open-external.ts`).
 */
export function resolveUrl(url: string): Promise<UrlMatch | null> {
  return invoke<UrlMatch | null>("resolve_url", { url });
}

/**
 * Draw a link between two entities — `knobas_app::commands::entity::create_link`.
 *
 * `relation` defaults to `"related"` and is folded to lower case; `note` is
 * optional and kept as typed — it is prose, not a group key. Both are
 * normalized backend-side, so a field the user left alone may be sent as `""`.
 * The origin is always `"manual"` and is deliberately not suppliable: the
 * other origins belong to the suggestion engine and to import.
 *
 * There is no matching read: an entity's links arrive with `getEntity`, and
 * they are undirected, so a link drawn from either end is on both.
 *
 * Rejects with `invalid` for an id that is not an entity id or for an entity
 * linked to itself, `not_found` when an endpoint is not in the local mirror —
 * it has not synced yet — and `conflict` when that pair is already linked
 * under that relation ("already linked"). Emits `EVENTS.activityNew`.
 *
 * The `conflict` is **directed**: the same pair linked the other way round is
 * not refused, and shows as a second row on both ends. Known — see #70.
 */
export function createLink(
  fromId: string,
  toId: string,
  relation?: string,
  note?: string,
): Promise<LinkRow> {
  return invoke<LinkRow>("create_link", { fromId, toId, relation, note });
}

/**
 * Withdraw a link — `knobas_app::commands::entity::unlink`.
 *
 * The row is kept, tombstoned, so the removal is remembered and the same pair
 * can be linked again afterwards. Idempotent: withdrawing an already-withdrawn
 * link resolves, writes no second activity line and emits nothing.
 *
 * Rejects with `invalid` for something that is not a link id and `not_found`
 * for one nothing carries. Emits `EVENTS.activityNew`.
 */
export function unlink(linkId: string): Promise<void> {
  return invoke<void>("unlink", { linkId });
}

/**
 * One note — `knobas_app::commands::entity::get_note`.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for a note that does not exist — including one deleted in
 * another window.
 */
export function getNote(noteId: string): Promise<NoteDetail> {
  return invoke<NoteDetail>("get_note", { noteId });
}

/**
 * Write a new note — `knobas_app::commands::entity::create_note`.
 *
 * Every argument is optional: *New note* creates the row before the first
 * keystroke, so nothing typed into it can be lost to a closed window.
 *
 * `links` is what a **capture** attaches (`CONTEXT.md`, **Capture**):
 * `captured-in` to the context of the stored room the reader stands in, which
 * makes the note a member of it by ADR-0008, and `captured-from` to the
 * **foreground** — the open detail, else the room's anchor, else nothing, the
 * word as `CONTEXT.md`'s **Passive attribution** defines it and as the
 * heartbeat computes it (the deputy's ruling of 2026-09-08 on #502). A derived
 * room has no context and a room with nothing in front of the reader has
 * nothing to point at, so a caller with neither omits the argument. Drawn in
 * the note's own transaction — a note is never briefly in no context.
 *
 * Rejects with `invalid` for a link whose target is not an entity id, and for a
 * blank relation. An id no row carries is not an error: no link is drawn and
 * the note is written, the same answer a `[[ref]]` naming nothing gets.
 */
export function createNote(
  title?: string,
  bodyMd?: string,
  links?: NoteLinkInput[],
): Promise<NoteDetail> {
  return invoke<NoteDetail>("create_note", { title, bodyMd, links });
}

/**
 * Save a note — `knobas_app::commands::entity::save_note`.
 *
 * What the editor's autosave calls. The body is the source of truth for the
 * note's `[[ref]]` links: this reconciles them, so adding a ref draws the link
 * and removing it withdraws the link, and the answer carries the refs the
 * editor redraws its chips from.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for a note that no longer exists — an editor open on a deleted
 * note does not resurrect it.
 */
export function saveNote(noteId: string, title: string, bodyMd: string): Promise<NoteDetail> {
  return invoke<NoteDetail>("save_note", { noteId, title, bodyMd });
}

/**
 * Delete a note — `knobas_app::commands::entity::delete_note`.
 *
 * The body goes; the address stays, tombstoned, so a link somebody drew *to*
 * the note stays visible and marked instead of dangling. Resolves `false` when
 * there was nothing left to delete.
 *
 * Rejects with `invalid` for something that is not an entity id.
 */
export function deleteNote(noteId: string): Promise<boolean> {
  return invoke<boolean>("delete_note", { noteId });
}

/**
 * The proposals a room holds, newest first — `room_suggestions`.
 *
 * `sources` is the room's membership, the same convention `EntityFilter.sources`
 * uses: `[]` means every source, and an enumerated list would hide anything
 * synced by a source with no configuration row. A proposal belongs to a room
 * when *either* of its ends does.
 *
 * `ctx` is a stored context's room (#47): proposals are scoped to its
 * membership — the fixed one-hop rule, resolved server-side — plus the
 * context's own entity, so a proposed *add to this context* surfaces in the
 * room it would add to. `null` leaves the derived rooms' source scoping as
 * the whole rule.
 *
 * The tray holds no state: this call is the whole of it.
 */
export function roomSuggestions(
  sources: string[],
  ctx: string | null,
  limit: number,
): Promise<SuggestionPage> {
  return invoke<SuggestionPage>("room_suggestions", { sources, ctx, limit });
}

/**
 * Accept a suggestion — `accept_suggestion`. It becomes an ordinary link and
 * appears in both ends' links panels.
 *
 * Idempotent: accepting an already-accepted or already-dismissed suggestion
 * resolves, writes no activity line and emits nothing. Rejects with `invalid`
 * for something that is not a link id and `not_found` for one nothing carries.
 * Emits `EVENTS.activityNew`.
 */
export function acceptSuggestion(linkId: string): Promise<void> {
  return invoke<void>("accept_suggestion", { linkId });
}

/**
 * Dismiss a suggestion — `dismiss_suggestion`. It is remembered, so the same
 * suggestion is never proposed again, and neither is the link it would have
 * become.
 *
 * The same tombstone `unlink` leaves, deliberately: a dismissed suggestion and
 * an unlinked link are one fact to the detector.
 *
 * Idempotent, and refuses nothing when the id names a link that has already
 * been accepted — that is a link, and `unlink` is what withdraws it. Emits
 * `EVENTS.activityNew`.
 */
export function dismissSuggestion(linkId: string): Promise<void> {
  return invoke<void>("dismiss_suggestion", { linkId });
}

/**
 * Why one proposal was made, beyond its one-line reason —
 * `proposal_evidence` (#836): the text it was found in with the match marked,
 * what supports it and what speaks against it. A read beside
 * `roomSuggestions`; it changes no ranking. Rejects with `invalid` for
 * something that is not a link id and `not_found` for one nothing carries.
 */
export function proposalEvidence(linkId: string): Promise<ProposalEvidence> {
  return invoke<ProposalEvidence>("proposal_evidence", { linkId });
}

/**
 * Ask a source to change something — `knobas_app::commands::entity::submit_write`.
 *
 * The one way the UI starts a write-back (issue #43). `payload` is a
 * serialized `knobas_source::WriteOp`, the same {@link WriteOpPayload} shape
 * {@link pendingWrites} hands back and {@link amendWrite} takes — untyped on
 * the wire because `WriteOp` grows per milestone (ADR-0006) and a typed
 * argument would make every growth an IPC change.
 *
 * **The write is queued, not sent**, and the row that comes back is the write
 * *as queued*, before the attempt: the queue is what decides send, pend or
 * hold, and there is no path around it. Read the outcome back with
 * {@link pendingWrites}, or re-read on `EVENTS.activityNew` — every queue
 * transition writes an activity line.
 *
 * **There is no source argument.** The target's namespace *is* the source id
 * (interfaces §4.1), so a second argument could only agree or contradict.
 *
 * Rejects with `invalid` for a payload that is not a write op, a target that
 * is not an entity id, or an op the source does not offer — which the action
 * bar should never have rendered, since it is drawn from that source's
 * `write_ops` — `not_found` when the target names a source that is not
 * configured, and `conflict` for a `CreateMonitor` whose name has a create on
 * its way to that source, pending or sent and not mirrored yet (#1959).
 *
 * `linkTo` (#984, a ticket's entity id) links the branch a `CreateBranch`
 * makes, the pull request a `CreatePullRequest` opens or the page a
 * `CreatePage` makes to that ticket, and the ticket a `CreateTicket` files to
 * that entity (#1451, the failed run it was filed from), once the write has
 * been sent and a sync has brought it in; it rejects with `invalid` on any
 * other op and `not_found` for an entity the mirror does not hold.
 */
export function submitWrite(payload: WriteOpPayload, linkTo?: string): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("submit_write", linkTo === undefined ? { payload } : { payload, linkTo });
}

/**
 * The tickets filed from `entityId` that it does not link yet — `owed_tickets`
 * (#1953): each `CreateTicket` queued with it as its `linkTo` (a failed run's
 * *Create ticket*, #1451) and not withdrawn, oldest first, held and refused
 * ones included with their state. The read resolves first, so a ticket the
 * mirror now holds is linked and not named.
 *
 * What a page reads to know a create is on its way after a reload, when its
 * session's marks are gone.
 */
export function owedTickets(entityId: string): Promise<QueuedWrite[]> {
  return invoke<QueuedWrite[]>("owed_tickets", { entityId });
}

/**
 * What this ticket's workflow offers from where it stands **right now** —
 * `knobas_app::commands::entity::reachable_transitions` (#498).
 *
 * The statuses in the source's own spelling, straight from the source: nothing
 * is stored, so this is an answer about now and is good only for as long as the
 * detail is open. The status select offers these and only these, and the write
 * is unchanged — {@link submitWrite}'s `Transition` still names the status and
 * the adapter still resolves and refuses it at write time.
 *
 * **Only ask it of a ticket whose source declares `transition`.** A source with
 * no workflow — Gitea, TeamCity, Confluence, Uptime Kuma, the mock — rejects
 * this by name rather than answering an empty list, because the two mean
 * different things: an empty answer is a real workflow with nowhere left to go.
 * A rejection of any kind is the caller's cue to fall back to the
 * corpus-observed offer ({@link MiniBoard.sources}) and say the offer is
 * unverified, never to show an empty select.
 *
 * **There is no source argument**, for {@link submitWrite}'s reason: the
 * entity id already names the source.
 *
 * Rejects with `invalid` for something that is not an entity id, `not_found`
 * when its namespace is not a configured source, and `unauthorized` /
 * `unreachable` / `internal` as the source's own failure maps.
 */
export function reachableTransitions(entityId: string): Promise<string[]> {
  return invoke<string[]>("reachable_transitions", { entityId });
}

/**
 * The source view of one entity — `knobas_app::commands::entity::entity_view`
 * (#661, ADR-0020).
 *
 * Read live from the entity's own source and never stored. `section` omitted
 * reads the whole view, deferred sections as stubs; a section id reads that
 * one section's blocks, and the answer holds exactly that section.
 *
 * Three answers, each drawn differently: a view (tabs, the generic projection
 * behind *Raw*); `null`, a source with nothing to draw for this kind (the panel
 * as it always was); and a rejection, which draws the same panel under one line
 * naming it. Rejects with `invalid`, `not_found`, `unauthorized`,
 * `unreachable` or `internal` as {@link reachableTransitions} does.
 */
export function entityView(entityId: string, section: string | null = null): Promise<SourceView | null> {
  return invoke<SourceView | null>("entity_view", { entityId, section });
}

/** What `entity_file_bytes` answers: an `ArrayBuffer` from the bridge, a `Uint8Array` from the dev fake. */
type RawBytes = ArrayBuffer | ArrayLike<number>;

/**
 * The bytes of the file at `path` in `entityId`'s tree — `knobas_app::commands::entity::entity_file_bytes`
 * (#2256, spec #2234 step 5): a README's relative image, a PDF.
 *
 * `commit` is the `commit` of the file block the reader has open, handed back unread, so a README's image is read
 * at the README's commit; `null` reads at the entity's own reference. Answered raw, whole and never cut; kept for
 * the session where the read is at a whole sha, so a second look costs the source nothing.
 *
 * Rejects with `invalid` for a file over 50 MiB (the message names its size and the cap) or a source that reads
 * no bytes, and otherwise as {@link entityView} does.
 */
export async function entityFileBytes(entityId: string, path: string, commit: string | null): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("entity_file_bytes", { entityId, path, commit });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/**
 * A file pipeline over the bytes of the file at `path` in `entityId`'s tree — `knobas_app::commands::entity::entity_file_piped`
 * (#2264, spec #2234 step 5): `steps` in order, each on the last one's output. Read at `commit` as
 * {@link entityFileBytes} reads; a run over a file at a whole sha is kept, so the same pipeline again costs nothing.
 *
 * A step that fails, passes the 200 MiB cap or its timeout is an answer (`outcome: "failed"`), naming the step, its
 * error and the head of its input. Rejects as {@link entityFileBytes} does.
 */
export function entityFilePiped(entityId: string, path: string, commit: string | null, steps: PipeStep[]): Promise<PipeRun> {
  return invoke<PipeRun>("entity_file_piped", { entityId, path, commit, steps });
}

/**
 * The writes among `asks` their sources' credentials cannot make, each with
 * its reason — `knobas_app::commands::entity::refused_writes` (#2040,
 * ADR-0020 rule 4).
 *
 * For a surface that draws a write from what the adapter declares (the
 * inbox's *Re-run*, the Builds frame's *Run…*): it asks after drawing, and
 * draws each refused write disabled with the reason. Read live from each
 * entity's own source, never stored, and nothing is sent to find out. An ask
 * the answer leaves out is no verdict and keeps its drawing. Rejects only
 * with `not_ready` before bring-up.
 */
export function refusedWrites(asks: WriteAsk[]): Promise<RefusedWrite[]> {
  if (asks.length === 0) return Promise.resolve([]);
  return invoke<RefusedWrite[]>("refused_writes", { asks });
}

/**
 * The flow for a ticket, proposing one if there is none and `repoId` says where
 * it would go — `knobas_app::commands::entity::start_work_flow`.
 *
 * An empty array means there is no flow and no repository was named, which is
 * the state where the view asks for one. **Proposing dispatches nothing**: the
 * sequence is composed and stored so it can be shown before anything happens.
 *
 * Rejects with `invalid` for an id that is not an entity id, and `not_found` if
 * the ticket is not in the mirror.
 */
export function startWorkFlow(entityId: string, repoId: string | null): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_flow", { entityId, repoId });
}

/**
 * Run the flow as far as it will go, and answer where it stopped —
 * `knobas_app::commands::entity::start_work_run`.
 *
 * A step that fails stops the sequence; a step whose source cannot take the
 * write leaves it queued and waiting. Neither is a rejection here — both are
 * outcomes in the rows that come back, which is what the stepper draws. Emits
 * `EVENTS.activityNew` for every queue transition the run causes.
 */
export function startWorkRun(entityId: string): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_run", { entityId });
}

/**
 * Retry one step, without redoing the ones that succeeded —
 * `knobas_app::commands::entity::start_work_retry`.
 *
 * A step whose write is still open is retried by reading that write, not by
 * queueing a second one; otherwise knobas looks for the effect at the source
 * before sending anything again.
 *
 * Rejects with `conflict` if an earlier step is where the flow stopped —
 * running a later step over a failed one is what the sequence exists to
 * prevent.
 */
export function startWorkRetry(stepId: number): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_retry", { stepId });
}

/**
 * Skip one step — `knobas_app::commands::entity::start_work_skip`.
 *
 * A skipped step is settled, so the sequence carries on past it: a ticket that
 * needs no branch still gets its status moved.
 */
export function startWorkSkip(stepId: number): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_skip", { stepId });
}

/**
 * Replace a step's proposal with the one the reader edited —
 * `knobas_app::commands::entity::start_work_amend`.
 *
 * `payload` is the step's own stored value, edited. Rejects with `conflict` if
 * the step has already happened: editing it then would change nothing at the
 * source and everything on screen.
 */
export function startWorkAmend(stepId: number, payload: unknown): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_amend", { stepId, payload });
}

/**
 * The reverse direction: move every ticket whose linked pull request has been
 * merged — `knobas_app::commands::entity::follow_merges`.
 *
 * Answers how many moved, so an automatic change can be announced rather than
 * happening silently. It acts **only on links knobas holds**: a merged pull
 * request nobody connected to a ticket does nothing at all.
 */
export function followMerges(): Promise<number> {
  return invoke<number>("follow_merges");
}

/**
 * The inbox — `inbox_items`. One actionable stream, newest first.
 *
 * Both shelves are the same derivation with the same predicate: ask for
 * `"snoozed"` to see what is deferred and when it returns.
 *
 * Rejects with `not_ready` while the database or the sync engine is still
 * coming up.
 */
export function inboxItems(shelf: InboxShelf): Promise<InboxEntry[]> {
  return invoke<InboxEntry[]>("inbox_items", { shelf });
}

/**
 * How many items need you now — `inbox_count`. The number the top strip shows.
 *
 * **Snoozed items are not in it**, because the number means "needs me now";
 * neither are items marked done. It is the stream's own statement counted, so
 * the badge cannot disagree with the view it opens — do not recompute it from
 * a filtered {@link inboxItems} result.
 */
export function inboxCount(): Promise<number> {
  return invoke<number>("inbox_count");
}

/**
 * Not now — `snooze_inbox_item`. The item leaves the stream and returns on
 * `until`, without anything having to be scheduled.
 *
 * `until` is an absolute RFC 3339 moment: the presets (*tomorrow*, *next
 * Monday*, *after the credential expires*) are this side's arithmetic.
 * Snoozing something already snoozed moves its date.
 *
 * Rejects with `not_found` for an item on neither shelf — which is what a
 * stale list in this webview looks like. Writes one activity line and emits
 * `EVENTS.activityNew`.
 */
export function snoozeInboxItem(itemKey: string, until: string): Promise<void> {
  return invoke<void>("snooze_inbox_item", { itemKey, until });
}

/**
 * I handled this — `complete_inbox_item`.
 *
 * The item leaves, and **comes back if its subject moves again**: done is
 * recorded as the moment it was answered, so marking a mention done hides it
 * until somebody says something new rather than muting the ticket for ever.
 *
 * Rejects like {@link snoozeInboxItem}. Writes one activity line and emits
 * `EVENTS.activityNew`.
 */
export function completeInboxItem(itemKey: string): Promise<void> {
  return invoke<void>("complete_inbox_item", { itemKey });
}

/**
 * The switcher's list — `list_contexts`. Every unarchived context, newest
 * first.
 */
export function listContexts(): Promise<ContextRow[]> {
  return invoke<ContextRow[]>("list_contexts");
}

/**
 * Promote an entity to a context of its own — `promote_context` (spec §7:
 * "any ticket can be promoted").
 *
 * Idempotent: promoting twice answers with the one context. Rejects with
 * `not_found` for an entity that never synced, and with `invalid` for a
 * context — a context about a context would union two working sets
 * (ADR-0008). Writes one activity line and emits `EVENTS.contextsChanged` —
 * on the first promotion only, because the second mutated nothing.
 */
export function promoteContext(entityId: string): Promise<ContextRow> {
  return invoke<ContextRow>("promote_context", { entityId });
}

/**
 * What a ticket reaches — `job_reach` (#817, R-2.8): its context's walk,
 * down only from a VM, proxy or host, plus its parent's own links behind
 * {@link parentLinks}. An id nothing answers to reaches nothing.
 */
export function jobReach(entityId: string): Promise<Reached[]> {
  return invoke<Reached[]>("job_reach", { entityId });
}

/** Whether *Sub-tasks show their parent ticket's connections* is on (R-7.3). */
export function parentLinks(): Promise<boolean> {
  return invoke<boolean>("parent_links");
}

/** Switch R-7.3's switch; answers what is now stored. */
export function setParentLinks(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_parent_links", { enabled });
}

/** A board read in lanes (#939): by what, and how many lanes to send. */
export interface LanesAsked {
  by: LaneField;
  first: number;
}

/**
 * The ticket board — `mini_board` (ADR-0009, #177).
 *
 * `role` (#1299, ADR-0029 as amended) reads the board over the sources that
 * fill that role, as the mirror answers them: the Tickets page asks for the
 * ticket system's, and its columns and counts are then the fillers' alone. A
 * role nobody fills is an empty board; `null` reads every source. `context`
 * narrows to a stored context's fixed one-hop membership (§16.11, ADR-0008). Of
 * what the scope admits the board draws the live tickets, so a ticket the
 * source deleted is off it. Grouping and column order happen in the command,
 * not here.
 *
 * `perColumn` cuts each column to its newest cards (#753): what the tile asks
 * for, so a room of thousands of tickets is not thousands of cards. Omitted,
 * the board is whole.
 *
 * `query` (#810) is search grammar text — the Tickets view's base query and
 * its filter bar's words and chips. A non-blank one narrows the board to
 * every ticket the grammar admits; the filtering is the server's, never the
 * webview's (R-2.7).
 *
 * An unknown context is an empty board rather than a rejection — the tile has
 * to be able to say "nothing here" without saying "this broke".
 *
 * `savedView` (#877) names a saved view (its `saved_view`, #1245): the board is then the
 * tickets its source put on it, in the source's order, under the view's own columns —
 * each column's `status` is the column's name — and the room scope is not
 * read.
 *
 * `lanes` (#939, #1098) puts the board in lanes on the server: `perColumn`
 * then caps each lane's column, and only the first `lanes.first` lanes are
 * sent, biggest first (`MiniBoard.lanes`, `MiniBoard.more_lanes`).
 */
export function miniBoard(
  filter: Pick<EntityFilter, "role" | "context" | "project">,
  perColumn?: number | null,
  query?: string | null,
  savedView?: string | null,
  lanes?: LanesAsked | null,
): Promise<MiniBoard> {
  return invoke<MiniBoard>("mini_board", {
    ctxId: filter.context,
    role: filter.role ?? null,
    project: filter.project,
    perColumn: perColumn ?? null,
    query: query ?? null,
    savedView: savedView ?? null,
    lanes: lanes?.by ?? null,
    lanesFirst: lanes?.first ?? null,
  });
}

/**
 * Every project the live corpus shows — `list_projects`, ordered by source
 * then key.
 *
 * What the switcher builds a room per project from. It is a **census** and
 * deliberately not derived from a room's own read, which is a window over the
 * newest items: a quiet project would otherwise silently have no room. A
 * source the user turned off shows no projects, the same way its items leave
 * every other reader.
 */
export function listProjects(): Promise<Project[]> {
  return invoke<Project[]>("list_projects");
}

/**
 * Which inbox categories may raise a desktop notification —
 * `notification_kinds` (#290, spec #272 "Notifications").
 *
 * Empty for a profile nobody has switched one on in: every kind is off by
 * default, so a noisy Jira cannot make the feature unusable on day one.
 *
 * One `knobas.setting` row and no migration — the rule
 * `set_backup_schedule` and `set_passive_attribution` follow. It is a
 * *preference* and says nothing about the OS permission, which is asked for on
 * the click that switches the first kind on and is not knobas' to remember.
 */
export function notificationKinds(): Promise<InboxCategory[]> {
  return invoke<InboxCategory[]>("notification_kinds");
}

/**
 * Set which inbox categories may notify — `set_notification_kinds`. Answers
 * with what is now **stored**, deduplicated and in the backend's own order, so
 * the toggles draw the database rather than the click.
 *
 * Rejects with `invalid` for a word that is not a category — the write side is
 * strict where the read side is forgiving, because the only caller is a
 * surface sending back words it was given.
 */
export function setNotificationKinds(kinds: InboxCategory[]): Promise<InboxCategory[]> {
  return invoke<InboxCategory[]>("set_notification_kinds", { kinds });
}

/**
 * The standup digest for one day — `standup_digest` (#288, spec #272).
 *
 * **The webview computes the days**, each as a date and the two instants it
 * spans, because the machine's timezone is a fact only this side holds — the
 * rule `lib/time/day.ts` records in full. `today` is the day being asked about
 * and `earlier` the days before it, oldest first; `lib/standup/standup.ts`'s
 * `digestWindows` is what builds both.
 *
 * How far back *yesterday* may reach is **not** this side's to decide: the
 * backend consults at most its own seven, however many windows are sent.
 */
export function standupDigest(
  today: DayWindow,
  earlier: DayWindow[],
): Promise<StandupDigest> {
  return invoke<StandupDigest>("standup_digest", { today, earlier });
}

/**
 * The protocol for a date, made from the template if it is not there yet —
 * `standup_protocol` (#289, story 64).
 *
 * **Get-or-create**: opening a date twice lands in the same note. It is also
 * where a publication that settled while nobody was looking catches up — the
 * page id comes off the write queue row, which outlives the process that first
 * received it.
 */
export function standupProtocol(day: string): Promise<Protocol> {
  return invoke<Protocol>("standup_protocol", { day });
}

/**
 * Publish a date's protocol to Confluence — `publish_standup_protocol`
 * (stories 65-68).
 *
 * `target` is the first publish's dialog answer; omit it to use the stored
 * one, and a missing stored one is a refusal rather than a guess. **A second
 * publish for one date queues nothing** and answers with the publication
 * already on the queue.
 */
export function publishStandupProtocol(
  day: string,
  target?: PublishTarget,
): Promise<Protocol> {
  // Answers once the page's write is queued (#779): the publication is that
  // write, with no page yet; `standupProtocol` names it once it lands.
  return invoke<Protocol>("publish_standup_protocol", { day, target: target ?? null });
}

/** Where protocols are published, or `null` until somebody has said. */
export function standupPublishTarget(): Promise<PublishTarget | null> {
  return invoke<PublishTarget | null>("standup_publish_target");
}

/** Change where protocols are published — settings' half of story 67. */
export function setStandupPublishTarget(target: PublishTarget): Promise<PublishTarget> {
  return invoke<PublishTarget>("set_standup_publish_target", { target });
}

/**
 * File a ticket from an action item and link it to the protocol —
 * `create_action_item_ticket` (story 69).
 *
 * Answers once the create is queued (#779): `ticket_entity_id` is `null`, and
 * {@link actionItemTicket} names the ticket once it is filed and mirrored.
 *
 * The **existing** create-ticket op, carried by the write queue like any other
 * write; `project` is the project's entity id, which is what {@link
 * listProjects} hands the picker.
 */
export function createActionItemTicket(
  noteId: string,
  project: string,
  ticketType: string,
  title: string,
  body: string,
): Promise<ActionItemTicket> {
  return invoke<ActionItemTicket>("create_action_item_ticket", {
    noteId,
    project,
    ticketType,
    title,
    body,
  });
}

/**
 * Where an action item's ticket stands — `action_item_ticket` (#779): its
 * write, and the ticket, linked to the protocol, once it is filed and
 * mirrored. Asked as the queue moves, to fill in the address of a ticket
 * drawn as on its way.
 */
export function actionItemTicket(noteId: string, writeId: number): Promise<ActionItemTicket> {
  return invoke<ActionItemTicket>("action_item_ticket", { noteId, writeId });
}

/**
 * Show a desktop notification whose click comes back as
 * `EVENTS.notificationClicked` — `notify` (#339).
 *
 * knobas's own Rust sends it through `notify-rust` and waits on the handle,
 * because `@tauri-apps/plugin-notification`'s desktop `notify` drops that
 * handle and its `onAction` exists on mobile only. Resolves once the OS has
 * the notification, not when it is clicked. Rejects with `internal` and the
 * platform's sentence when the OS refused — on macOS a bare `tauri dev`
 * binary is refused by `UNUserNotificationCenter` (no bundle identifier).
 */
export function notify(draft: NotificationDraft): Promise<void> {
  return invoke<void>("notify", { draft });
}

/**
 * Set the clones root, or clear it — `set_clones_root` (#499).
 *
 * A blank path clears it: an empty root would make the scan walk whatever the
 * process's working directory happens to be.
 */
export function setClonesRoot(path: string | null): Promise<void> {
  return invoke<void>("set_clones_root", { path });
}

/**
 * The checkout for a repo or a branch — `entity_checkout` (#499).
 *
 * Read on every open rather than stored, so a clone that moved is a *no
 * checkout* and not a path pointing at nothing. Rejects with `invalid` for an
 * entity that is neither a repo nor a branch.
 */
export function entityCheckout(entityId: string): Promise<CheckoutView> {
  return invoke<CheckoutView>("entity_checkout", { entityId });
}

/**
 * Set this repository's checkout path by hand, or clear it back to the scan —
 * `set_checkout_override` (#499).
 *
 * Answers the fresh view, so clearing an override shows the scan taking over.
 * Keyed on the repository even when a branch was the address.
 */
export function setCheckoutOverride(
  entityId: string,
  path: string | null,
): Promise<CheckoutView> {
  return invoke<CheckoutView>("set_checkout_override", { entityId, path });
}

/** The applications, the default, the clones root and the git — `checkout_settings` (#844). */
export function checkoutSettings(): Promise<CheckoutSettings> {
  return invoke<CheckoutSettings>("checkout_settings");
}

/**
 * Save the applications in order, and which is the default —
 * `set_checkout_applications` (#844). Answers the fresh settings. Rejects
 * with `invalid` for an entry with no name or a command knobas could not run.
 */
export function setCheckoutApplications(
  applications: CheckoutApplicationDraft[],
  defaultApplication: string | null,
): Promise<CheckoutSettings> {
  return invoke<CheckoutSettings>("set_checkout_applications", { applications, defaultApplication });
}

/** What {@link openCheckout} runs, and where. */
export interface OpenAt {
  /** An application's id; the default when absent. */
  application?: string | null;
  /** A file inside the checkout, relative to it: the file command runs. */
  file?: string | null;
  /** Fills `{line}`. */
  line?: number | null;
}

/**
 * Run an application at this entity's checkout — `open_checkout` (#501,
 * #844). Answers the argument vector it started.
 *
 * The one call that starts a program a person chose, and the program is
 * always a saved application: *Try it* saves the draft first and opens it by
 * id (#1643). Every value filled into it is from this disk: the checkout path
 * and a file inside it, never a field of the mirrored repo (ADR-0016).
 * Resolves once the program has started.
 */
export function openCheckout(entityId: string, at: OpenAt = {}): Promise<string[]> {
  return invoke<string[]>("open_checkout", {
    entityId,
    application: at.application ?? null,
    file: at.file ?? null,
    line: at.line ?? null,
  });
}

/**
 * Run an application on a code object of this entity's repository —
 * `open_object` (#2274): a repository, a file at a line, a commit, a range or
 * a pull request. The application's command for the object's kind runs at
 * the checkout; one without such a command is refused. Answers the argument
 * vector it started.
 */
export function openObject(entityId: string, application: string | null, object: OpenObject): Promise<string[]> {
  return invoke<string[]>("open_object", { entityId, application, object });
}

/**
 * The page a code object has at its source — `object_page` (#2274): what
 * *Open in › Browser* opens. Built from the source's typed base URL and the
 * pages its adapter declares; `null` when it declares none.
 */
export function objectPage(entityId: string, object: OpenObject): Promise<string | null> {
  return invoke<string | null>("object_page", { entityId, object });
}

/**
 * What *Open in › Diff tool* would run for `base`..`head` — `diff_tool`
 * (#2275): `git difftool --dir-diff --gui --no-prompt` at the checkout, `git`
 * first. `null` when it is not offered: no checkout, or an end that is not a
 * sha the checkout holds.
 */
export function diffTool(entityId: string, base: string, head: string): Promise<string[] | null> {
  return invoke<string[] | null>("diff_tool", { entityId, base, head });
}

/** Start the Diff tool on `base`..`head` — `open_diff_tool` (#2275). Answers the argv it started. */
export function openDiffTool(entityId: string, base: string, head: string): Promise<string[]> {
  return invoke<string[]>("open_diff_tool", { entityId, base, head });
}

/** The pull request worktrees knobas made under the clones root — `pull_request_worktrees` (#2285). */
export function pullRequestWorktrees(): Promise<PullRequestWorktree[]> {
  return invoke<PullRequestWorktree[]>("pull_request_worktrees");
}

/**
 * Remove one pull request worktree knobas made — `remove_pull_request_worktree`
 * (#2285). git refuses one with changes or untracked files (`conflict`).
 * Answers the list as it then stands.
 */
export function removePullRequestWorktree(path: string): Promise<PullRequestWorktree[]> {
  return invoke<PullRequestWorktree[]>("remove_pull_request_worktree", { path });
}

/**
 * Clone a repository into `<clones root>/<name>`, or stop the clone that is
 * running — `clone_repo` (#823, ADR-0023).
 *
 * Starting resolves at once with `cloning` (progress follows on
 * `EVENTS.cloneProgress`) or `adopted` (the folder is already a clone of it).
 * Stopping resolves once the half-made folder is removed. A folder that
 * exists and is not a clone of this repository, or that another repository
 * of the same name is cloning into, rejects with `conflict`.
 */
export function cloneRepo(entityId: string, stop?: boolean): Promise<CloneAnswer> {
  return invoke<CloneAnswer>("clone_repo", { entityId, stop: stop ?? null });
}

/**
 * The uncommitted changes at a repo's or a branch's checkout —
 * `entity_checkout_diff` (#824, ADR-0026). Read when the *Local changes* tab
 * opens. Rejects with `not_found` for an entity with no checkout on this
 * machine and `invalid` for one that is neither a repo nor a branch.
 */
export function entityCheckoutDiff(entityId: string): Promise<DiffRead> {
  return invoke<DiffRead>("entity_checkout_diff", { entityId });
}

/**
 * The diff from `base` to `head` of a repository — `entity_range_diff`
 * (#2272, ADR-0036): local read-only git when its checkout holds both
 * commits (whitespace ignored when asked), else the source's compare read.
 * `read_at` says which; only `checkout` ignores whitespace.
 */
export function entityRangeDiff(entityId: string, base: string, head: string, ignoreWhitespace: boolean): Promise<RangeDiff> {
  return invoke<RangeDiff>("entity_range_diff", { entityId, base, head, ignoreWhitespace });
}

/** What the capture shortcut is set to, and whether it holds — `capture_shortcut` (#503). */
export function captureShortcut(): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("capture_shortcut");
}

/**
 * Set the capture shortcut, or clear it with a blank one —
 * `set_capture_shortcut` (#503).
 *
 * Answers the fresh view, so the field draws what is now stored. A combination
 * the plugin or the operating system refuses does **not** reject: it is stored,
 * and the answer carries the refusal. See {@link CaptureShortcut}.
 */
export function setCaptureShortcut(accelerator: string | null): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("set_capture_shortcut", { accelerator });
}

/**
 * Record where the reader is standing — `record_capture_context` (#503).
 *
 * The **main** window's to call, whenever either half changes. The capture
 * window is a webview with no shell in it and cannot work either out; this is
 * how it is told.
 */
export function recordCaptureContext(
  context: string | null,
  foreground: string | null,
): Promise<void> {
  return invoke<void>("record_capture_context", { context, foreground });
}

/** What the main window last recorded — `capture_context` (#503). */
export function captureContext(): Promise<CaptureContext> {
  return invoke<CaptureContext>("capture_context");
}

/**
 * Bring the main window forward and open one note in it — `reveal_note` (#503).
 *
 * The capture window's *Open in knobas* button. It does not close the capture
 * window; the capture window closes itself, so that a main window which would
 * not come forward leaves the reader holding the words they typed.
 */
export function revealNote(noteId: string): Promise<void> {
  return invoke<void>("reveal_note", { noteId });
}

/**
 * Record that the reader opened this ticket's job, and answer when they last
 * did before now — `visit_entity` (#819, R-5.5). `null` on a first visit.
 * Rejects with `invalid` for a non-id and `not_found` for an unknown entity.
 */
export function visitEntity(entityId: string): Promise<string | null> {
  return invoke<string | null>("visit_entity", { entityId });
}

/**
 * A ticket's thread — `entity_thread` (#819, R-5.2): its comments, each marked
 * when it is the reader's or mentions them, and the status moves knobas sent.
 * Rejects with `invalid` for a non-id and `not_found` for one the mirror does
 * not hold.
 */
export function entityThread(entityId: string): Promise<Thread> {
  return invoke<Thread>("entity_thread", { entityId });
}

/**
 * Turn *Show notes and quick capture* on or off — `set_notes_shown` (#832,
 * D10). Registers or releases the stored capture shortcut to match, and
 * answers the capture shortcut's view, which carries the setting.
 */
export function setNotesShown(shown: boolean): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("set_notes_shown", { shown });
}

/**
 * *Export notes as Markdown* — `export_notes_markdown` (#832, D10): one `.md`
 * per note, `[[refs]]` kept as text, in `~/Downloads/knobas-notes`.
 */
export function exportNotesMarkdown(): Promise<NotesExport> {
  return invoke<NotesExport>("export_notes_markdown");
}

/**
 * *Move to Confluence…* — `move_notes_to_confluence` (#941, D10): one
 * `create_page` per note under `parent` (a mirrored page), through the write
 * queue, answering what was queued. The notes stay. Rejects with `invalid`
 * when there are no notes and `not_found` for a parent the mirror does not hold.
 */
export function moveNotesToConfluence(parent: string): Promise<QueuedWrite[]> {
  return invoke<QueuedWrite[]>("move_notes_to_confluence", { parent });
}

/**
 * The Docs reader's left pane — `docs_tree` (#859). The read narrows itself
 * to the wiki role's fillers (#1296, §10.8's #1294 and #1296 entries): a
 * space, the page read last and *For you* are only theirs.
 */
export function docsTree(): Promise<DocsTree> {
  return invoke<DocsTree>("docs_tree");
}

/**
 * One page drawn as blocks — `docs_page` (#859). Rejects with `invalid` for a
 * non-id and `not_found` for an id that is not a live page.
 */
export function docsPage(entityId: string): Promise<DocPage> {
  return invoke<DocPage>("docs_page", { entityId });
}

/**
 * The Docs search — `docs_search` (#860): the pages `query` admits in `space`
 * (the picker's; a typed `space:` wins), or `null` for a query with neither
 * words nor a filter. Only the wiki role's fillers' pages, as the tree
 * (#1296, §10.8's #1296 entry).
 */
export function docsSearch(
  query: string,
  space: { source_id: string; key: string } | null,
): Promise<DocsHits | null> {
  return invoke<DocsHits | null>("docs_search", { query, space });
}

/** *Export as Markdown* — `docs_export_markdown` (#861): the page as `.md` in Downloads. */
export function docsExportMarkdown(entityId: string): Promise<DocExport> {
  return invoke<DocExport>("docs_export_markdown", { entityId });
}

/**
 * *Edit section* — `docs_edit_section` (#862): queue section `section`'s new
 * text against the version read. Rejects with `invalid` for a section knobas
 * does not edit or a page that moved on.
 */
export function docsEditSection(
  entityId: string,
  section: number,
  version: number | null,
  text: string,
): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("docs_edit_section", { entityId, section, version, text });
}

/** *New page* — `docs_create_page` (#862), linked to `linkTo` (a ticket id) once it lands. */
export function docsCreatePage(page: NewPage, linkTo: string | null): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("docs_create_page", { page, linkTo });
}

/** Every link set, by name (R-7.6). */
export function linkSets(): Promise<LinkSetRow[]> {
  return invoke<LinkSetRow[]>("link_sets");
}

/** One link set and the tickets it shows on; `not_found` for an id nobody holds. */
export function linkSet(id: string): Promise<LinkSetDetail> {
  return invoke<LinkSetDetail>("link_set", { id });
}

/** Save a whole set, members and rules replaced in order; answers it as stored. */
export function saveLinkSet(draft: LinkSetDraft): Promise<LinkSetDetail> {
  return invoke<LinkSetDetail>("save_link_set", { draft });
}

/** Delete a set; links drawn on tickets by hand stay. */
export function deleteLinkSet(id: string): Promise<void> {
  return invoke<void>("delete_link_set", { id });
}

/** The live tickets one rule applies to now, yours marked (R-7.7's live counts). */
export function previewLinkSetRule(rule: LinkSetRule): Promise<RuleTicket[]> {
  return invoke<RuleTicket[]>("preview_link_set_rule", { rule });
}

/** What a pattern would match now, the projects to pick from, and why it is not a pattern (#868, R-7.7). */
export function previewLinkSetPattern(pattern: LinkSetPattern): Promise<LinkSetPatternPreview> {
  return invoke<LinkSetPatternPreview>("preview_link_set_pattern", { pattern });
}

/** *Hide here* (`hidden`) or *Show again* one set member on one ticket (R-7.5); answers the new state. */
export function hideLinkSetMember(ticketId: string, memberId: string, hidden: boolean): Promise<boolean> {
  return invoke<boolean>("hide_link_set_member", { ticketId, memberId, hidden });
}

/** The hints, most tickets first (#870). */
export function linkSetHints(): Promise<LinkSetHint[]> {
  return invoke<LinkSetHint[]>("link_set_hints");
}

/** The link sets and the parent a ticket's job shows; an unknown id has neither. */
export function jobLinkSets(entityId: string): Promise<JobLinkSets> {
  return invoke<JobLinkSets>("job_link_sets", { entityId });
}
