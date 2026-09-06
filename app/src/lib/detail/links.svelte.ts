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
  await draw(fromId, toId, `Linked ${label}`, null);
}

/**
 * The write both one-keystroke link paths make, and the two sentences that
 * differ between them.
 *
 * Private, because what a caller chooses is *what to say*, never how to write:
 * the counter, the toast and the never-rejects contract are the same for both,
 * and two copies of them would be two chances for a refusal to surface in one
 * surface and not the other. `conflict` is the only code either of them treats
 * as something other than a failure, so it is the only one parameterised —
 * `null` means "no special sentence", which is `linkTo`'s answer: a pair
 * already linked under a relation is a state worth naming, and
 * {@link linkFailureMessage} already names it.
 */
async function draw(
  fromId: string,
  toId: string,
  done: string,
  onConflict: string | null,
): Promise<void> {
  try {
    await createLink(fromId, toId);
    linkChanges.count += 1;
    push({ text: done });
  } catch (rejection) {
    if (onConflict !== null && isIpcError(rejection) && rejection.code === "conflict") {
      push({ text: onConflict });
      return;
    }
    push({ text: linkFailureMessage(rejection), tone: "err" });
  }
}

/**
 * Put an entity in a stored context, and acknowledge it — the launcher's
 * *Add to context* (#436, spec §4's action chain).
 *
 * **Not a command of its own.** ADR-0008 states the membership rule and says
 * so in as many words: *"the seed is the explicit adds (every confirmed link
 * touching the context's own `ctx:` entity — an* Add to context *is an
 * ordinary link)"*. So this is {@link linkTo}'s write with the context's
 * entity at one end — literally, through the same private `draw` — and the
 * reason it is a second function rather than a second call site is the two
 * sentences it says: what a reader is told when it lands, and what they are
 * told when the entity is already in the room.
 *
 * `conflict` is that second sentence. From `createLink` it means *this pair is
 * already linked*, and for a context that is not a failure at all — it is the
 * state the reader was asking for. "Already linked — this pair already carries
 * that relation" is true and describes a link nobody drew on purpose; *already
 * in Payments* is what happened.
 *
 * Never rejects, for {@link linkTo}'s reason: the caller is a keyboard chain
 * where the alternative to a toast is an unhandled rejection nobody sees.
 */
export async function addToContext(
  ctxId: string,
  targetId: string,
  label: string,
  contextLabel: string,
): Promise<void> {
  await draw(
    ctxId,
    targetId,
    `Added ${label} to ${contextLabel}`,
    `${label} is already in ${contextLabel}`,
  );
}
