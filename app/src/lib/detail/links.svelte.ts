/**
 * The link write, and what to say when it refuses.
 *
 * Two surfaces draw a link — the *Link to…* dialog and the launcher's Tab
 * action chain — and the sentence they show for a refusal has to be the same
 * one, because it is the sentence that tells the reader nothing is broken.
 *
 * A `.svelte.ts` module because of {@link linkChanges}: `$state` is compiler
 * syntax, and only this suffix is compiled.
 */
import { createLink, ipcErrorMessage, isIpcError } from "../ipc";
import { push } from "../shell/toasts.svelte";

/**
 * What went wrong, in words a reader can act on.
 *
 * `conflict` is the one code that is not a fault: the pair is already linked
 * under that relation, which is a *state*, not a failure of the app. The
 * backend's own message names ids and a constraint, so it is replaced rather
 * than shown — a raw error for an ordinary outcome reads as a bug.
 *
 * Every other code keeps its message: `not_found` names the entity that has
 * not synced, and that is exactly what the reader needs.
 */
export function linkFailureMessage(rejection: unknown): string {
  if (isIpcError(rejection) && rejection.code === "conflict") {
    return "Already linked — this pair already carries that relation.";
  }
  return ipcErrorMessage(rejection);
}

/**
 * How many links have been drawn from **outside** a detail view.
 *
 * The launcher can link the entity a detail has open without that detail
 * knowing, and a panel that then kept saying "Nothing linked yet" would be
 * showing a state the app has already left. A counter rather than an event bus:
 * there is one fact here — *something changed* — and the only reader is a
 * slide-over that re-reads.
 *
 * The dialog does not bump it; it is inside the detail, which refreshes itself.
 */
export const linkChanges = $state({ count: 0 });

/**
 * Draw a manual `related` link between two entities, and acknowledge it.
 *
 * The one-keystroke path (#40's story 17): no relation, no note, no dialog.
 * The toast is the visible acknowledgement — the activity surfaces tick on
 * their own, because the command emits `activity:new`.
 *
 * Never rejects: this is invoked from a keyboard chain where the alternative
 * to a toast is an unhandled rejection nobody sees.
 */
export async function linkTo(fromId: string, toId: string, label: string): Promise<void> {
  try {
    await createLink(fromId, toId);
    linkChanges.count += 1;
    push({ text: `Linked ${label}` });
  } catch (rejection) {
    push({ text: linkFailureMessage(rejection), tone: "err" });
  }
}
