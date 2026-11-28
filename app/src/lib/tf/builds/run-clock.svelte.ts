/**
 * The clock a run in flight counts up by (#2574, the mockup's *A run in flight*): one interval for the window, ticking
 * each second while a row reads it and stopped when none does (`createSubscriber`), so a page with no run in flight
 * keeps no timer.
 */
import { createSubscriber } from "svelte/reactivity";

/** How often a running run's clock moves on. */
export const RUN_CLOCK_MS = 1_000;

class RunClock {
  #subscribe = createSubscriber((update) => {
    const timer = setInterval(update, RUN_CLOCK_MS);
    return () => clearInterval(timer);
  });

  /** Now, in ms: read in an effect or a template, it is read again each second while anything reads it. */
  get now(): number {
    this.#subscribe();
    return Date.now();
  }
}

export const runClock = new RunClock();

/** The whole seconds a run started at `started` has run at `now`, never below 0. */
export function elapsedSeconds(started: string, now: number): number {
  const at = Date.parse(started);
  return Number.isNaN(at) ? 0 : Math.max(0, Math.floor((now - at) / 1000));
}
