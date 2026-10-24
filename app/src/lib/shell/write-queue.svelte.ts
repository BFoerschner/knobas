/**
 * What knobas still owes each source, for the status bar and the queue panel
 * (issue #42).
 *
 * Two things live here rather than in the component: the **loading** of the
 * queue, because the badge and the panel must never disagree about it, and the
 * **reading** of a snapshot, because "what changed" is per-op and a component
 * should not be the place that knows it.
 *
 * ## Why there is no `write:*` event to listen to
 *
 * Every queue transition already writes an activity line, so `activity:new`
 * is the signal that something moved. A second channel saying the same thing
 * would be a second thing to keep in step. The component subscribes and calls
 * {@link WriteQueue.refresh}; this module does not listen on its own, for the
 * reason `latest-change` does not either — a subscription belongs to the
 * lifetime of a component, not of a module.
 *
 * ## The rule this module must not break
 *
 * A held write is terminal until the user acts. **Nothing here may release
 * one on a timer, on a refresh, or as part of a flush** — `flushWrites` cannot
 * do it (the backend will not offer a held write to its flush loop), and
 * nothing in this file may do it either. `write-queue.test.svelte.ts` runs the
 * clock forward over a held write and asserts it is still held.
 */
import { keyOf, minutesLabel } from "../time/format";
import {
  amendWrite,
  applyHeldWrite,
  discardWrite,
  flushWrites,
  pendingWrites,
  writeQueueCounts,
  type QueuedWrite,
  type QueueCounts,
  type WriteOpPayload,
  type WriteState,
} from "../ipc/sources";
import { minutesOf } from "../time/week";

/**
 * Whether the queue panel is open. Module-level for one reason: a toast about
 * a write the queue refused or held (#752) offers *Open write queue*, and a
 * toast is raised far from the status bar that draws the panel.
 */
export const writeQueuePanel = $state<{ open: boolean }>({ open: false });

/** Open the queue panel, from wherever a write's outcome was told. */
export function openWriteQueue(): void {
  writeQueuePanel.open = true;
}

/**
 * Back to a fresh window: the panel shut and nobody waiting on a withdrawal.
 * Module-level state is the window's one, so a test calls this between runs.
 */
export function reset(): void {
  writeQueuePanel.open = false;
  withdrawals.clear();
}

/** Nothing owed, and nothing asked yet — the counts before the first read. */
const NONE: QueueCounts = { pending: 0, held: 0, refused: 0 };

/**
 * How many writes need a **decision** rather than patience.
 *
 * Held and refused both do, and for the same reason from the user's side:
 * neither will ever move on its own. Kept as one number because the badge has
 * one slot for "you owe somebody an answer", and split back apart in the panel
 * where there is room to say which.
 */
export function decisionsIn(counts: QueueCounts): number {
  return counts.held + counts.refused;
}

/**
 * What one row needs from the reader, in one word.
 *
 * The distinction story 18 turns on: `waiting` is patience, `decide` is a
 * decision. A UI that showed one badge over both would be telling the user
 * that a conflict is something that resolves itself.
 */
export type Demand = "waiting" | "decide" | "settled";

export function demandOf(state: WriteState): Demand {
  switch (state) {
    case "pending":
      return "waiting";
    case "held":
    case "refused":
      return "decide";
    // `sent` and `discarded` never reach the panel, but a total switch is what
    // makes a *new* state a type error here rather than a row that silently
    // renders as patience.
    case "sent":
    case "discarded":
      return "settled";
  }
}

/**
 * The held or refused write each waiting write is stuck behind (#1473), by the
 * waiting write's id.
 *
 * One item's writes go in queue order: `knobas_core::write_queue::due` offers
 * an entity's oldest open write and nothing for an entity whose oldest is held
 * or refused. So a pending write with an earlier held or refused write on the
 * same item does not go on its own; it goes once that one is sent or
 * discarded. The blocker is the oldest such write, the one `due` stops on.
 *
 * Read off what every row already carries (`entity_id`, the queue-order `id`,
 * `state`); a waiting write with no entry here waits only on its source.
 */
