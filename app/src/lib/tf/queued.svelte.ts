/**
 * What a ticket-first page queued this session (#1424, #1542), so a write is
 * drawn at once where it lands: the job's picked status and its comment, a
 * Docs section's new text, comment and page, a pull request's chips and its
 * comment, a run queued on a pipeline from its page or a run page (#1449).
 * The repository page keeps its own (`repo/queued.svelte.ts`).
 *
 * A mark is module `$state` by the entity it lands on, kind by kind, never
 * written anywhere: the write queue is the record, and a restart reads only
 * what the source has. It goes in one of two ways:
 *
 * - **The source's read shows it** (`settleQueued`): each page says, as its
 *   read arrives, which of its marks the read holds -- a status that moved, a
 *   comment whose words are in the thread, a page version that moved. A mark
 *   dropped stays dropped, so a ticket moved back by somebody else does not
 *   draw an old move again.
 * - **The queue says it will not go** (held or refused, or withdrawn in the
 *   queue panel, which drops the mark quietly: `noteWithdrawn`, #1583): the mark goes, and a
 *   toast says why with *Edit and resend* (`shell/follow-write.ts`'s
 *   `followWrite`). The follow lives here, not on the page, so the toast
 *   comes and the echo goes wherever the reader is by then. A write held as
 *   unconfirmed (sent, never answered) may have arrived, so its mark stays
 *   until the reader sends it again or discards it, and its toast offers no
 *   *Edit and resend* (#1966).
 *
 * *Edit and resend* reopens a page that may have closed: the toast asks
 * (`askAgain`), navigates, and the page takes the request once it draws
 * (`takeAgain`).
 */
import type { QueuedWrite } from "../ipc/sources";
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
}

/** How a write is marked. */
export interface Marking {
  /** `submit_write`'s answer; its id keys the mark and is followed. */
  write?: Pick<QueuedWrite, "id"> | null;
  /** What the read said as the write was queued. */
  base?: string | null;
  /** Several of this kind live side by side (comments, pages); otherwise a new one replaces the old. */
  many?: boolean;
  /** Follow the write: held or refused drops the mark and toasts in these words (held as unconfirmed keeps it, #1966). */
  following?: Following;
  follower?: WriteFollower;
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
  marks[entity] = [...kept, { write, kind, text, base: how.base ?? null }];
  if (write === null) return;
  const gone = () => drop(entity, (mark) => mark.write === write);
  follows.add(onWithdrawn(write, gone));
  if (!how.following) return;
  follows.add(followWrite({ id: write }, { ...how.following, onstop: gone }, how.follower ?? writeFollower()));
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
