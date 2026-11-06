/**
 * A press that queues a run holds its button for about two seconds (F-338), in
 * the same place, saying what the write is doing: *Queued*, then *Sent*. A
 * second press in that time lands on it and does nothing, so one press is one
 * run however fast the hand. It lets go early if the write's echo goes, because
 * the run has been read or the queue held or refused it, and its toast said so.
 */
import { echoesOn } from "../queued.svelte";
import { QUEUED_RUN } from "./pipeline";

export const RUN_HOLD_MS = 2000;

export class RunHold {
  #pressed = $state(false);
  #write = $state<number | null>(null);
  #timer: ReturnType<typeof setTimeout> | null = null;
  #pipeline: () => string;

  constructor(pipeline: () => string) {
    this.#pipeline = pipeline;
  }

  /** What the button says while it holds, `null` when it does not. */
  get word(): "Queued" | "Sent" | null {
    if (!this.#pressed) return null;
    if (this.#write === null) return "Queued";
    const echo = echoesOn(this.#pipeline(), QUEUED_RUN).find((one) => one.write === this.#write);
    if (echo === undefined) return null;
    return echo.state === "sent" ? "Sent" : "Queued";
  }

  /** Queue the run with `send`, which answers its write's id, or `null` when it was not queued. */
  async press(send: () => Promise<number | null>): Promise<void> {
    if (this.word !== null) return;
    this.#write = null;
    this.#pressed = true;
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.#release(), RUN_HOLD_MS);
    const id = await send();
    if (id === null) this.#release();
    else this.#write = id;
  }

  #release() {
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = null;
    this.#pressed = false;
    this.#write = null;
  }
}