export function blockers<Row extends Pick<QueuedWrite, "id" | "entity_id" | "state">>(rows: readonly Row[]): Map<number, Row> {
  const stalled = new Map<string, Row>();
  for (const row of rows) {
    if (demandOf(row.state) !== "decide") continue;
    const oldest = stalled.get(row.entity_id);
    if (oldest === undefined || row.id < oldest.id) stalled.set(row.entity_id, row);
  }
  const found = new Map<number, Row>();
  for (const row of rows) {
    if (demandOf(row.state) !== "waiting") continue;
    const blocker = stalled.get(row.entity_id);
    if (blocker !== undefined && blocker.id < row.id) found.set(row.id, blocker);
  }
  return found;
}

/**
 * One side of the two-versions comparison, in a shape a template can render.
 *
 * A snapshot is `knobas_core::write_queue::project`'s output, and its shape is
 * **per op**: `{op, live, text}` for a comment, and the whole mirrored record
 * for an op with no stated projection. So this narrows rather than assumes,
 * and says plainly when it cannot read one — a dialog that rendered
 * `[object Object]` beside a comment would be worse than one that says it does
 * not know how to show this op.
 */
export interface SnapshotView {
  /** The target's text, when the projection carries one. */
  text: string | null;
  /** False when the mirror has no live item — withdrawn, or never synced. */
  live: boolean;
  /** Set when the snapshot is not a shape this build knows how to render. */
  raw: string | null;
  /**
   * The **version** this side of the comparison is of, where the op has one.
   *
   * A held page edit is the one write whose two sides can look identical while
   * differing in what matters: a wiki page whose body was reformatted, or
   * moved, or whose macro re-rendered, reads the same stripped down to text.
   * The number is what says the mirror moved (#286), so the panel shows it
   * beside each side rather than asking the reader to spot the difference.
   *
   * `null` for every other op. Read only from an `update_page` snapshot, which
   * one adapter declares and no other — so this is gated on the op the same
   * way `storage-format.ts`'s reads are gated on the adapter kind, and it
   * misses to `null` like them.
   */
  version: number | null;
}

export function readSnapshot(snapshot: unknown): SnapshotView {
  if (typeof snapshot !== "object" || snapshot === null) {
    return {
      text: null,
      live: false,
      raw: snapshot === undefined ? null : JSON.stringify(snapshot),
      version: null,
    };
  }
  const bag = snapshot as Record<string, unknown>;
  const live = bag.live === true;
  const version = pageVersion(bag);
  if (typeof bag.text === "string") return { text: bag.text, live, raw: null, version };
  if (bag.text === null) return { text: null, live, raw: null, version };
  // A projection this build has no reader for. Shown verbatim rather than
  // hidden: the user is being asked to decide, and "I cannot show you what
  // changed" is information they need in order to answer honestly.
  return { text: null, live, raw: JSON.stringify(snapshot, null, 2), version };
}

/**
 * `version.number` out of an `update_page` snapshot's mirrored payload.
 *
 * The whole-record projection carries the payload verbatim, and a Confluence
 * page keeps its version there. Nothing else is read out of it, and nothing
 * but `update_page` is read at all: a `version` key means different things in
 * different sources, and guessing from a payload's shape is the guess ADR-0007
 * forbids.
 */
function pageVersion(bag: Record<string, unknown>): number | null {
  if (bag.op !== "update_page") return null;
  const payload = bag.payload;
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  const version = (payload as Record<string, unknown>).version;
  if (typeof version !== "object" || version === null || Array.isArray(version)) return null;
  const number = (version as Record<string, unknown>).number;
  return typeof number === "number" && Number.isSafeInteger(number) ? number : null;
}

/**
 * The editable text of a write, when there is one.
 *
 * `WriteOp` grows per milestone (ADR-0006), and a build meeting an op it does
 * not know must still be able to *apply* and *discard* it — only *edit* needs
 * to know where the words are. `null` therefore means "no edit box", not "no
 * actions".
 */
export function editableBody(payload: unknown): string | null {
  if (typeof payload !== "object" || payload === null) return null;
  const comment = (payload as { Comment?: { body?: unknown } }).Comment;
  if (comment) return typeof comment.body === "string" ? comment.body : null;
  // A file's commit (#845): its words are the file's whole new text.
  const commit = (payload as { CommitFile?: { content?: unknown } }).CommitFile;
  return commit && typeof commit.content === "string" ? commit.content : null;
}

