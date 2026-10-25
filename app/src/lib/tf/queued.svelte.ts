/**
 * The shell's **echo** (CONTEXT.md, #2102): what a ticket-first page draws of
 * a write it queued this session (#1424, #1542), at once and where it lands:
 * the job's picked status and its comment, a Docs section's new text,
 * comment and page, a pull request's chips and its comment, a run queued on a
 * pipeline from its page or a run page (#1449). The repository page keeps its
 * own (`repo/queued.svelte.ts`). The code still says *mark* for one echo.
 *
 * A mark is module `$state` by the entity it lands on, kind by kind, never
 * written anywhere: the write queue is the record, and a restart reads only
 * what the source has. Every echo keeps one lifecycle:
 *
 * - **queued** while its write is open (pending, or held as an unconfirmed
 *   send, which may have arrived, #1966), and **sent** once the write left
 *   the queue's open rows other than by withdrawal (`state`). It is never
 *   *queued* again after that.
 * - **gone when the surface's read shows the write** (`settleQueued`): each
 *   page says, as its read arrives, which of its marks the read holds -- a
 *   status that moved, a comment whose words are in the thread, a page
 *   version that moved. That check is the one thing a surface supplies. A
 *   mark dropped stays dropped, so a ticket moved back by somebody else does
 *   not draw an old move again.
 * - **gone when the queue says it will not go** (held or refused, or
 *   withdrawn in the queue panel, which drops the mark quietly:
 *   `noteWithdrawn`, #1583): a toast says why with *Edit and resend*
 *   (`shell/follow-write.ts`'s `followWrite`). The follow lives here, not on
 *   the page, so the toast comes and the echo goes wherever the reader is by
 *   then. A write held as unconfirmed keeps its mark until the reader sends it
 *   again or discards it, and its toast offers no *Edit and resend* (#1966).
 *
 * **When the read comes** (`watchEchoes`): a mounted surface re-reads when the
 * `item:refreshed` event (#2095, ADR-0031) names one of its echoes' writes or
 * the entity one lands on, and when a run of one of its echoes' sources ends.
 * A surface tags its marks with its name (`surface`) and their `source`, and
 * hands `watchEchoes` its re-read; the event follows a write that went from
 * any send path, so the echo settles without the page being opened again.
 *
 * *Edit and resend* reopens a page that may have closed: the toast asks
 * (`askAgain`), navigates, and the page takes the request once it draws
 * (`takeAgain`).
 */
import type { QueuedWrite } from "../ipc/sources";
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import type { RefreshEnded, SourceSyncStatus } from "../ipc/sources";
import { onWithdrawn } from "../shell/write-queue.svelte";
import { followWrite, writeFollower, type Following, type WriteFollower } from "../shell/follow-write";

/** One queued write as its page draws it. */
export interface Mark {
  /** The queued write's id, `null` where the answer carried none: what a held or refused write drops it by. */
  write: number | null;
  /** The place it draws in, in the page's word: `status`, `comment`, `close`, `section 2`, `page`. */
  kind: string;
  /** What it draws: the picked status, the comment's words, the section's text, the page's title, the chip. */
  text: string;
  /** What the read said when the write was queued, for a page whose read moving is the landing. */
  base: string | null;
  /** Where its write is: `queued` while open, `sent` once it left the queue's open rows as sent (#2102). */
  state: "queued" | "sent";
  /** The surface that drew it, whose `watchEchoes` re-reads for it; `null` for none. */
  surface: string | null;
  /** The source its write goes to, whose run's end re-reads its surface; `null` for any. */
  source: string | null;
}

/** How a write is marked. */
export interface Marking {
  /** `submit_write`'s answer; its id keys the mark and is followed. One already `sent` starts *sent*. */
  write?: (Pick<QueuedWrite, "id"> & Partial<Pick<QueuedWrite, "state">>) | null;
  /** What the read said as the write was queued. */
  base?: string | null;
  /** Several of this kind live side by side (comments, pages); otherwise a new one replaces the old. */
  many?: boolean;
  /** Follow the write: held or refused drops the mark and toasts in these words (held as unconfirmed keeps it, #1966). */
  following?: Following;
  follower?: WriteFollower;
  /** The surface drawing it, for `watchEchoes` (#2102). */
  surface?: string;
  /** The source its write goes to, for `watchEchoes`'s run ends. */
  source?: string;
}

const marks: Record<string, Mark[]> = $state({});
/** Edit and resend's requests, by entity and kind. */
const asked: Record<string, unknown> = $state({});
/** The follows this session started, to stop on a new one. */
const follows = new Set<() => void>();

const askKey = (entity: string, kind: string) => `${entity}\u0000${kind}`;

