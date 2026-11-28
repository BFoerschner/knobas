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
 * Notes told while the toast stands join it, so a burst of writes is one toast. A source that syncs by hand offers
 * *Sync now* on the toast. The toast goes early once what it names arrives: a refresh
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

/**
 * "in about 4 min", "in about 2 h", "in about 3 days", or nothing for a time past or unknown. The steps are `ageLabel`'s
 * (`time/format.ts`), so a time ahead is spelled as an age is.
 */
function inAbout(at: string | null, now: number): string {
  if (at === null) return "";
  const minutes = Math.round((Date.parse(at) - now) / 60_000);
  if (!(minutes >= 1)) return "";
  if (minutes < 60) return `, in about ${minutes} min`;
  const hours = Math.round(minutes / 60);
  return hours < 48 ? `, in about ${hours} h` : `, in about ${Math.round(hours / 24)} days`;
}

/** The sentence's ending for one line: when what it names shows. */
function when(line: Unread, now: number): string {
  if (line.why === "run_in_flight") return `once the sync of ${line.source_name} under way ends`;
  if (line.why === "turned_off") return `${afterSync(line.source_name, line.by_hand)}, which is turned off`;
  return `${afterSync(line.source_name, line.by_hand)}${line.by_hand ? "" : inAbout(line.next_run_at, now)}`;
}

/** What one line names, in the sentence. */
function subject(line: Unread, note: WriteUnread): string {
  if (line.why === "left_out") return `${OWN}the other boards and filters with ${line.title}`;
  // Beside the write's own item, "the write" is plain; alone, it names what the write was to.
  if (line.entity_id === null) return note.unread.some((l) => l.entity_id === note.entity_id) ? `${OWN}what else the write moved` : `${OWN}what else the write to ${note.title} moved`;
  return line.title;
}

/** Marks a subject in the app's own words, the only kind a sentence's start may capitalise: a title keeps its case. */
const OWN = "\u0001";

/** `subjects` as the sentence's start: the app's own words take a capital, a title stays as the source wrote it. */
function opening(subjects: readonly string[]): string {
  const named = subjects.length <= 2 ? subjects.join(" and ") : `${subjects[0]} and ${subjects.length - 1} more`;
  const own = named.startsWith(OWN);
  const words = named.replaceAll(OWN, "");
  return own ? `${words.charAt(0).toUpperCase()}${words.slice(1)}` : words;
}

/** A note on the toast, with what is left of it and the sources a run of which began after it came. */
interface Held {
  note: WriteUnread;
  left: Unread[];
  began: Set<string>;
}

/**
 * The toast's words for `lines` of `note`: one sentence per source and ending, in the order first found, naming at
 * most one item and counting the rest.
 */
export function unreadWords(note: WriteUnread, lines: readonly Unread[], now = Date.now()): string {
  return wordsOf([{ note, left: lines }], now);
}

/** The words for every note on the toast: their lines grouped as one note's are, across notes. */
function wordsOf(held: readonly { note: WriteUnread; left: readonly Unread[] }[], now: number): string {
  const groups = new Map<string, { ending: string; subjects: string[]; plural: boolean }>();
  for (const { note, left } of held)
    for (const line of left) {
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
      const verb = plural || subjects.length > 1 ? "show" : "shows";
      return `${opening(subjects)} ${verb} ${ending}.`;
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

/**
 * The notes said, in **one toast**: a note told while the toast stands joins it, on a new time, so a burst of landed
 * writes (a queue drained once a source answers again, *Retry waiting writes*) is one toast and not one each. Each line
 * goes as it arrives, and the toast goes with the last. `say` adds a note; `stop` says no more.
 */
export function unreadNotes(how: UnreadHow = {}): { say: (note: WriteUnread) => void; stop: () => void } {
  const listen = how.listen ?? ((event, handler) => windowListen(event, handler));
  const now = how.now ?? Date.now;
  const sync = how.sync ?? ((source: string) => void syncNow(source).catch(() => {}));
  let held: Held[] = [];
  let id: number | null = null;
  let dead = false;
  // A toast's life is one generation: what its listeners hear, and what they answer, once it has gone is dropped.
  let generation = 0;
  const offs: (() => void)[] = [];
  // Set while a standing toast is raised again with a note joined, so its going ends nothing.
  let joining = false;
  const quiet = () => {
    generation += 1;
    held = [];
    id = null;
    for (const off of offs.splice(0)) off();
  };
  /** *Sync now* on the first source left that syncs by hand. */
  const action = () => {
    const line = held.flatMap((h) => h.left).find((l) => l.by_hand);
    return line === undefined ? {} : { action: { label: "Sync now", run: () => sync(line.source_id) } };
  };
  const arrived = (gone: (line: Unread, h: Held) => boolean) => {
    if (id === null) return;
    let changed = false;
    for (const h of held) {
      const kept = h.left.filter((line) => !gone(line, h));
      if (kept.length !== h.left.length) [h.left, changed] = [kept, true];
    }
    if (!changed) return;
    held = held.filter((h) => h.left.length > 0);
    if (held.length === 0) dismiss(id);
    else amend(id, { text: wordsOf(held, now()), ...action() });
  };
  const hear = (event: string, handler: (payload: unknown) => void) => {
    const life = generation;
    void listen(event, ({ payload }) => {
      if (life === generation) handler(payload);
    })
      .then((off) => (life === generation ? offs.push(off) : off()))
      .catch(() => {});
  };
  const hearArrivals = () => {
    hear(EVENTS.itemRefreshed, (payload) => {
      const ended = payload as RefreshEnded;
      if (ended.ended !== "failed") arrived((line) => line.entity_id === ended.entity_id);
    });
    hear(EVENTS.syncState, (payload) => {
      const row = payload as SourceSyncStatus;
      if (row.running) {
        for (const h of held) h.began.add(row.source_id);
        return;
      }
      if (row.last_outcome !== "ok") return;
      arrived((line, h) => line.source_id === row.source_id && (h.began.has(row.source_id) || line.why === "run_in_flight"));
    });
  };
  const say = (note: WriteUnread) => {
    if (dead || note.unread.length === 0) return;
    if (id === null || !toasts.items.some((toast) => toast.id === id)) {
      quiet();
      hearArrivals();
    } else {
      // Raised again holding every note, on a new time, as a `tally` is; no `subject`, since it holds several (the
      // removal rule).
      joining = true;
      dismiss(id);
      joining = false;
    }
    held.push({ note, left: [...note.unread], began: new Set() });
    id = push({ text: wordsOf(held, now()), ms: NOTE_MS, onend: () => joining || quiet(), ...action() });
  };
  return {
    say,
    stop: () => {
      dead = true;
    },
  };
}

/** Say every `write:unread` for the life of the shell. Answers the teardown; a toast standing then goes on its own. */
export function startSayingUnread(how: UnreadHow = {}): () => void {
  const listen = how.listen ?? ((event, handler) => windowListen(event, handler));
  const notes = unreadNotes(how);
  let dead = false;
  let off: (() => void) | undefined;
  void listen(EVENTS.writeUnread, ({ payload }) => {
    if (!dead) notes.say(payload as WriteUnread);
  })
    .then((unlisten) => (dead ? unlisten() : (off = unlisten)))
    .catch(() => {});
  return () => {
    dead = true;
    notes.stop();
    off?.();
  };
}