/**
 * A commit the source held because the file moved since it was read (#822,
 * #845): the reader's version of the file and the source's, side by side.
 * `null` for any other write, and for a commit held for another reason (the
 * held side names no `base`).
 */
export function movedFile(row: Pick<QueuedWrite, "payload" | "held_snapshot">): { path: string; mine: string; theirs: string } | null {
  const commit = (row.payload as { CommitFile?: { path?: unknown; content?: unknown } } | null)?.CommitFile;
  const held = row.held_snapshot as { text?: unknown; base?: unknown } | null;
  if (!commit || typeof commit.content !== "string" || typeof commit.path !== "string") return null;
  if (held === null || typeof held !== "object" || typeof held.text !== "string" || typeof held.base !== "string") return null;
  return { path: commit.path, mine: commit.content, theirs: held.text };
}

/**
 * A write knobas sent that its source never answered (#1125): it may have
 * landed, so the queue holds it rather than sending it again. `source` is the
 * source's display name, the place the reader checks; `detail` is what the
 * transport said. `null` for every other row (the held side names no
 * `unconfirmed`).
 */
export function unconfirmedSend(row: Pick<QueuedWrite, "state" | "held_snapshot">): { source: string; detail: string } | null {
  if (row.state !== "held") return null;
  const held = row.held_snapshot as { unconfirmed?: { source?: unknown; detail?: unknown } } | null;
  const said = held !== null && typeof held === "object" ? held.unconfirmed : undefined;
  if (!said || typeof said.source !== "string") return null;
  return { source: said.source, detail: typeof said.detail === "string" ? said.detail : "" };
}

const withdrawals = new Set<{ id: number; gone: () => void }>();

/**
 * The queue panel withdrew write `id` (its `discard` succeeded). A withdrawn
 * row and a sent one both leave `pending_writes`, so the read cannot tell
 * them apart; the withdrawal is the one client-visible signal (#1583).
 */
export function noteWithdrawn(id: number): void {
  for (const w of [...withdrawals]) if (w.id === id) w.gone();
}

/** Call `gone` when write `id` is withdrawn in the queue panel; answers the stop. */
export function onWithdrawn(id: number, gone: () => void): () => void {
  const w = { id, gone };
  withdrawals.add(w);
  return () => void withdrawals.delete(w);
}

/** What a queued `log_work` write would put on a ticket, read off its payload. */
export interface WithdrawnWorklog {
  /** The ticket the time was logged against. */
  entity: string;
  /** RFC 3339 -- when the logged span began. */
  started: string;
  /** How long was worked, in the seconds that cross the wire. */
  seconds: number;
  comment: string;
}

