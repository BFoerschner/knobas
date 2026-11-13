/**
 * Every `{#each}` over something whose length comes from source data draws only what is near the viewport, or is cut
 * where it is read, or is small for a reason written here (the extremes pass of PR #2479, 2026-11-13). A list drawn
 * whole is how a folder of 3,000 files took a second to open on Björn's PC, and how a pull request of 3,000 changed
 * files held the main thread 17 s at 4x: every check before measured ordinary shapes and pressed only what it was
 * scripted to press. So the shape is refused here instead of found by a profiler.
 *
 * Read off disk with Svelte's own parser, over `tf/` and `shell/`. An `{#each}` passes when its list is:
 * - windowed: `<something>win.slice(...)` (`shell/windowed.svelte.ts`);
 * - cut by a constant: `....slice(0, NAME)`;
 * - a constant or a literal: `NAME`, `[...]`, `{ length: ... }`;
 * - or named in `SMALL` below, with why it stays small. A new list from source data is windowed, or added here with
 *   its reason, which a review reads.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

import { parse } from "svelte/compiler";
import { expect, test } from "vitest";

/** `app/src/lib`, off `process.cwd()` (vitest's root is `app/`). */
const LIB = join(process.cwd(), "src", "lib");

function walk(dir: string, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, found);
    else if (entry.name.endsWith(".svelte")) found.push(path);
  }
  return found.sort();
}