/** A write on `entity` queued, drawn as `text` in its `kind`'s place. */
export function markQueued(entity: string, kind: string, text: string, how: Marking = {}): void {
  const write = typeof how.write?.id === "number" ? how.write.id : null;
  const kept = (marks[entity] ?? []).filter((mark) => how.many || mark.kind !== kind);
  const state = how.write?.state === "sent" ? "sent" : "queued";
  marks[entity] = [...kept, { write, kind, text, base: how.base ?? null, state, surface: how.surface ?? null, source: how.source ?? null }];
  if (write === null) return;
  const gone = () => drop(entity, (mark) => mark.write === write);
  follows.add(onWithdrawn(write, gone));
  if (!how.following || state === "sent") return;
  // Left the open rows: sent, or withdrawn, which `onWithdrawn` has dropped already.
  const sent = () => {
    for (const mark of marks[entity] ?? []) if (mark.write === write) mark.state = "sent";
  };
  follows.add(followWrite({ id: write }, { ...how.following, onstop: gone, onleave: sent }, how.follower ?? writeFollower()));
}

function drop(entity: string, gone: (mark: Mark) => boolean): void {
  const had = marks[entity];
  if (had === undefined) return;
  const kept = had.filter((mark) => !gone(mark));
  if (kept.length === had.length) return;
  if (kept.length === 0) delete marks[entity];
  else marks[entity] = kept;
}

/** The marks on `entity`, of `kind` when given, oldest first. */
export function queuedMarks(entity: string, kind?: string): Mark[] {
  const all = marks[entity] ?? [];
  return kind === undefined ? all : all.filter((mark) => mark.kind === kind);
}

/** The newest mark of `kind` on `entity`, or `null`. */
export function queuedMark(entity: string, kind: string): Mark | null {
  return queuedMarks(entity, kind).at(-1) ?? null;
}

/** A read of `entity` arrived: drop each mark it shows `landed`. */
export function settleQueued(entity: string, landed: (mark: Mark) => boolean): void {
  drop(entity, landed);
}

export interface EchoWatch {
  /** The bridge's `listen`; Tauri's when omitted. */
  listen?: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
  /** Re-read on the end of every run of every source, for a surface that reads again on each anyway (Docs, #2072). */
  everyRun?: boolean;
}

/**
 * Call `reread` when what `surface`'s echoes wait for may have come: the
 * refresh event names one's write or its entity, or a run of one's source
 * ends (every run's end, with `everyRun`). The surface's own `settleQueued`
 * check then says which echoes its read shows. Answers the stop, which also
 * stops a listen still being set up.
 */
export function watchEchoes(surface: string, reread: () => void, how: EchoWatch = {}): () => void {
  const listen = how.listen ?? ((event, handler) => tauriListen(event, handler));
  const echoes = () => Object.entries(marks).flatMap(([entity, all]) => all.filter((m) => m.surface === surface).map((m) => ({ entity, m })));
  let dead = false;
  const offs: (() => void)[] = [];
  const hear = (event: string, handler: (payload: unknown) => void) =>
    void listen(event, ({ payload }) => {
      if (!dead) handler(payload);
    })
      .then((off) => (dead ? off() : offs.push(off)))
      .catch(() => {});
  hear(EVENTS.itemRefreshed, (payload) => {
    const ended = payload as RefreshEnded;
    if (echoes().some(({ entity, m }) => entity === ended.entity_id || (m.write !== null && m.write === ended.write_id))) reread();
  });
  hear(EVENTS.syncState, (payload) => {
    const row = payload as SourceSyncStatus;
    if (row.running) return;
    if (how.everyRun || echoes().some(({ m }) => m.source === null || m.source === row.source_id)) reread();
  });
  return () => {
    dead = true;
    for (const off of offs.splice(0)) off();
  };
}

/** Whether a read's words are a queued comment's: the same words, whatever the source did to the spaces. */
export function sameWords(read: string, queued: string): boolean {
  const words = (text: string) => text.replace(/\s+/g, " ").trim();
  return words(read) === words(queued);
}

/** *Edit and resend*: ask the page of `entity` to reopen its `kind` with `value`. */
export function askAgain(entity: string, kind: string, value: unknown): void {
  asked[askKey(entity, kind)] = value;
}

/** What *Edit and resend* asked of `entity`'s `kind`, once; `null` when nothing was. */
export function takeAgain<T = string>(entity: string, kind: string): T | null {
  const key = askKey(entity, kind);
  if (!(key in asked)) return null;
  const value = asked[key] as T;
  delete asked[key];
  return value;
}

/** Forget every mark and request and stop every follow: a new session, which is what each test is. */
export function forgetQueued(): void {
  for (const stop of follows) stop();
  follows.clear();
  for (const bag of [marks, asked] as Record<string, unknown>[]) for (const key of Object.keys(bag)) delete bag[key];
}