/**
 * The worklog a discard would withdraw, when withdrawing it can bill the same
 * hour twice. `null` when the withdrawal is an ordinary one.
 *
 * ## Why `log_work` and no other op
 *
 * Since #328 a discarded worklog **gives its blocks back**: the local copy
 * goes with the write and the hour is unlogged again, offered by *Log all*
 * and editable in the day review. Set that beside the race
 * `knobas_core::write_queue::discard` names in its own doc comment — the
 * flush loop's per-source lock does not hold a discard back, and every writer
 * of `attempts` bumps it *after* the call, so a write in flight is
 * indistinguishable from one never tried — and the window is one HTTP
 * round-trip wide. Inside it, the hour is at Jira *and* back on knobas'
 * timesheet, one click from being sent again.
 *
 * No other op has that shape, and `discard`'s own statement is the reason: it
 * deletes from `knobas.worklog` and from nothing else, so `log_work` is the
 * only op holding a local copy a withdrawal can give back. That is the
 * question each bullet answers, and the only one: where the other question
 * touches a bullet, the bullet points instead of arguing.
 *
 * * `comment`, `approve`, `transition`, `trigger_build`, `rerun_build` —
 *   withdrawing one in flight leaves the reply posted or the status moved,
 *   which is the write the user asked for arriving. Nothing is re-offered and
 *   nothing is charged twice.
 * * `create_ticket` — ADR-0012 names it as the create that is *not* naturally
 *   idempotent, but the duplicate it warns about is the **queue re-sending**,
 *   which a discard prevents rather than causes, so no hour comes back to the
 *   timesheet.
 * * `create_branch`, `create_pull_request` — a withdrawal releases nothing
 *   here either. What one can leave standing at the source is the other
 *   question, and so is start-work's retry of the step that made it.
 * * `create_page`, `update_page` — the same, and which of the two can leave
 *   something standing is that other question too.
 *
 * That other question — did the source make something knobas cannot name,
 * `CONTEXT.md`'s **unclaimed write** — is asked and answered in
 * `knobas_sync::write_queue::unclaimed`'s doc comment. Issue #353 settled
 * it there, and settled it against the answer that used to sit on the
 * `create_branch` line, that a 409 stops the duplicate: that one asks whether
 * a *re-send* makes a second artefact, which is neither question. It is the
 * 409 *answer* that does not return to these lines — `create_ticket`'s bullet
 * names re-sending only to say a discard is not what causes it.
 *
 * Those bullets and `log_work` itself are the whole of `WriteOp` as ADR-0006's
 * set currently stands, and they are enumerated here **without a guard**.
 * `knobas-sync`'s
 * `every_write_op_says_whether_a_withdrawal_can_leave_one` matches on the enum
 * with no wildcard arm, so a new variant stops that test compiling; nothing
 * does the same for these bullets, which is why they point at that doc rather
 * than compete with it.
 *
 * ## Why the state is not part of the question
 *
 * It is tempting to ask only about a `pending` row, on the grounds that
 * `knobas_core::write_queue::due` yields an entity's head only when it is
 * pending and so the flush loop cannot be holding a held or refused one. That
 * reasoning is wrong in both directions, and `discard` releases the blocks of
 * all three states alike:
 *
 * * **Refused.** `knobas_source_jira::write::log_work` answers
 *   `SourceError::Protocol` "if Jira accepted the worklog but did not name
 *   it", and a bare `Protocol` is not retryable, so that row lands in
 *   `refused`. It is a refusal whose worklog is *at Jira*.
 * * **Held.** Hold detection runs *before* the send, so a write that arrived
 *   and whose `sent` never landed sits `pending` until the next flush -- which
 *   re-reads the target, finds it moved, and holds it. The hour is at Jira and
 *   the row says the target changed.
 *
 * The honest rule is the one `discard`'s own doc comment states: nothing in
 * the queue tells a write that arrived and was never settled from one that
 * never left -- not the state, and not `attempts`, which every writer bumps
 * *after* the call. So every open worklog is asked about, and the dialog says
 * exactly that rather than pretending to know which.
 *
 * Only *open* rows ever reach this: `pendingWrites` returns `pending`, `held`
 * and `refused` and nothing else, so a settled row is not a case to guard.
 *
 * `WriteOp` grows per milestone (ADR-0006), so the payload is narrowed rather
 * than asserted: a build that cannot read this worklog cannot describe it
 * either, and a dialog with blanks where the hours go is worse than none.
 */
export function withdrawnWorklog(row: QueuedWrite): WithdrawnWorklog | null {
  if (row.op !== "log_work") return null;
  if (typeof row.payload !== "object" || row.payload === null) return null;
  const log = (row.payload as { LogWork?: Record<string, unknown> }).LogWork;
  if (!log) return null;
  if (typeof log.entity !== "string" || typeof log.started !== "string") return null;
  if (typeof log.seconds !== "number") return null;
  return {
    entity: log.entity,
    started: log.started,
    seconds: log.seconds,
    comment: typeof log.comment === "string" ? log.comment : "",
  };
}

/**
 * The same payload with its text replaced, ready for `amendWrite`.
 *
 * A commit's edit answers the version the source holds now: where the held
 * side names the blob it found (`base`, #822), the amended commit is made
 * against that blob, or the source would hold it again over the same move.
 */
export function withBody(payload: unknown, body: string, heldSnapshot: unknown = null): WriteOpPayload | null {
  if (typeof payload !== "object" || payload === null) return null;
  const comment = (payload as { Comment?: { entity?: unknown } }).Comment;
  if (comment) return typeof comment.entity === "string" ? { Comment: { entity: comment.entity, body } } : null;
  const commit = (payload as { CommitFile?: Extract<WriteOpPayload, { CommitFile: unknown }>["CommitFile"] }).CommitFile;
  if (!commit || typeof commit.repo !== "string") return null;
  const base = (heldSnapshot as { base?: unknown } | null)?.base;
  return { CommitFile: { ...commit, content: body, base_sha: typeof base === "string" ? base : commit.base_sha } };
}

