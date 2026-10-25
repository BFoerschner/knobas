/**
 * The shell's **echo** (CONTEXT.md, #2102): what a page draws of a write it
 * queued this session (#1424, #1542), at once and where it lands: the job's
 * picked status and its comment, a Docs section's new text, comment and page,
 * a pull request's chips and its comment, a run and a parameter change on a
 * pipeline, a run's filed ticket, Standup's filed tickets, a monitor's
 * create, pause and resume, and the repository's branches and file commits
 * (#2103-#2105). Every surface's echo lives here.
 *
 * An echo is module `$state` by the entity it lands on, kind by kind, never
 * written anywhere: the write queue is the record, and a restart reads only
 * what the source has. Every echo keeps one lifecycle:
 *
 * - **queued** while its write is open (pending, or held as an unconfirmed
 *   send, which may have arrived, #1966), and **sent** once the write left
 *   the queue's open rows other than by withdrawal (`state`), or once a
 *   refresh names its write, which follows the write landed. It is never
 *   *queued* again after that.
 * - **gone when the surface's read shows the write** (`settleEchoes`): each
 *   page says, as its read arrives, which of its echoes the read holds -- a
 *   status that moved, a comment whose words are in the thread, a page
 *   version that moved. That check is the one thing a surface supplies. An
 *   echo dropped stays dropped, so a ticket moved back by somebody else does
 *   not draw an old move again.
 * - **gone when the queue says it will not go** (held or refused, or
 *   withdrawn in the queue panel, which drops the echo quietly:
 *   `noteWithdrawn`, #1583): a toast says why with *Edit and resend*
 *   (`shell/follow-write.ts`'s `followWrite`). The follow lives here, not on
 *   the page, so the toast comes and the echo goes wherever the reader is by
 *   then. A write held as unconfirmed keeps its echo until the reader sends it
 *   again or discards it, and its toast offers no *Edit and resend* (#1966).
 *
 * **When the read comes** (`watchEchoes`): a mounted surface re-reads when the
 * `item:refreshed` event (#2095, ADR-0031) names one of its echoes' writes or
 * the entity one lands on, and when a run of one of its echoes' sources ends.
 * A surface tags its echoes with its name (`surface`) and their `source`, and
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
export interface Echo {
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

/** How a write's echo is drawn. */
export interface Echoing {
  /** `submit_write`'s answer; its id keys the echo and is followed. One already `sent` starts *sent*. */
  write?: (Pick<QueuedWrite, "id"> & Partial<Pick<QueuedWrite, "state">>) | null;
  /** What the read said as the write was queued. */
  base?: string | null;
  /** Several of this kind live side by side (comments, pages); otherwise a new one replaces the old. */
  many?: boolean;
  /** Follow the write: held or refused drops the echo and toasts in these words (held as unconfirmed keeps it, #1966). */
  following?: Following;
  follower?: WriteFollower;
  /** The surface drawing it, for `watchEchoes` (#2102). */
  surface?: string;
  /** The source its write goes to, for `watchEchoes`'s run ends; `null` or none for any. */
  source?: string | null;
  /** Called once the follow sees the write sent, for a surface that reads at once what the write made (Standup's ticket, #2104). */
  onsent?: () => void;
  /** Called once the write will not go (held or refused), its echo dropped, for a surface whose read still names it (Monitors' create, #2105). */
  onstop?: () => void;
}

const echoes: Record<string, Echo[]> = $state({});
/** Edit and resend's requests, by entity and kind. */
const asked: Record<string, unknown> = $state({});
/** The follows this session started, to stop on a new one. */
const follows = new Set<() => void>();

const askKey = (entity: string, kind: string) => `${entity}\u0000${kind}`;

