/**
 * The toast a queued write raises, from every page that queues one (#1438).
 *
 *     Queued: re-run Payout_IntegrationTests #1187.
 *     Queued: re-run Payout_IntegrationTests #1187. TeamCity sign-in is failing, so it waits there.
 *
 * with *Show queue*. The second sentence is R-15.1's: a write to a source whose
 * credential the reader has to fix says so, because it will not go until they
 * do. There is no "Sends with the next sync": `submit_write` queues and
 * flushes the source at once, so a write goes in a second, and pausing a
 * source's sync does not hold it back (#860–#862, ruled again on #1438).
 *
 * The credential is read from the shell's live health store, the one the
 * status bar draws, so the toast and the bar agree about which source fails.
 */
import type { AuthState, SourceSummary } from "../ipc/sources";
import { health, type Health } from "./health.svelte";
import { push } from "./toasts.svelte";
import { openWriteQueue } from "./write-queue.svelte";

/** The source a write goes to: its id, to read its credential, and its name, to say it. */
export type WriteSource = Pick<SourceSummary, "id" | "display_name">;

/**
 * The credential states the reader has to fix before the source takes a
 * write: rejected, or none stored. An unreachable server is retried on its own
 * and an untested credential is not a failing one (`isActionable`'s note).
 */
const SIGN_IN_FAILS: Partial<Record<AuthState, true>> = { unauthorized: true, missing_secret: true };

/** "Queued: <what>.", and the sign-in sentence when `failing` names the source whose sign-in fails. */
export function queuedText(what: string, failing: string | null): string {
  const queued = `Queued: ${what}.`;
  return failing === null ? queued : `${queued} ${failing} sign-in is failing, so it waits there.`;
}

/**
 * Raise the toast for a write just queued to `source`, in the button's words
 * (`what`, without a closing full stop). `null` for a page that cannot name
 * the source: the toast then says what was queued and nothing about sign-in.
 */
export function toastQueued(what: string, source: WriteSource | null, store: Pick<Health, "get"> = health): number {
  const state = source === null ? undefined : store.get(source.id)?.state;
  const failing = source !== null && state !== undefined && SIGN_IN_FAILS[state] === true;
  return push({
    text: queuedText(what, failing ? source.display_name || source.id : null),
    action: { label: "Show queue", run: openWriteQueue },
  });
}