/**
 * One piece of a row's heading. `code` marks a real code value -- a ticket
 * key, a branch, a path -- which the panel sets in mono; the words around it
 * are not (the design system keeps mono off labels).
 */
export interface WritePart {
  text: string;
  code?: true;
}

/** What `writeWords` reads off a row: the op, its target, and for a page edit the page it was made against. */
export type WordedRow = Pick<QueuedWrite, "op" | "entity_id" | "payload"> & { target_snapshot?: unknown };

/**
 * What a queued write does, in words (#1425): *Comment on PAY-231*, *1 h
 * worklog on PAY-228*, *Pause monitor Payments API*.
 *
 * The op's wire name and the raw entity id are never drawn. A ticket or pull
 * request is named by its key, the half of the id after the source's prefix,
 * as the launcher's rows name it. A monitor, a page or a build configuration
 * has no key a person reads, so `name` looks its title up; while it has none
 * the heading says what kind of thing it is (*Pause a monitor*). A payload
 * this build cannot read says *A write*, or *A write to* the target's name.
 *
 * A page edit names the section it changes, *Edit section “Context” of
 * SEPA payout retry design*, where {@link editedSection} finds exactly one;
 * otherwise it names the page.
 */
export function writeWords(
  row: WordedRow,
  name: (entityId: string) => string | null,
): WritePart[] {
  const bag = (typeof row.payload === "object" && row.payload !== null ? row.payload : {}) as Record<string, Record<string, unknown> | undefined>;
  const first = Object.keys(bag)[0];
  // Typed as the union's variants so the `default` below stops compiling when
  // `WriteOp` gains one (ADR-0006); a variant this build has never heard of
  // still lands there at run time.
  const variant = first as PayloadVariant | undefined;
  const op = variant === undefined ? undefined : bag[variant];
  const str = (key: string): string | null => (typeof op?.[key] === "string" ? (op[key] as string) : null);
  const target = str("entity") ?? row.entity_id;
  const key: WritePart = { text: keyOf(target), code: true };
  /** The target by its title, or by what it is when the title is not known. */
  const titled = (named: string, unnamed: string): WritePart[] => {
    const title = name(target);
    return [{ text: title === null ? unnamed : `${named} ${title}` }];
  };
  const quoted = (lead: string, field: string): WritePart[] | null => {
    const value = str(field);
    return value === null ? null : [{ text: `${lead} “${value}”` }];
  };

  const said = ((): WritePart[] | null => {
    switch (variant) {
      case "Comment":
        return [{ text: "Comment on " }, key];
      case "Transition":
        return [{ text: "Move " }, key, { text: ` to ${str("status") ?? "another status"}` }];
      case "Approve":
        return [{ text: "Approve " }, key];
      case "LogWork": {
        const seconds = op?.seconds;
        if (typeof seconds !== "number") return null;
        return [{ text: `${minutesLabel(minutesOf(seconds))} worklog on ` }, key];
      }
      case "CreateTicket":
        return quoted("New ticket", "title");
      case "CreateBranch": {
        const branch = str("name");
        return branch === null ? null : [{ text: "Create branch " }, { text: branch, code: true }];
      }
      case "CreatePullRequest":
        return quoted("Open pull request", "title");
      case "MergePullRequest":
        return [{ text: "Merge " }, key];
      case "SubmitReview":
        return [{ text: "Review of " }, key];
      case "EditPullRequest":
        return [{ text: "Edit " }, key];
      case "UpdatePullRequestBranch":
        return [{ text: "Update the branch of " }, key];
      case "DeleteBranch":
        return titled("Delete branch", "Delete a branch");
      case "CommitFile": {
        const path = str("path");
        const branch = str("new_branch") ?? str("branch");
        if (path === null || branch === null) return null;
        return [{ text: "Commit " }, { text: path, code: true }, { text: " to " }, { text: branch, code: true }];
      }
      case "CreatePage":
        return quoted("New page", "title");
      case "UpdatePage": {
        const body = str("body");
        const section = body === null ? null : editedSection(row.target_snapshot, body);
        if (section === null) return titled("Edit page", "Edit a page");
        const title = name(target);
        return [{ text: `Edit section “${section}” of ${title ?? "a page"}` }];
      }
      case "PauseMonitor":
        return titled("Pause monitor", "Pause a monitor");
      case "ResumeMonitor":
        return titled("Resume monitor", "Resume a monitor");
      case "CreateMonitor":
        return quoted("New monitor", "name");
      case "TriggerBuild":
      case "RunBuild":
        return titled("Run", "Run a build");
      case "RerunBuild":
        return titled("Re-run", "Re-run a build");
      case "SetParameter": {
        const parameter = str("name");
        if (parameter === null) return null;
        const title = name(target);
        return [{ text: "Set " }, { text: parameter, code: true }, { text: title === null ? "" : ` on ${title}` }];
      }
      case "SourceAction":
        return titled("Action on", "An action on an item");
      default: {
        const unheard: undefined = variant;
        void unheard;
        return null;
      }
    }
  })();
  if (said !== null) return said.filter((part) => part.text !== "");
  const title = name(row.entity_id);
  return [{ text: title === null ? "A write" : `A write to ${title}` }];
}

