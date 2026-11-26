/**
 * **A run in flight moves on where it is drawn** (the TeamCity run fix, 2026-11-25,
 * `docs/decisions/2026-11-25-a-started-run-moves-on-at-once.md`, Fable's ruling).
 *
 * Björn: "Team city isn't syncing a started run immediately after it's started." A run started in knobas is read to
 * its end by the write queue (`follow_run.rs`), each read that moved it ending on `item:refreshed` with no write behind
 * it. The pages that draw runs read the mirror again on that event ({@link hearRuns}), and refresh the runs they draw
 * in flight when they are opened, shown again or the window comes back ({@link InFlightReads}), so a run started
 * elsewhere or before a restart moves too. Neither discovers a new run: the source's runs do.
 */
import { EVENTS } from "../../ipc";
import { cacheEvents } from "../../ipc/answer-cache";
import type { RunRow } from "../../ipc/entity";
import type { RefreshEnded } from "../../ipc/sources";
import type { EchoListen } from "../queued.svelte";
import { isStartedHere } from "./pipeline";

/** How many runs in flight a page refreshes when shown: the newest. */
export const IN_FLIGHT_READS = 5;
/** How long after a page's last refresh of its runs in flight a return reads them again. */
export const IN_FLIGHT_AGAIN_MS = 5_000;
/** How long after a page opens it refreshes its runs in flight: once its landing has drawn, behind its reads. */
export const IN_FLIGHT_AFTER_MS = 100;

/** Whether a run is in flight: queued or running, not finished. A row queued here and not read back yet is not a run. */
export function inFlight(run: Pick<RunRow, "entity_id" | "finished">): boolean {
  return run.finished === null && !isStartedHere(run as RunRow);
}

/** The newest {@link IN_FLIGHT_READS} runs of `rows` (newest first) in flight. */
export function runsInFlight(rows: readonly RunRow[] | null): string[] {
  return (rows ?? []).filter(inFlight).slice(0, IN_FLIGHT_READS).map((run) => run.entity_id);
}

/** Where a run stands, as a page draws it: what a full read again waits on. */
export function phase(run: Pick<RunRow, "started" | "finished"> | null): "none" | "queued" | "running" | "finished" {
  if (run === null) return "none";
  if (run.finished !== null) return "finished";
  return run.started !== null ? "running" : "queued";
}

/**
 * Call `heard(entity)` when a refresh with no write behind it moved an item `mine` says the page draws: a followed
 * run's read, or a page's own. A failed refresh moved nothing. Answers the stop.
 */
export function hearRuns(listen: EchoListen, mine: (entity: string) => boolean, heard: (entity: string) => void): () => void {
  let dead = false;
  let off: (() => void) | null = null;
  void listen(EVENTS.itemRefreshed, ({ payload }) => {
    const ended = payload as RefreshEnded;
    if (dead || ended.write_id !== null || ended.ended === "failed" || !mine(ended.entity_id)) return;
    // The answers the refresh moved go before the read again: the window's cache hears the event on a listener of its
    // own, in no set order with this one, and a read asked first was answered the kept *running* (the real app on a
    // real TeamCity, 2026-11-25). Dropping twice is dropping once.
    cacheEvents.refreshed(ended);
    heard(ended.entity_id);
  })
    .then((stop) => (dead ? stop() : (off = stop)))
    .catch(() => {});
  return () => {
    dead = true;
    off?.();
  };
}

/**
 * A page's refresh of the runs it draws in flight, by id, in the background lane: at most once in
 * {@link IN_FLIGHT_AGAIN_MS}. A refresh that moved a run ends on `item:refreshed`, which {@link hearRuns} hears.
 */
export class InFlightReads {
  #at = 0;
  constructor(private readonly ask: ((runId: string) => Promise<unknown>) | undefined) {}

  /** Refresh `runs` now, unless the last refresh was under {@link IN_FLIGHT_AGAIN_MS} ago or there are none. */
  read(runs: readonly string[]): void {
    const ask = this.ask;
    if (ask === undefined || runs.length === 0 || Date.now() - this.#at < IN_FLIGHT_AGAIN_MS) return;
    this.#at = Date.now();
    // Asked from a promise, so a bridge that throws as it is called is a failed refresh, never the caller's.
    for (const run of runs) void Promise.resolve(run).then(ask).catch(() => {});
  }

  /** A new page: the next {@link read} goes at once. */
  reset(): void {
    this.#at = 0;
  }
}
