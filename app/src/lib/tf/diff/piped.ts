/**
 * Pipelines on diffs (#2270, spec #2234 step 6a, stories 68–70): a file whose
 * pattern has a default pipeline is diffed as its pipeline's output, both
 * sides run through the same steps, so two base64 blobs diff as the decoded,
 * pretty-printed JSON. The runs are the backend's (`entityFilePiped`, the
 * cache keeps a run at a whole sha); the diff of the two outputs is worked
 * out here (`lineDiff`).
 *
 * This module is the pure half: which pipeline a file has (the seam the
 * default-pipeline store fills, #2265), what a pipeline is called in the
 * diff's header, and the diff two runs make, or why they make none.
 */
import type { DiffFile, PipeRun, PipeStep } from "../../ipc/entity";
import { sized, stepName } from "../../render/pipeline";
import { lineDiff } from "../code/line-diff";
import type { DrawnFile } from "./diff";
import type { Side } from "./rendered";

/** A file's pipeline: its steps, the name it was saved under (`null`: none) and the pattern that chose it. */
export interface DiffPipeline {
  name: string | null;
  pattern: string | null;
  steps: PipeStep[];
}

/** Which pipeline a file's diff runs: its default pipeline by pattern, or `null` where it has none. */
export interface DiffPipelines {
  of(path: string): DiffPipeline | null;
}

/** No file has a pipeline. */
export const noPipelines: DiffPipelines = { of: () => null };

/** What the header calls a pipeline: its name, else its steps in order. */
export function pipelineLabel(pipeline: DiffPipeline): string {
  return pipeline.name ?? pipeline.steps.map(stepName).join(" → ");
}

/** The diff of the two outputs, or the side that gave none and why. */
export type PipedDiff = { kind: "diff"; file: DrawnFile } | { kind: "failed"; side: Side; why: string };

/** What a run gave as text, or why it gave none. */
function textOf(run: PipeRun): { text: string } | { why: string } {
  if (run.outcome === "failed") return { why: `step ${run.failure.index + 1} (${run.failure.name}): ${run.failure.error}` };
  if (run.output.kind === "bytes") return { why: `its output is ${sized(run.output.size)} of bytes, not text` };
  return { text: run.output.text };
}

/**
 * `file`'s diff as its pipeline's outputs: `old` and `new` are the runs over each side, `null` where the
 * file is not on that side. The diff keeps the file's path and label (comments, *Viewed*, the tree and
 * the language go by them) and carries no threads: their lines are the raw file's.
 */
export function pipedDiff(file: DrawnFile, old: PipeRun | null, nu: PipeRun | null): PipedDiff {
  const sides: Record<Side, string | null> = { old: null, new: null };
  for (const [side, run] of [["old", old], ["new", nu]] as const) {
    if (run === null) continue;
    const got = textOf(run);
    if ("why" in got) return { kind: "failed", side, why: got.why };
    sides[side] = got.text;
  }
  const diffed: DiffFile = lineDiff(file.path, sides.old ?? "", sides.new ?? "");
  const change: DiffFile["change"] =
    file.change === "binary" ? (sides.old === null ? "added" : sides.new === null ? "deleted" : "modified") : file.change;
  return { kind: "diff", file: { ...diffed, old_path: file.old_path, change, ...(file.label === undefined ? {} : { label: file.label }) } };
}