/**
 * Whether a row's heading names its target by a looked-up title, so the panel
 * knows which rows to ask a name for: `writeWords` asks for one or it does not.
 */
export function namedByTitle(row: WordedRow): boolean {
  let asked = false;
  writeWords(row, () => {
    asked = true;
    return null;
  });
  return asked;
}

/**
 * The heading of the one section a queued page edit changes (#1520), or
 * `null` when it cannot say which.
 *
 * `UpdatePage` carries the page's **whole** storage format: *Save section*
 * replaces one section's body and keeps everything outside it byte for byte
 * (`DocWrites::edit_section`). So the queued body and the page it was made
 * against differ in one stretch, found by their common start and end, and the
 * section is the heading that stretch follows. The page it was made against
 * is the queue-time snapshot's mirrored record, the whole-record projection
 * `knobas_core::write_queue::project` gives this op.
 *
 * A payload read outside an adapter (ADR-0007), confined to this function:
 * the record's `body.storage.value` and its `<h1>`–`<h6>` headings, as the
 * Confluence adapter keeps them. Every level, because the adapter's section
 * split (`spans` in `knobas-source-confluence`'s `blocks.rs`) starts a section
 * at every level, so the heading named is the one *Save section* edited under.
 * Until the wiki role declares where a page's sections live (ADR-0007's
 * destination), another wiki source's record misses here. It **misses to the page name**, never to
 * another section: no snapshot, another op's, no storage body, no difference,
 * a difference before the first heading, or one that touches a heading or
 * spans two sections all answer `null`, and the row says *Edit page …* as it
 * did before. Pinned by "a page edit names the page when no one section is
 * the change".
 */
export function editedSection(snapshot: unknown, body: string): string | null {
  if (typeof snapshot !== "object" || snapshot === null) return null;
  const bag = snapshot as { op?: unknown; payload?: { body?: { storage?: { value?: unknown } } } | null };
  if (bag.op !== "update_page") return null;
  const base = bag.payload?.body?.storage?.value;
  if (typeof base !== "string" || base === body) return null;

  let from = 0;
  while (from < base.length && from < body.length && base[from] === body[from]) from += 1;
  let back = 0;
  while (back < base.length - from && back < body.length - from && base[base.length - 1 - back] === body[body.length - 1 - back]) back += 1;
  const changedTo = base.length - back;

  const heading = /<h([1-6])\b[^>]*>([\s\S]*?)<\/h\1\s*>/gi;
  let owner: { end: number; text: string } | null = null;
  for (const match of base.matchAll(heading)) {
    const start = match.index;
    const end = start + match[0].length;
    if (end <= from) owner = { end, text: match[2]! };
    // A heading that starts inside the change, or straddles its start: the
    // change is not one section's body.
    else if (start < changedTo) return null;
  }
  // A heading the edit adds.
  if (/<h[1-6]\b/i.test(body.slice(from, body.length - back))) return null;
  if (owner === null) return null;
  const text = decodeEntities(owner.text.replace(/<[^>]*>/g, "")).replace(/\s+/g, " ").trim();
  return text === "" ? null : text;
}

/** The character references a heading's storage format spells, as characters. */
function decodeEntities(text: string): string {
  const named: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (whole, ref: string) => {
    if (ref[0] === "#") {
      const code = ref[1] === "x" || ref[1] === "X" ? parseInt(ref.slice(2), 16) : parseInt(ref.slice(1), 10);
      return Number.isFinite(code) && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
    }
    return named[ref.toLowerCase()] ?? whole;
  });
}

