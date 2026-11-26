/**
 * **A press's hold** (design system, The press rule): after a press answers, what was pressed stands for about two
 * seconds and a second press lands on it and does nothing, so a double press is one press however fast the hand and
 * whichever way it came (F-338, F-150, F-351). A row's key says which of a page's presses it is. A page that has a way
 * back (*Undo*) leaves that outside the hold. A press that failed isn't held, so *Try again* can go at once.
 *
 * `RunHold` is this with the write's words on the button. A page reads `held` in its handler, and the button stays as it is.
 */
import { SvelteSet } from "svelte/reactivity";

export const PRESS_HOLD_MS = 2000;

export class PressHold {
  #held = new SvelteSet<string>();
  #timers = new Map<string, ReturnType<typeof setTimeout>>();

  /** Whether `key`'s press holds. */
  held(key = ""): boolean {
    return this.#held.has(key);
  }

  /** Hold `key` for `ms`, from now. */
  hold(key = "", ms = PRESS_HOLD_MS): void {
    this.release(key);
    this.#held.add(key);
    this.#timers.set(
      key,
      setTimeout(() => this.release(key), ms),
    );
  }

  /** Let go of `key` before its time. */
  release(key = ""): void {
    const timer = this.#timers.get(key);
    if (timer !== undefined) clearTimeout(timer);
    this.#timers.delete(key);
    this.#held.delete(key);
  }
}
