/**
 * What to say when a link write refuses.
 *
 * The *Link to…* dialog and the job page's link picker both show the
 * sentence, and it has to be the same one, because it is the sentence that
 * tells the reader nothing is broken.
 */
import { ipcErrorMessage, isIpcError } from "../ipc";

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
