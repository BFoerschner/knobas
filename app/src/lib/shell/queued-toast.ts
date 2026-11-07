/**
 * The toast a queued write raises, from every page that queues one (#1438).
 *
 *     Queued: re-run Payout_IntegrationTests #1187.
 *     Queued: re-run Payout_IntegrationTests #1187. TeamCity sign-in is failing, so it waits there.
 *
 * with *Show queue*. The second sentence is R-15.1's: a write to a source whose
 * credential the reader has to fix says so, because it will not go until they
 * do. Nothing promises the next sync, as prototype-6 did: `submit_write`
 * queues and flushes the source at once, so a write goes in a second, and
 * pausing a source's sync does not hold it back (#860–#862, ruled again on
 * #1438).
 *
 * The credential is read from the shell's live health store, the one the
 * status bar draws, so the toast and the bar agree about which source fails.
 */
import type { SourceSummary } from "../ipc/sources";
import { health, signInFails, type Health } from "./health.svelte";
import type { QueuedWrite } from "../ipc/sources";
import { willNotGo, writeFollower, type WriteFollower } from "./follow-write";
import { amend, dismiss, push, toasts, type ToastAction } from "./toasts.svelte";
import { onWithdrawn, openWriteQueue } from "./write-queue.svelte";

/** The source a write goes to: its id, to read its credential, and its name, to say it. */
export type WriteSource = Pick<SourceSummary, "id" | "display_name">;

/** The name of `source` when its sign-in fails, else `null`. */
function failingName(source: WriteSource | null | undefined, store: Pick<Health, "get">): string | null {
  return source && signInFails(store.get(source.id)?.state) ? source.display_name || source.id : null;
}

/** "Queued: <what>.", and the sign-in sentence when `failing` names the source whose sign-in fails. */
function queuedText(what: string, failing: string | null): string {
  return withSignIn(`Queued: ${what}.`, failing);
}

/** `text`, and the sign-in sentence when `failing` names the source whose sign-in fails. */
function withSignIn(text: string, failing: string | null): string {
  return failing === null ? text : `${text} ${failing} sign-in is failing, so it waits there.`;
}

/** What a queued toast follows, and the stores behind it. Every one but a test's takes the shell's. */
export interface QueuedToastHow {
  /** `submit_write`'s answer: the toast says *Sent* in place once this write has gone. */
  write?: (Pick<QueuedWrite, "id"> & Partial<Pick<QueuedWrite, "state">>) | null | undefined;
  follower?: WriteFollower | undefined;
  store?: Pick<Health, "get"> | undefined;
  /** Where the write's effect shows, offered once it has gone: *Show in runs*. Without it, a sent toast offers nothing. */
  where?: ToastAction | undefined;
  /** What the write is about: a toast with the same subject takes the last one's place (`ToastSpec.subject`). */
  subject?: string | undefined;
}

/** A write's follow, by the toast it says *Sent* on: a toast that took its place stops the older write's follow. */
const following = new Map<number, () => void>();

/** A review of `entity`: added to, then submitted, in one toast (F-260). */
export function reviewSubject(entity: string): string {
  return `review:${entity}`;
}

/**
 * Raise the toast for a write just queued to `source`, in the button's words
 * (`what`, without a closing full stop). `null` or `undefined` for a page
 * that cannot name the source (a detail read before it carried one): the
 * toast then says what was queued and nothing about sign-in. Returns the toast's id.
 *
 * Given the write, the toast follows it. A write goes within a second, so a toast left on *Queued* with *Show queue*
 * offered a queue that no longer held it. Withdrawn in the queue the toast goes, because nothing was sent. Held or
 * refused it stays as it is, because the write is still in the queue it offers and the page's own follow says why.
 */
export function toastQueued(what: string, source: WriteSource | null | undefined, how: QueuedToastHow = {}): number {
  const failing = failingName(source, how.store ?? health);
  const id = push({
    text: queuedText(what, failing),
    action: { label: "Show queue", run: openWriteQueue },
    ...(how.subject === undefined ? {} : { subject: how.subject }),
  });
  if (how.subject !== undefined) following.get(id)?.();
  const write = how.write;
  if (write === null || write === undefined) return id;
  if (write.state === "sent") {
    toastSent(id, what, how.where);
    return id;
  }
  let stopFollow: () => void = () => {};
  let stopWithdrawn: () => void = () => {};
  let over = false;
  const finish = () => {
    over = true;
    stopFollow();
    stopWithdrawn();
    if (following.get(id) === finish) following.delete(id);
  };
  if (how.subject !== undefined) following.set(id, finish);
  stopWithdrawn = onWithdrawn(write.id, () => {
    finish();
    dismiss(id);
  });
  stopFollow = (how.follower ?? writeFollower()).follow(write.id, (row) => {
    if (over) return;
    // Its toast has gone, so there is nothing left to say it on.
    if (!toasts.items.some((toast) => toast.id === id)) return finish();
    if (row === undefined) {
      finish();
      toastSent(id, what, how.where);
    } else if (willNotGo(row)) finish();
  });
  if (over) stopFollow();
  return id;
}

/** The write a toast from {@link toastQueued} named has gone: it says so where it stands, and offers where it shows rather than a queue. */
function toastSent(id: number, what: string, where: ToastAction | undefined): void {
  amend(id, where === undefined ? { text: `Sent: ${what}.` } : { text: `Sent: ${what}.`, action: where });
}

/**
 * The toast *Run all steps* raises on the Start work stepper (#1426), in
 * prototype-6's words: "Started work on PAY-240. 3 writes queued." with
 * *Show queue*, "1 write queued." for one, and no count when nothing was
 * queued (every step skipped or already there). The sign-in sentence follows
 * the count, as on every other queued write, and only when something waits.
 * `replaced` is the job this one takes over from, which the toast names (F-130):
 * "Started work on PAY-240, in place of PAY-231." `timing`: the same press starts
 * the timer, so one line ends "Timer running." rather than a second toast (F-145).
 */
export function toastStarted(key: string, queued: number, source: WriteSource | null | undefined, store: Pick<Health, "get"> = health, replaced: string | null = null, timing = false): void {
  const started = replaced === null ? `Started work on ${key}.` : `Started work on ${key}, in place of ${replaced}.`;
  const running = timing ? " Timer running." : "";
  if (queued <= 0) {
    push({ text: `${started}${running}` });
    return;
  }
  const failing = failingName(source, store);
  const count = `${started} ${queued === 1 ? "1 write" : `${queued} writes`} queued.`;
  push({
    text: `${withSignIn(count, failing)}${running}`,
    action: { label: "Show queue", run: openWriteQueue },
  });
}
