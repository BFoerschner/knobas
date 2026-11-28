/**
 * **What a landed write could not read back, said** (ADR-0064 rule 3, #2596). After a write lands, knobas reads back the
 * item and what the write moved (its effects); what it could not read the backend tells once on `write:unread`, and
 * this module says it in one quiet toast instead of waiting in silence:
 *
 *     PAY-231 shows once the sync of Jira under way ends.
 *     Retry SEPA payouts shows after the next sync of Jira, in about 14 min.
 *     What else the write to Fix rounding moved shows after Sync now on Gitea.
 *     The other boards and filters with PAY-231 show after the next sync of Jira.
 *
 * One sentence per source and wording, naming the source and, where the backend knows it, when that source runs next.
 * A source that syncs by hand offers *Sync now* on the toast. The toast goes early once what it names arrives: a refresh
 * of the item, or the end of a run of its source that began after the toast (any run's end, for one under way). It
 * stands over the page in the toast stack, so nothing on the page moves.
 */
import { EVENTS } from "../ipc";
import { windowListen } from "../ipc/shared-listen";
import { syncNow } from "../ipc/sources";
import type { RefreshEnded, SourceSyncStatus, Unread, WriteUnread } from "../ipc/sources";
import { amend, dismiss, push, toasts } from "./toasts.svelte";

/** When the next sync of a source comes: *after Sync now on X* for one that syncs by hand, else *after the next sync of X*. */
export function afterSync(name: string, byHand: boolean): string {
  return byHand ? `after Sync now on ${name}` : `after the next sync of ${name}`;
}

/** How long the note stays when nothing it names arrives: a sentence to read, and nothing to press but *Sync now*. */
const NOTE_MS = 10_000;

/** "in about 4 min", "in about 2 h", or nothing for a time past or unknown. */
function inAbout(at: string | null, now: number): string {
  if (at === null) return "";
  const minutes = Math.round((Date.parse(at) - now) / 60_000);
  if (!(minutes >= 1)) return "";
  return minutes < 90 ? `, in about ${minutes} min` : `, in about ${Math.round(minutes / 60)} h`;
}

/** The sentence's ending for one line: when what it names shows. */
function when(line: Unread, now: number): string {
  if (line.why === "run_in_flight") return `once the sync of ${line.source_name} under way ends`;
  if (line.why === "turned_off") return `${afterSync(line.source_name, line.by_hand)}, which is turned off`;
  return `${afterSync(line.source_name, line.by_hand)}${line.by_hand ? "" : inAbout(line.next_run_at, now)}`;
}

/** What one line names, in the sentence. */
function subject(line: Unread, note: WriteUnread): string {
  if (line.why === "left_out") return `the other boards and filters with ${line.title}`;
  // Beside the write's own item, "the write" is plain; alone, it names what the write was to.
  if (line.entity_id === null) return note.unread.some((l) => l.entity_id === note.entity_id) ? "what else the write moved" : `what else the write to ${note.title} moved`;
  return line.title;
}

/**
 * The toast's words for `lines` of `note`: one sentence per source and ending, in the order first found, naming at
 * most one item and counting the rest.
 */
export function unreadWords(note: WriteUnread, lines: readonly Unread[], now = Date.now()): string {
  const groups = new Map<string, { ending: string; subjects: string[]; plural: boolean }>();
  for (const line of lines) {
    const ending = when(line, now);
    const key = `${line.source_id}\u0000${ending}`;
    const group = groups.get(key) ?? { ending, subjects: [], plural: false };
    const said = subject(line, note);
    if (!group.subjects.includes(said)) group.subjects.push(said);
    if (line.why === "left_out") group.plural = true;
    groups.set(key, group);
  }
  return [...groups.values()]
    .map(({ ending, subjects, plural }) => {
      const named = subjects.length <= 2 ? subjects.join(" and ") : `${subjects[0]} and ${subjects.length - 1} more`;
      const verb = plural || subjects.length > 1 ? "show" : "shows";
      return `${named.charAt(0).toUpperCase()}${named.slice(1)} ${verb} ${ending}.`;
    })
    .join(" ");
}

/** The bridge's `listen`, as the shell's share of it carries it. */
export type UnreadListen = (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;

export interface UnreadHow {
  listen?: UnreadListen;
  /** Starts a run of a source that syncs by hand: *Sync now* on the toast. */
  sync?: (source: string) => void;
  now?: () => number;
}

/** Say `note` in a toast, and take each line off it as it arrives; the toast goes with the last. Answers the stop. */
export function sayUnread(note: WriteUnread, how: UnreadHow = {}): () => void {
  const listen = how.listen ?? ((event, handler) => windowListen(event, handler));
  const now = how.now ?? Date.now;
  let left = [...note.unread];
  if (left.length === 0) return () => {};
  const byHand = left.find((line) => line.by_hand);
  const sync = how.sync ?? ((source: string) => void syncNow(source).catch(() => {}));
  const action = byHand === undefined ? undefined : { label: "Sync now", run: () => sync(byHand.source_id) };
  let dead = false;
  const offs: (() => void)[] = [];
  const stop = () => {
    dead = true;
    for (const off of offs.splice(0)) off();
  };
  const id = push({ text: unreadWords(note, left, now()), ms: NOTE_MS, subject: `unread:${note.write_id}`, onend: stop, ...(action ? { action } : {}) });
  /** The sources a run of which began after the toast: their run's end brings what they hold. */
  const began = new Set<string>();
  const arrived = (gone: (line: Unread) => boolean) => {
    const kept = left.filter((line) => !gone(line));
    if (kept.length === left.length || dead) return;
    left = kept;
    if (!toasts.items.some((toast) => toast.id === id)) return stop();
    if (left.length === 0) dismiss(id);
    else amend(id, { text: unreadWords(note, left, now()), ...(action && left.some((line) => line.by_hand) ? { action } : {}) });
  };
  const hear = (event: string, handler: (payload: unknown) => void) =>
    void listen(event, ({ payload }) => {
      if (!dead) handler(payload);
    })
      .then((off) => (dead ? off() : offs.push(off)))
      .catch(() => {});
  hear(EVENTS.itemRefreshed, (payload) => {
    const ended = payload as RefreshEnded;
    if (ended.ended !== "failed") arrived((line) => line.entity_id === ended.entity_id);
  });
  hear(EVENTS.syncState, (payload) => {
    const row = payload as SourceSyncStatus;
    if (row.running) {
      began.add(row.source_id);
      return;
    }
    if (row.last_outcome !== "ok") return;
    arrived((line) => line.source_id === row.source_id && (began.has(row.source_id) || line.why === "run_in_flight"));
  });
  return stop;
}

/** Say every `write:unread` for the life of the shell. Answers the teardown. */
export function startSayingUnread(how: UnreadHow = {}): () => void {
  const listen = how.listen ?? ((event, handler) => windowListen(event, handler));
  let dead = false;
  let off: (() => void) | undefined;
  // Each note stops hearing once its toast goes, so only the listener for new notes is the shell's to stop.
  void listen(EVENTS.writeUnread, ({ payload }) => {
    if (!dead) sayUnread(payload as WriteUnread, how);
  })
    .then((unlisten) => (dead ? unlisten() : (off = unlisten)))
    .catch(() => {});
  return () => {
    dead = true;
    off?.();
  };
}