/** The shapes that need no reason. */
const SAFE: RegExp[] = [/^\w*[wW]in\.slice\(/, /\.slice\(0, ?[A-Z_][A-Z0-9_]*\)$/, /^[A-Z][A-Z0-9_]*$/, /^\[/, /^\{ ?length:/];

type Node = { type?: string; expression?: { start: number; end: number } } & Record<string, unknown>;

/** `path  expression` for each `{#each}` in `source` whose list is not one of the safe shapes. */
function eaches(path: string, source: string): string[] {
  const found: string[] = [];
  const visit = (node: unknown) => {
    if (node === null || typeof node !== "object") return;
    if (Array.isArray(node)) return node.forEach(visit);
    const n = node as Node;
    if (n.type === "EachBlock" && n.expression) {
      const list = source.slice(n.expression.start, n.expression.end).replace(/\s+/g, " ");
      if (!SAFE.some((shape) => shape.test(list))) found.push(`${path}  ${list}`);
    }
    for (const [key, value] of Object.entries(n)) if (key !== "parent" && key !== "metadata") visit(value);
  };
  visit((parse(source, { modern: true }) as unknown as { fragment: unknown }).fragment);
  return found;
}

/** Every `{#each}` under `tf/` and `shell/` that needs a reason, by file and list. */
function unsafe(): string[] {
  const files = [...walk(join(LIB, "tf")), ...walk(join(LIB, "shell"))];
  return [...new Set(files.flatMap((file) => eaches(relative(LIB, file), readFileSync(file, "utf8"))))];
}

/** The lists drawn whole, each with why it stays small. Keyed `path  list` as `eaches` writes them. */
const SMALL: Record<string, string> = {
  "shell/TimerPicker.svelte  assets": "capped where it is read or drawn: the launcher's search answers at most a page of hits",
  "shell/TimerPicker.svelte  candidates": "capped where it is read or drawn: the launcher's search answers at most a page of hits",
  "shell/WriteQueue.svelte  decisions": "what this session queued that the source does not hold yet: the writes waiting on the reader's decision: what they queued",
  "shell/WriteQueue.svelte  waiting": "what this session queued that the source does not hold yet: the reader's own writes on their way: what they queued",
  "shell/WriteQueue.svelte  writeWords(row, (id) => names[id] ?? null)": "the runs of one line's or one name's words",
  "tf/TfCreateTicket.svelte  projects": "a handful by what it is: the projects the sources sync: tens",
  "tf/TfCrumbs.svelte  parts": "a handful by what it is: a page's crumbs: a few levels",
  "tf/TfEntityAction.svelte  action.fields": "a handful by what it is: one action's fields, as the source declares them",
  "tf/TfEntityAction.svelte  field.kind.options": "a handful by what it is: one field's choices, as the source declares them",
  "tf/TfKeyed.svelte  keyParts(text)": "the runs of one line's or one name's words",
  "tf/TfPage.svelte  drawn": "a handful by what it is: the pages kept alive: a few, by the keep-alive's own limit",
  "tf/TfStatusBar.svelte  failed": "the profile's own settings, tens at most: the configured sources",
  "tf/TfStatusBar.svelte  healthy": "the profile's own settings, tens at most: the configured sources",
  "tf/TfToasts.svelte  toasts.items": "a handful by what it is: the toasts up at once",
  "tf/TfWorklogDraft.svelte  draft.candidates": "a handful by what it is: the tickets a draft's time could go to: the day's few",
  "tf/asset-tree/TfAssetTree.svelte  reached.list": "a handful by what it is: one asset's reach: what it runs on and what runs on it",
  "tf/builds/TfPipeline.svelte  day.runs": "inside one drawn row: one day's runs, inside the windowed days",
  "tf/builds/TfPipeline.svelte  field.kind.options": "a handful by what it is: one parameter's choices",
  "tf/builds/TfPipeline.svelte  history": "capped where it is read or drawn: `historyOf`: the last `HISTORY` runs",
  "tf/builds/TfPipeline.svelte  repositories": "a handful by what it is: the repositories one pipeline builds",
  "tf/builds/TfPipeline.svelte  settings.parameters": "a handful by what it is: one pipeline's parameters as its source declares them",
  "tf/builds/TfPipeline.svelte  settings.triggers": "a handful by what it is: one pipeline's triggers",
  "tf/builds/TfRun.svelte  artifacts.first": "capped where it is read or drawn: `artifactsOf`: the first `FIRST_ARTIFACTS`; the Artifacts tab is windowed",
  "tf/builds/TfRun.svelte  changes": "a handful by what it is: the commits one run built: the source's own change list for a build, tens",
  "tf/builds/TfRun.svelte  parameters": "a handful by what it is: one run's parameters",
  "tf/builds/TfRun.svelte  segments": "a handful by what it is: one run's stages",
  "tf/builds/TfRunChain.svelte  boxes": "a handful by what it is: one run's chain of builds",
  "tf/builds/TfRunDialog.svelte  extra": "a handful by what it is: the parameters the reader added by hand",
  "tf/builds/TfRunDialog.svelte  field.kind.options": "a handful by what it is: one parameter's choices",
  "tf/builds/TfRunDialog.svelte  fields": "a handful by what it is: one pipeline's parameters",
  "tf/builds/TfRunDialog.svelte  presets": "the profile's own settings, tens at most: the reader's saved presets for one pipeline",
  "tf/builds/TfRunLog.svelte  runs": "the runs of one line's or one name's words",
  "tf/builds/TfRunTicket.svelte  tickets": "a handful by what it is: the tickets one run's commits name",
  "tf/code/TfCode.svelte  groups": "a set the code fixes: Code's pull request groups",
  "tf/diff/TfDiffCard.svelte  comments": "inside one drawn row: one thread's comments, inside a windowed line",
  "tf/diff/TfDiffCard.svelte  expands(it.gap)": "a set the code fixes: a gap's two or three expand buttons",
  "tf/diff/TfDiffCard.svelte  marked(cell.line.text, toks, words)": "the runs of one line's or one name's words",
  "tf/diff/TfDiffCard.svelte  placed.apart": "a handful by what it is: one file's threads the diff no longer draws a line for",
  "tf/diff/TfDiffCard.svelte  strays": "what this session queued that the source does not hold yet: the reader's pending comments on lines this file no longer has",
  "tf/diff/TfDiffCard.svelte  talk.queuedOn(thread)": "what this session queued that the source does not hold yet",
  "tf/diff/TfDiffCard.svelte  toks": "the runs of one line's or one name's words: one line's highlighted tokens",
  "tf/diff/TfDiffRendered.svelte  sides": "a set the code fixes: a rendered diff's two sides",
  "tf/diff/TfDiffView.svelte  comments": "inside one drawn row: one thread's comments",
  "tf/diff/TfDiffView.svelte  gone": "a handful by what it is: threads on files the change no longer holds",
  "tf/diff/TfDiffView.svelte  homeless": "what this session queued that the source does not hold yet: the reader's pending comments on files the diff no longer holds",
  "tf/diff/TfDiffView.svelte  stale": "what this session queued that the source does not hold yet: the reader's pending comments written on an earlier head",
  "tf/diff/TfWrittenAt.svelte  thread.hunk.lines": "a handful by what it is: the few lines a thread quotes",
  "tf/docs/TfDocBlocks.svelte  block.rows[0] ?? []": "inside one drawn row: one table row's cells",
  "tf/docs/TfDocBlocks.svelte  bodyRows(index, block.items)": "capped where it is read or drawn: a long list's items drawn a piece a task (`TABLE_ROWS_FIRST`, then more)",
  "tf/docs/TfDocBlocks.svelte  bodyRows(index, head ? block.rows.slice(1) : block.rows)": "capped where it is read or drawn: a long table's rows drawn a piece a task (`TABLE_ROWS_FIRST`, then more)",
  "tf/docs/TfDocBlocks.svelte  row": "inside one drawn row: one table row's cells",
  "tf/docs/TfDocBlocks.svelte  shownBlocks": "capped where it is read or drawn: drawn a piece a task (`DOC_BLOCKS_FIRST`, then more) unless the body is asked for whole",
  "tf/docs/TfDocBody.svelte  shown": "capped where it is read or drawn: a body's sections drawn a budget a task (`FIRST`, then `MORE`)",
  "tf/docs/TfDocFoot.svelte  group.links": "a handful by what it is: one page's links of one kind",
  "tf/docs/TfDocFoot.svelte  groups": "a set the code fixes: the foot's link groups",
  "tf/docs/TfDocFoot.svelte  page.work": "a handful by what it is: the work items a page names",
  "tf/docs/TfDocHits.svelte  found.hits": "capped where it is read or drawn: Docs search answers at most a page of hits",
  "tf/docs/TfDocHits.svelte  hit.snippet.segments": "the runs of one line's or one name's words: one snippet's marked runs",
  "tf/docs/TfDocInlines.svelte  text": "the runs of one line's or one name's words: one paragraph's inline runs",
  "tf/docs/TfDocTree.svelte  queued": "what this session queued that the source does not hold yet",
  "tf/docs/TfDocs.svelte  forYou": "capped where it is read or drawn: *For you*: the newest few the read answers",
  "tf/docs/TfDocs.svelte  group.pages": "a handful by what it is: the pages one job names",
  "tf/docs/TfDocs.svelte  jobGroups": "a set the code fixes: the job's page groups",
  "tf/docs/TfDocs.svelte  p.labels": "on one ticket, page or pull request: one page's labels",
  "tf/docs/TfDocs.svelte  queuedComments": "what this session queued that the source does not hold yet",
  "tf/docs/TfDocs.svelte  tree.spaces": "a handful by what it is: the spaces the sources sync: tens",
  "tf/docs/TfDocsHome.svelte  forYouRows": "capped where it is read or drawn: *For you*: the newest few the read answers",
  "tf/docs/TfDocsHome.svelte  jobRows": "capped where it is read or drawn: `JOB_PAGES`",
  "tf/docs/TfDocsHome.svelte  rows": "capped where it is read or drawn: `tileRows`: a space tile's `TOP_PAGES`",
  "tf/docs/TfDocsHome.svelte  tree.spaces": "a handful by what it is: the spaces the sources sync: tens",
  "tf/docs/TfMarked.svelte  markText(text, stems)": "the runs of one line's or one name's words",
  "tf/docs/TfNewPage.svelte  places": "a handful by what it is: the parents the reader's typing narrows to",
  "tf/docs/TfNewPage.svelte  tree.spaces": "a handful by what it is: the spaces the sources sync: tens, in a select",
  "tf/filter/FilterBar.svelte  config.dropdowns ?? []": "a set the code fixes: a filter's dropdowns",
  "tf/filter/FilterBar.svelte  dropdown.options": "capped where it is read or drawn: a native select's options, never laid out until opened; Tickets' are the backend's `FIELD_SUGGESTIONS` (50) and `PARENT_SUGGESTIONS` (20)",
  "tf/filter/FilterBar.svelte  group.options": "capped where it is read or drawn: a chip group's few values (Tickets cuts them at `CHIPS_AT_MOST`)",
  "tf/filter/FilterBar.svelte  leadChips": "a set the code fixes: a filter's chip groups",
  "tf/filter/Marked.svelte  needles ? markNeedles(text, needles) : marks(text, typed)": "the runs of one line's or one name's words",
  "tf/filter/TfScope.svelte  scopes": "a set the code fixes: a filter's scopes",
  "tf/inbox/TfInbox.svelte  rowActions(entry).filter((action) => action !== \"done\" && action !== \"snooze\")": "inside one drawn row: one inbox row's actions",
  "tf/inbox/TfInbox.svelte  rowActions(entry).filter((action) => action === \"done\" || action === \"snooze\")": "inside one drawn row: one inbox row's actions",
  "tf/inbox/TfInbox.svelte  shelfDrawn": "capped where it is read or drawn: the shelf's few the read answers",
  "tf/inbox/TfInbox.svelte  snoozePresets(now, expiryOf(item))": "a set the code fixes: the snooze presets",
  "tf/item/TfItem.svelte  block.facts": "a handful by what it is: one block's facts",
  "tf/item/TfItem.svelte  block.rows": "capped where it is read or drawn: a view's entity rows, which every source cuts at a page (the Gitea branches view at 1,000, with a note)",
  "tf/item/TfItem.svelte  blocks": "a handful by what it is: one view section's blocks",
  "tf/item/TfItem.svelte  list": "a handful by what it is: one item's actions",
  "tf/item/TfItem.svelte  view.sections": "a handful by what it is: one view's sections",
  "tf/job/TfApplySet.svelte  shown": "the profile's own settings, tens at most: the reader's link sets",
  "tf/job/TfCardRows.svelte  rows": "a set the code fixes: a card's facts rows",
  "tf/job/TfConnections.svelte  groups": "a handful by what it is: the link sets that apply to one ticket",
  "tf/job/TfConnections.svelte  hidden": "on one ticket, page or pull request: what the reader hid on one ticket",
  "tf/job/TfConnections.svelte  rows": "on one ticket, page or pull request: one link set's rows for one ticket, in the drawer",
  "tf/job/TfConnections.svelte  shown": "on one ticket, page or pull request: the links on one ticket, in a drawer opened on a press",
  "tf/job/TfConversation.svelte  mentionRuns(comment.body, thread.me)": "the runs of one line's or one name's words",
  "tf/job/TfConversation.svelte  queued": "what this session queued that the source does not hold yet",
  "tf/job/TfJob.svelte  blocked": "on one ticket, page or pull request: what one ticket blocks",
  "tf/job/TfJobAttachments.svelte  files": "on one ticket, page or pull request: one ticket's attachments",
  "tf/job/TfJobCard.svelte  blockedBy": "on one ticket, page or pull request: what one ticket is blocked by",
  "tf/job/TfJobCard.svelte  labels": "on one ticket, page or pull request: one ticket's labels: plain chips",
  "tf/job/TfJobCard.svelte  others": "a handful by what it is: the other sources holding one ticket",
  "tf/job/TfLinkTo.svelte  hits": "capped where it is read or drawn: the launcher's search answers at most a page of hits",
  "tf/job/TfLinkTo.svelte  taken": "a handful by what it is: the items picked in this dialog",
  "tf/job/TfSaveSet.svelte  offers": "a handful by what it is: the sets the dialog offers",
  "tf/job/TfStartWork.svelte  options": "a handful by what it is: the repositories a ticket's work could start in",
  "tf/monitors/TfCreateMonitor.svelte  targets": "the profile's own settings, tens at most: the monitor sources",
  "tf/monitors/TfMonitors.svelte  row.actions.filter((op) => op in ROW_OPS)": "inside one drawn row: one monitor row's actions",
  "tf/monitors/TfMonitors.svelte  row.assets": "inside one drawn row: one monitor row's assets",
  "tf/monitors/TfMonitors.svelte  shownUnwatched": "a handful by what it is: assets no monitor watches, under its own filter",
  "tf/monitors/TfMonitors.svelte  stale": "a handful by what it is: monitors whose source went stale",
  "tf/monitors/TfMonitors.svelte  was.assets": "inside one drawn row: one monitor row's assets",
  "tf/note/TfNote.svelte  detail.links": "on one ticket, page or pull request: one note's links",
  "tf/note/TfNote.svelte  tokenise(detail.note.body_md, detail.refs)": "the runs of one line's or one name's words: one note's tokens, the reader's own",
  "tf/openin/TfAppPage.svelte  repos ?? []": "a handful by what it is: the repositories with a checkout on this disk, in a select",
  "tf/openin/TfAppPage.svelte  warned": "a handful by what it is: one app's warnings",
  "tf/openin/TfOpenIn.svelte  entries": "the profile's own settings, tens at most: the reader's apps",
  "tf/openin/TfOpenObject.svelte  entries": "the profile's own settings, tens at most: the reader's apps",
  "tf/pr/TfPr.svelte  askable": "a handful by what it is: the reviewers one pull request could ask",
  "tf/pr/TfPr.svelte  buildsBeyondChecks": "on one ticket, page or pull request: one pull request's builds",
  "tf/pr/TfPr.svelte  chips": "what this session queued that the source does not hold yet",
  "tf/pr/TfPr.svelte  commitList": "a handful by what it is: the commit lists of one pull request: one, and a line; each list is windowed (`TfPrCommitRows`)",
  "tf/pr/TfPr.svelte  detail?.labels ?? []": "on one ticket, page or pull request: one pull request's labels",
  "tf/pr/TfPr.svelte  overview": "a handful by what it is: the overview's parts: description, checks, a line or two",
  "tf/pr/TfPr.svelte  part.checks": "on one ticket, page or pull request: one pull request's checks",
  "tf/pr/TfPr.svelte  queuedComments": "what this session queued that the source does not hold yet",
  "tf/pr/TfPr.svelte  rail": "on one ticket, page or pull request: one pull request's checks",
  "tf/pr/TfPr.svelte  railReviewers": "on one ticket, page or pull request: one pull request's reviewers",
  "tf/pr/TfPr.svelte  standing.lines": "a handful by what it is: *Ready to merge?*'s lines",
  "tf/pr/TfPr.svelte  writes": "what this session queued that the source does not hold yet",
  "tf/pr/TfPrMerge.svelte  options": "a set the code fixes: the merge styles",
  "tf/pr/TfPrMerge.svelte  ready": "a handful by what it is: the merge dialog's lines",
  "tf/pr/TfPrMerge.svelte  reasons": "a handful by what it is: the merge dialog's reasons",
  "tf/pr/TfPrPicker.svelte  range.commits": "a handful by what it is: the commits a reader picked a range across",
  "tf/pr/TfPrReview.svelte  choices": "a set the code fixes: the review verdicts",
  "tf/pr/TfPrReview.svelte  drafts": "what this session queued that the source does not hold yet: the reader's pending review comments",
  "tf/render/TfCommandOutput.svelte  lines": "capped where it is read or drawn: a command's output shown in the viewer, cut at the viewer's cap",
  "tf/render/TfFilePipeline.svelte  commands": "the profile's own settings, tens at most: the reader's step commands",
  "tf/render/TfFilePipeline.svelte  steps": "the profile's own settings, tens at most: one pipeline's steps",
  "tf/render/TfFileViewer.svelte  content.text.replace(/\\r?\\n$/, \"\").split(\"\\n\")": "capped where it is read or drawn: an attachment or artifact preview's text, read up to the preview's cap",
  "tf/render/TfPdf.svelte  shown.sizes": "capped where it is read or drawn: a PDF's pages drawn as they come into view",
  "tf/render/TfPdfDiff.svelte  hunk.lines": "a handful by what it is: one hunk's lines",
  "tf/render/TfPdfDiff.svelte  hunks": "a handful by what it is: one page's text hunks",
  "tf/render/TfPdfDiff.svelte  shown.rows": "capped where it is read or drawn: a PDF diff's pages drawn as they come into view",
  "tf/render/TfRendererSwitch.svelte  opened.offered": "the profile's own settings, tens at most: the renderers that fit one file",
  "tf/repo/TfNewBranch.svelte  branches": "capped where it is read or drawn: a native select's options, never laid out until opened: the branches view, which the source cuts (Gitea at 1,000)",
  "tf/repo/TfRepo.svelte  drawn": "a handful by what it is: the tabs kept drawn",
  "tf/repo/TfRepo.svelte  known ?? []": "capped where it is read or drawn: a native select's options, never laid out until opened: the branches view, which the source cuts (Gitea at 1,000)",
  "tf/repo/TfRepoBranches.svelte  notes": "a handful by what it is: the lines a cut listing ends on",
  "tf/repo/TfRepoCommits.svelte  shown": "capped where it is read or drawn: the commits the view answers, which the source cuts at a page",
  "tf/repo/TfRepoCompare.svelte  branches": "capped where it is read or drawn: a native select's options, never laid out until opened: the branches view, which the source cuts (Gitea at 1,000)",
  "tf/repo/TfRepoCompare.svelte  commits": "capped where it is read or drawn: a branch's commits as its view lists them, which the source cuts at a page (Gitea's 250); left to the Compare pass",
  "tf/repo/TfRepoCompare.svelte  heads": "capped where it is read or drawn: a native select's options, never laid out until opened: the branches view, which the source cuts (Gitea at 1,000)",
  "tf/repo/TfRepoFiles.svelte  Object.entries(cutDirs).filter(([dir]) => dir === \"\" || open.has(dir))": "a handful by what it is: the notes on folders the source cut",
  "tf/repo/TfRepoFiles.svelte  goFound": "capped where it is read or drawn: *Go to file*'s best few",
  "tf/repo/TfRepoFiles.svelte  segs": "the runs of one line's or one name's words: one code line's highlighted runs",
  "tf/repo/TfRepoLocal.svelte  untracked": "a handful by what it is: a clone's untracked files",
  "tf/repo/TfRepoPrs.svelte  sent": "what this session queued that the source does not hold yet",
  "tf/sets/TfSetEditor.svelte  members": "the profile's own settings, tens at most: one set's members, picked by hand",
  "tf/sets/TfSetEditor.svelte  options(rule.kind, rule.value)": "a handful by what it is: one rule's choices",
  "tf/sets/TfSetEditor.svelte  pattern.matches": "capped where it is read or drawn: a pattern's preview: the matches the preview answers",
  "tf/sets/TfSetEditor.svelte  patterns": "the profile's own settings, tens at most: one set's patterns, the reader's own",
  "tf/sets/TfSetEditor.svelte  ruleRows": "the profile's own settings, tens at most: one set's rules",
  "tf/sets/TfSetEditor.svelte  shownKeys": "capped where it is read or drawn: the keys a rule's preview answers",
  "tf/sets/TfSetPattern.svelte  projects": "a handful by what it is: the projects the sources sync, in a select",
  "tf/sets/TfSets.svelte  keyWords(hintTickets(offer))": "the runs of one line's or one name's words",
  "tf/sets/TfSets.svelte  lines": "inside one drawn row: one set row's lines",
  "tf/sets/TfSets.svelte  rows": "the profile's own settings, tens at most: the reader's link sets",
  "tf/sets/TfSets.svelte  ruleWords(rule)": "the runs of one line's or one name's words",
  "tf/sets/TfSets.svelte  set.rules": "inside one drawn row: one set's rules",
  "tf/settings/TfAddSource.svelte  chosen.auth_methods": "a set the code fixes: one adapter's sign-in kinds",
  "tf/settings/TfAddSource.svelte  folded && chosen !== null ? [chosen] : adapters": "the profile's own settings, tens at most: the installed adapters",
  "tf/settings/TfBackupSection.svelte  listed": "the profile's own settings, tens at most: the backups kept",
  "tf/settings/TfBackupSection.svelte  ticks": "a set the code fixes: a backup's steps",
  "tf/settings/TfBoardsAndFilters.svelte  views": "the profile's own settings, tens at most: the reader's saved views",
  "tf/settings/TfCheckoutsSection.svelte  settings.applications": "the profile's own settings, tens at most: the reader's apps",
  "tf/settings/TfCheckoutsSection.svelte  worktrees": "the profile's own settings, tens at most: the reader's worktrees",
  "tf/settings/TfInstalledAdapters.svelte  asked.changes": "a handful by what it is: what an adapter update changes",
  "tf/settings/TfInstalledAdapters.svelte  row.sources": "the profile's own settings, tens at most: one adapter's sources",
  "tf/settings/TfInstalledAdapters.svelte  rows": "the profile's own settings, tens at most: the installed adapters",
  "tf/settings/TfKeyboardSection.svelte  chord.split(\" \")": "the runs of one line's or one name's words: one chord's keys",
  "tf/settings/TfKeyboardSection.svelte  keys.split(\", \")": "the runs of one line's or one name's words: one shortcut's chords",
  "tf/settings/TfNotesSection.svelte  keyCaps(view.accelerator)": "the runs of one line's or one name's words: one shortcut's keys",
  "tf/settings/TfPipelinesSettings.svelte  pipelines.list": "the profile's own settings, tens at most: the reader's file pipelines",
  "tf/settings/TfPipelinesSettings.svelte  stepCommands.list": "the profile's own settings, tens at most: the reader's step commands",
  "tf/settings/TfRenderersSection.svelte  choices": "the profile's own settings, tens at most: the renderers",
  "tf/settings/TfRenderersSection.svelte  defaults.list": "the profile's own settings, tens at most: the reader's defaults",
  "tf/settings/TfRenderersSection.svelte  pipelines.list": "the profile's own settings, tens at most: the reader's file pipelines",
  "tf/settings/TfRenderersSection.svelte  saved": "the profile's own settings, tens at most: the reader's renderers",
  "tf/settings/TfSchemaForm.svelte  field.control.options": "a handful by what it is: one config field's choices, as the adapter declares them",
  "tf/settings/TfSchemaForm.svelte  fields": "a handful by what it is: one adapter's config fields",
  "tf/settings/TfSourcePage.svelte  intervalOptions(source.sync_interval_secs, floor)": "a set the code fixes: the sync intervals",
  "tf/settings/TfSourcePage.svelte  missLines": "a handful by what it is: what one source misses",
  "tf/settings/TfSourcePage.svelte  runs": "capped where it is read or drawn: one source's last runs, which the read cuts",
  "tf/settings/TfSources.svelte  sources": "the profile's own settings, tens at most: the configured sources",
  "tf/settings/TfStateColours.svelte  runs(line)": "the runs of one line's or one name's words",
  "tf/settings/TfStateColours.svelte  stored": "the profile's own settings, tens at most: the reader's state colours",
  "tf/settings/TfStateColours.svelte  words.filter((w) => columnOf(w) === column)": "a handful by what it is: the state words the sources use",
  "tf/tickets/SavedViewSourceLine.svelte  queries": "a handful by what it is: one view's queries",
  "tf/tickets/SavedViewSourceLine.svelte  savedViewSource.unsynced": "a handful by what it is: the projects one view names that no source syncs",
  "tf/tickets/Signals.svelte  signals": "on one ticket, page or pull request: one ticket's signals",
  "tf/tickets/TicketCard.svelte  labels": "inside one drawn row: one card's labels, inside the windowed board",
  "tf/tickets/TicketsPage.svelte  columnsOf(lane)": "a handful by what it is: one lane's status columns",
  "tf/tickets/TicketsPage.svelte  drawnNow(placeAt(lane, column), heldCards.get(placeAt(lane, column)) ?? [])": "capped where it is read or drawn: a column's cards, drawn as the board scrolls (`drawnNow`)",
  "tf/tickets/TicketsPage.svelte  group.views": "the profile's own settings, tens at most: the reader's saved views",
  "tf/tickets/TicketsPage.svelte  groups": "the profile's own settings, tens at most: the saved views' groups",
  "tf/tickets/TicketsPage.svelte  hidden": "the profile's own settings, tens at most: the saved views the reader hid",
  "tf/tickets/TicketsPage.svelte  listColumns": "a set the code fixes: the list's columns",
  "tf/tickets/TicketsPage.svelte  reader.bands": "capped where it is read or drawn: the board's lanes: the fields a board groups by",
  "tf/tickets/ValuePicker.svelte  drawn": "capped where it is read or drawn: the picker's best few",
  "tf/tickets/ValuePicker.svelte  options": "capped where it is read or drawn: a native select's options",
  "tf/tickets/ValuePicker.svelte  picked": "a handful by what it is: the values the reader picked",
  "tf/tickets/ValuePicker.svelte  runs": "the runs of one line's or one name's words",
  "tf/tickets/ViewEditor.svelte  column.cards": "capped where it is read or drawn: the preview's few cards a column",
  "tf/tickets/ViewEditor.svelte  columns ?? []": "the profile's own settings, tens at most: one view's columns",
  "tf/tickets/ViewEditor.svelte  conditions": "the profile's own settings, tens at most: one view's conditions",
  "tf/tickets/ViewEditor.svelte  copiedFrom.left": "the runs of one line's or one name's words",
  "tf/tickets/ViewEditor.svelte  drawnList": "a set the code fixes: the list's columns",
  "tf/tickets/ViewEditor.svelte  fieldChoices(condition)": "a handful by what it is: one condition's fields",
  "tf/tickets/ViewEditor.svelte  groups": "a set the code fixes: the view's grouping choices",
  "tf/tickets/ViewEditor.svelte  listRows": "capped where it is read or drawn: the preview's few rows",
  "tf/tickets/ViewEditor.svelte  parts": "the runs of one line's or one name's words",
  "tf/tickets/ViewEditor.svelte  shown.columns": "a handful by what it is: the preview's columns",
  "tf/tickets/ViewEditor.svelte  statuses": "a handful by what it is: the statuses a view's columns hold",
  "tf/time/TfBlockDialog.svelte  choices": "capped where it is read or drawn: the dialog's best few",
  "tf/time/TfDay.svelte  drawn": "a handful by what it is: one day's time blocks, placed",
  "tf/time/TfDay.svelte  hours": "a set the code fixes: a day's hours",
  "tf/time/TfDay.svelte  rows": "a handful by what it is: one day's time blocks",
  "tf/time/TfLogAll.svelte  rows": "a handful by what it is: the blocks one *Log all* sends",
  "tf/time/TfStandup.svelte  form.attendees": "a handful by what it is: the standup's attendees",
  "tf/time/TfStandup.svelte  form.items": "a handful by what it is: the standup's items",
  "tf/time/TfStandup.svelte  known": "a handful by what it is: the standup's attendees",
  "tf/time/TfStandup.svelte  lines": "a handful by what it is: one standup digest's lines",
  "tf/time/TfWeek.svelte  ready.held": "a handful by what it is: the week's notes",
  "tf/time/TfWeek.svelte  ready.week.days": "a set the code fixes: a week's days",
  "tf/time/TfWeek.svelte  row.cells": "a set the code fixes: a row's seven days",
  "tf/time/TfWeek.svelte  shown.dayTotals": "a set the code fixes: a week's days",
  "tf/time/TfWeek.svelte  shown.rows": "a handful by what it is: the week's rows: what the reader logged time on",
};

test("every {#each} over source data is windowed, cut, or named small with its reason", () => {
  expect(unsafe().filter((key) => !(key in SMALL))).toEqual([]);
});

test("every list named small is still there, so the list keeps up with the code", () => {
  const now = new Set(unsafe());
  expect(Object.keys(SMALL).filter((key) => !now.has(key))).toEqual([]);
});

test("the rule tells the shapes apart", () => {
  const caught = (markup: string) => eaches("x.svelte", markup).length;
  expect(caught("{#each rows as row (row.id)}<p>{row.id}</p>{/each}")).toBe(1);
  expect(caught("{#each items.filter((x) => x.on) as row}<p></p>{/each}")).toBe(1);
  expect(caught("{#each win.slice(rows) as row (row.id)}<p></p>{/each}")).toBe(0);
  expect(caught("{#each fileWin.slice(drawn) as file}<p></p>{/each}")).toBe(0);
  expect(caught("{#each matches.slice(0, DRAWN) as row}<p></p>{/each}")).toBe(0);
  expect(caught("{#each matches.slice(0, 40) as row}<p></p>{/each}")).toBe(1);
  expect(caught("{#each TABS as tab}<p></p>{/each}")).toBe(0);
  expect(caught("{#each [0, 1, 2] as n}<p></p>{/each}")).toBe(0);
  expect(caught("{#each { length: count } as _}<p></p>{/each}")).toBe(0);
});