/** `"Comment"`, `"LogWork"`, … -- the keys of the `WriteOp` payload union. */
type PayloadVariant = WriteOpPayload extends infer Payload ? (Payload extends Record<infer Key, unknown> ? Key : never) : never;

export interface WriteQueue {
  /** The counts, `{0,0,0}` until the first read answers. */
  readonly counts: QueueCounts;
  /** Every open write, newest first. */
  readonly rows: QueuedWrite[];
  /** Set when the last read or action failed, so the panel can say so. */
  readonly error: string | null;
  /**
   * Whether a read has ever succeeded (#746): `rows` being `[]` means "nothing
   * is queued" only after this; before it, it is the list the store was built
   * with.
   */
  readonly answered: boolean;
  /** True while an action is in flight, so a button cannot be double-fired. */
  readonly busy: boolean;
  /** True while a *Flush now* is out, so its button cannot be double-fired (#1671). */
  readonly flushing: boolean;
  refresh(): Promise<void>;
  flush(sourceId: string | null): Promise<void>;
  apply(id: number): Promise<void>;
  discard(id: number): Promise<void>;
  /** Resolves `true` when the amend ran, `false` when it was skipped because the store was busy. */
  amend(id: number, payload: WriteOpPayload): Promise<boolean>;
}

export function createWriteQueue(): WriteQueue {
  const state = $state<{
    counts: QueueCounts;
    rows: QueuedWrite[];
    error: string | null;
    busy: boolean;
    flushing: boolean;
    answered: boolean;
  }>({ counts: NONE, rows: [], error: null, busy: false, flushing: false, answered: false });

  async function refresh(): Promise<void> {
    try {
      // Both, and in one place: a badge that said "1 needs you" over a panel
      // listing nothing is the two reads having been taken at different times.
      const [counts, rows] = await Promise.all([writeQueueCounts(), pendingWrites()]);
      state.counts = counts;
      state.rows = rows;
      state.error = null;
      state.answered = true;
    } catch (error) {
      // The counts are left where they were rather than zeroed: a failed read
      // is not evidence that nothing is owed, and a badge that blinked to zero
      // during a hiccup is a held write the user stops looking for.
      state.error = message(error);
    }
  }

  /**
   * Run one action, then re-read.
   *
   * Always re-reads, including after a failure: an action that failed with
   * `conflict` failed *because* the queue moved, so the list on screen is the
   * stale thing that caused it.
   */
  async function act(run: () => Promise<void>): Promise<boolean> {
    if (state.busy) return false;
    state.busy = true;
    try {
      await run();
      state.error = null;
    } catch (error) {
      state.error = message(error);
    } finally {
      state.busy = false;
      await refresh();
    }
    return true;
  }

  /**
   * *Flush now* (#775): the send, so it takes no `busy` -- the panel's other
   * answers stay pressable while it is out -- and the re-read follows it as
   * `act`'s does, failure included. Its own `flushing` flag (#1671) keeps a
   * second click from sending a second `flush_writes`: one invoke per click
   * that lands, none while one is out.
   */
  async function flush(sourceId: string | null): Promise<void> {
    if (state.flushing) return;
    state.flushing = true;
    try {
      await flushWrites(sourceId);
      state.error = null;
    } catch (error) {
      state.error = message(error);
    } finally {
      state.flushing = false;
    }
    await refresh();
  }

  return {
    get counts() {
      return state.counts;
    },
    get rows() {
      return state.rows;
    },
    get error() {
      return state.error;
    },
    get answered() {
      return state.answered;
    },
    get busy() {
      return state.busy;
    },
    get flushing() {
      return state.flushing;
    },
    refresh,
    flush,
    apply: async (id) => void (await act(() => applyHeldWrite(id))),
    discard: async (id) => {
      await act(async () => {
        await discardWrite(id);
        noteWithdrawn(id);
      });
    },
    amend: (id, payload) => act(() => amendWrite(id, payload)),
  };
}

/** Whatever a rejection was, as something displayable. */
function message(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const text = (error as { message: unknown }).message;
    if (typeof text === "string") return text;
  }
  return error instanceof Error ? error.message : String(error);
}
