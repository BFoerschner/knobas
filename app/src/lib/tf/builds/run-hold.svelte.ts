/**
 * A press that queues a run holds its button for about two seconds (F-338), in
 * the same place, saying what the write is doing: *Queued*, then *Sent*. A
 * second press in that time lands on it and does nothing, so one press is one
 * run however fast the hand. It lets go early if the write's echo goes, because
 * the run has been read or the queue held or refused it, and its toast said so.
 */
import { PressHold } from "../press-hold.svelte";
import { echoesOn } from "../queued.svelte";
import { QUEUED_RUN } from "./pipeline";

export class RunHold {
  #hold = new PressHold();
  #write = $state<number | null>(null);
  #pipeline: () => string;

  constructor(pipeline: () => string) {
    this.#pipeline = pipeline;
  }

  /** What the button says while it holds, `null` when it does not. */
  get word(): "Queued" | "Sent" | null {
    if (!this.#hold.held()) return null;
    if (this.#write === null) return "Queued";
    const echo = echoesOn(this.#pipeline(), QUEUED_RUN).find((one) => one.write === this.#write);
    if (echo === undefined) return null;
    return echo.state === "sent" ? "Sent" : "Queued";
  }

  /** Queue the run with `send`, which answers its write's id, or `null` when it was not queued. */
  async press(send: () => Promise<number | null>): Promise<void> {
    if (this.word !== null) return;
    this.#write = null;
    this.#hold.hold();
    const id = await send();
    if (id === null) this.#hold.release();
    else this.#write = id;
  }
}
