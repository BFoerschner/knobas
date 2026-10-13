/**
 * Whether a wait has outlasted {@link PLACEHOLDER_MS} (#746).
 *
 * The same rule as the detail panel's (#703): under the threshold a read is
 * waited on in silence, so a local answer does not flicker a line in and out;
 * over it, the line is honest news. What a caller must never do instead is
 * draw the *empty* state while the wait runs: silence is for the loading
 * line, not for a claim about the data.
 *
 * Call during component initialisation (it owns an `$effect`).
 */
import { PLACEHOLDER_MS } from "../detail/view-read";

export function patience(waiting: () => boolean): { readonly slow: boolean } {
  let slow = $state(false);
  $effect(() => {
    if (!waiting()) {
      slow = false;
      return;
    }
    const timer = setTimeout(() => (slow = true), PLACEHOLDER_MS);
    return () => clearTimeout(timer);
  });
  return {
    get slow() {
      return slow;
    },
  };
}