/** A write on `entity` queued, drawn as `text` in its `kind`'s place. */
export function echoWrite(entity: string, kind: string, text: string, how: Echoing = {}): void {
  const write = typeof how.write?.id === "number" ? how.write.id : null;
  const kept = (echoes[entity] ?? []).filter((echo) => how.many || echo.kind !== kind);
  const state = how.write?.state === "sent" ? "sent" : "queued";
  echoes[entity] = [...kept, { write, kind, text, base: how.base ?? null, state, surface: how.surface ?? null, source: how.source ?? null }];
  if (write === null) return;
  const gone = () => drop(entity, (echo) => echo.write === write);
  follows.add(onWithdrawn(write, gone));
  if (!how.following || state === "sent") return;
  // Left the open rows: sent, or withdrawn, which `onWithdrawn` has dropped already.
  const sent = () => {
    for (const echo of echoes[entity] ?? []) if (echo.write === write) echo.state = "sent";
    how.onsent?.();
  };
  const stopped = () => {
    gone();
    how.onstop?.();
  };
  follows.add(followWrite({ id: write }, { ...how.following, onstop: stopped, onleave: sent }, how.follower ?? writeFollower()));
}

function drop(entity: string, gone: (echo: Echo) => boolean): void {
  const had = echoes[entity];
  if (had === undefined) return;
  const kept = had.filter((echo) => !gone(echo));
  if (kept.length === had.length) return;
  if (kept.length === 0) delete echoes[entity];
  else echoes[entity] = kept;
}

/** The echoes on `entity`, of `kind` when given, oldest first. */
export function echoesOn(entity: string, kind?: string): Echo[] {
  const all = echoes[entity] ?? [];
  return kind === undefined ? all : all.filter((echo) => echo.kind === kind);
}

/** The newest echo of `kind` on `entity`, or `null`. */
export function echoOn(entity: string, kind: string): Echo | null {
  return echoesOn(entity, kind).at(-1) ?? null;
}

/**
 * The writes of `entity`'s echoes already *sent*: taken as a live read is
 * asked, they are the ones that read shows, for a write whose landing no
 * field reads (#2103).
 */
export function sentWrites(entity: string): Set<number> {
  return new Set((echoes[entity] ?? []).flatMap((echo) => (echo.state === "sent" && echo.write !== null ? [echo.write] : [])));
}

/** A read of `entity` arrived: drop each echo it shows `landed`. */
export function settleEchoes(entity: string, landed: (echo: Echo) => boolean): void {
  drop(entity, landed);
}

/** The bridge's `listen`, as a surface's ports carry it for `watchEchoes`. */
export type EchoListen = (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;

export interface EchoWatch {
  /** The bridge's `listen`; Tauri's when omitted. */
  listen?: EchoListen | undefined;
  /** Re-read on the end of every run of every source, for a surface that reads again on each anyway (Docs, #2072). */
  everyRun?: boolean;
  /** `false` for a surface that re-reads on its own source's run ends already (a pipeline, #1450): the refresh event only. */
  runEnds?: boolean;
}

/**
 * Call `reread` when what `surface`'s echoes wait for may have come: the
 * refresh event names one's write or its entity, or a run of one's source
 * ends (every run's end, with `everyRun`). The surface's own `settleEchoes`
 * check then says which echoes its read shows. Answers the stop, which also
 * stops a listen still being set up.
 */
export function watchEchoes(surface: string, reread: () => void, how: EchoWatch = {}): () => void {
  const listen = how.listen ?? ((event, handler) => tauriListen(event, handler));
  const mine = () => Object.entries(echoes).flatMap(([entity, all]) => all.filter((m) => m.surface === surface).map((m) => ({ entity, m })));
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
    // A refresh that followed a write follows it landed (ADR-0031): its echo is *sent*, whatever the follow has heard yet.
    if (ended.write_id !== null) for (const all of Object.values(echoes)) for (const echo of all) if (echo.write === ended.write_id) echo.state = "sent";
    if (mine().some(({ entity, m }) => entity === ended.entity_id || (m.write !== null && m.write === ended.write_id))) reread();
  });
  if (how.runEnds !== false)
    hear(EVENTS.syncState, (payload) => {
      const row = payload as SourceSyncStatus;
      if (row.running) return;
      if (how.everyRun || mine().some(({ m }) => m.source === null || m.source === row.source_id)) reread();
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

/** Forget every echo and request and stop every follow: a new session, which is what each test is. */
export function forgetEchoes(): void {
  for (const stop of follows) stop();
  follows.clear();
  for (const bag of [echoes, asked] as Record<string, unknown>[]) for (const key of Object.keys(bag)) delete bag[key];
}
