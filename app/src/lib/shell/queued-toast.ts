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
import { push } from "./toasts.svelte";
import { openWriteQueue } from "./write-queue.svelte";

/** The source a write goes to: its id, to read its credential, and its name, to say it. */
export type WriteSource = Pick<SourceSummary, "id" | "display_name">;

/** "Queued: <what>.", and the sign-in sentence when `failing` names the source whose sign-in fails. */
function queuedText(what: string, failing: string | null): string {
  const queued = `Queued: ${what}.`;
  return failing === null ? queued : `${queued} ${failing} sign-in is failing, so it waits there.`;
}

/**
 * Raise the toast for a write just queued to `source`, in the button's words
 * (`what`, without a closing full stop). `null` or `undefined` for a page
 * that cannot name the source (a detail read before it carried one): the
 * toast then says what was queued and nothing about sign-in.
 */
export function toastQueued(what: string, source: WriteSource | null | undefined, store: Pick<Health, "get"> = health): void {
  const failing = source && signInFails(store.get(source.id)?.state) ? source : null;
  push({
    text: queuedText(what, failing === null ? null : failing.display_name || failing.id),
    action: { label: "Show queue", run: openWriteQueue },
  });
}

/**
 * The toast *Run all steps* raises on the Start work stepper (#1426), in
 * prototype-6's words: "Started work on PAY-240. 3 writes queued." with
 * *Show queue*, "1 write queued." for one, and no count when nothing was
 * queued (every step skipped or already there). The sign-in sentence follows
 * the count, as on every other queued write, and only when something waits.
 */
export function toastStarted(key: string, queued: number, source: WriteSource | null | undefined, store: Pick<Health, "get"> = health): void {
  const started = `Started work on ${key}.`;
  if (queued <= 0) {
    push({ text: started });
    return;
  }
  const failing = source && signInFails(store.get(source.id)?.state) ? source : null;
  const count = `${started} ${queued === 1 ? "1 write" : `${queued} writes`} queued.`;
  push({
    text: failing === null ? count : `${count} ${failing.display_name || failing.id} sign-in is failing, so it waits there.`,
    action: { label: "Show queue", run: openWriteQueue },
  });
}
