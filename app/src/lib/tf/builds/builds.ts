/**
 * The Builds list's rules (#848, R-9.1, R-9.4a): pure functions over the
 * rows `pipelines` answers, so every one is a test on plain data.
 *
 * Nothing here knows a source. A run is *failed* by its declared tone, *in
 * flight* by its declared instants (started and not finished, or neither),
 * and a pipeline is named by its declared name (ADR-0022).
 */
import type { PipelineRow, RunRow } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";
import { when } from "../tickets/tickets";

/** How many runs the strip under a pipeline's name shows (R-9.4a). */
export const STRIP = 12;

/** The list's two chips, in prototype-6's words (R-9.1). */
export const CHIPS = ["Failing", "Running"] as const;
export type Chip = (typeof CHIPS)[number];

/** Whether the pipeline's latest run failed: its name is drawn in err tone. */
export function isFailing(pipeline: PipelineRow): boolean {
  return pipeline.recent[0]?.state?.tone === "err";
}

/** Whether one of the pipeline's runs started and has not finished. */
export function isRunning(pipeline: PipelineRow): boolean {
  return pipeline.recent.some((run) => run.started !== null && run.finished === null);
}

/**
 * A run's block in the strip: finished, its declared tone; in flight,
 * running once it started and queued before.
 */
export function runTone(run: RunRow): "ok" | "err" | "warn" | "muted" | "run" | "q" {
  if (run.finished === null && run.started !== null) return "run";
  if (run.finished === null && run.started === null) return "q";
  const tone = run.state?.tone;
  return tone === "ok" || tone === "err" || tone === "warn" ? tone : "muted";
}

/** The last {@link STRIP} runs, oldest first, so the newest is on the right. */
export function stripOf(pipeline: PipelineRow): RunRow[] {
  return pipeline.recent.slice(0, STRIP).reverse();
}

/**
 * When the latest run started, or was last touched if it has not started,
 * as prototype-6's row writes it: *11:45* today, *yest. 16:20*, else the
 * date. Empty for a pipeline nothing ran.
 */
export function latestTime(pipeline: PipelineRow, now: Date = new Date()): string {
  const latest = pipeline.recent[0];
  const at = latest?.started ?? latest?.updated_at ?? null;
  return at === null ? "" : shortTime(at, now);
}

/** An instant as a row writes it: *11:45* today, *yest. 16:20*, else the date. */
export function shortTime(iso: string, now: Date = new Date()): string {
  return when(iso, now).replace("today ", "").replace("yesterday", "yest.");
}

/** A run's address: its pipeline's entity id, then its own (#853 draws it). */
export function runHash(pipelineId: string, runId: string): string {
  return `#/build/${encodeURIComponent(pipelineId)}/${encodeURIComponent(runId)}`;
}

/** The pipelines the filter's words and the chips keep, in the list's order. */
export function keptPipelines(list: readonly PipelineRow[], text: string, chips: readonly string[]): PipelineRow[] {
  return list.filter(
    (pipeline) =>
      matchesAll(`${pipeline.name} ${pipeline.group ?? ""}`, text) &&
      (!chips.includes("Failing") || isFailing(pipeline)) &&
      (!chips.includes("Running") || isRunning(pipeline)),
  );
}

/** A pipeline's address: the entity id, as every full-view address carries one. */
export function pipelineHash(entityId: string): string {
  return `#/builds/${encodeURIComponent(entityId)}`;
}

/**
 * The pipeline an address selects. A bare `#/builds` selects the first
 * failing one, else the first, as prototype-6 opens on a failure; an id
 * nothing carries selects none.
 */
export function pipelineOf(list: readonly PipelineRow[], rest: readonly string[]): PipelineRow | null {
  const id = rest[0];
  if (id !== undefined && id !== "") return list.find((pipeline) => pipeline.entity_id === id) ?? null;
  return list.find(isFailing) ?? list[0] ?? null;
}

/** A duration as prototype-6 writes one: *38 s*, *4 m 12 s*. */
export function duration(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  return `${Math.floor(seconds / 60)} m ${String(seconds % 60).padStart(2, "0")} s`;
}
