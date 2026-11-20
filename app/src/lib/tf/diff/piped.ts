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
import { lineDiff } from "./line-diff";
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

let installed: DiffPipelines = noPipelines;

/**
 * The app's lookup, what every diff asks by default: the default-pipeline store installs itself here
 * ({@link installPipelines}; named pipelines as file-pattern defaults are #2265's). Until one is
 * installed no file has a default pipeline.
 */
export const defaultPipelines: DiffPipelines = { of: (path) => installed.of(path) };

/** Makes `lookup` the app's default pipeline per file (`null` for a file with none). */
export function installPipelines(lookup: DiffPipelines): void {
  installed = lookup;
}

/** What the header calls a pipeline: its name, else its steps in order. */
export function pipelineLabel(pipeline: DiffPipeline): string {
  return pipeline.name ?? pipeline.steps.map((step) => stepName(step)).join(" → ");
}

/**
 * The diff of the two outputs, or the side that gave none and why, worded to follow "failed on the file before the change, ".
 * `sides` is each output as it came, text or bytes, `null` where the file is not on that side: a rendered card draws
 * them with no read (ux26, ruling 4's pipeline row).
 */
export type PipedDiff =
  | { kind: "diff"; file: DrawnFile; name: string; sides: Record<Side, string | Uint8Array | null> }
  | { kind: "failed"; side: Side; why: string };

/** What a run gave as text, or why it gave none. */
function textOf(run: PipeRun, drawsBytes: (name: string) => boolean): { text: string } | { bytes: Uint8Array } | { why: string } {
  if (run.outcome === "failed") return { why: `at step ${run.failure.index + 1} (${run.failure.name}): ${run.failure.error}` };
  if (run.output.kind === "bytes") {
    // Bytes a renderer draws by the output's name (a gunzipped picture): drawn rendered, with no lines (ux26).
    if (drawsBytes(run.name)) return { bytes: Uint8Array.from(atob(run.output.base64), (c) => c.charCodeAt(0)) };
    return { why: `where its output is ${sized(run.output.size)} of bytes, not text` };
  }
  // A last line's newline ends that line, as git reads a file; it is not one more empty line.
  return { text: run.output.text.endsWith("\n") ? run.output.text.slice(0, -1) : run.output.text };
}

/**
 * `file`'s diff as its pipeline's outputs: `old` and `new` are the runs over each side, `null` where the
 * file is not on that side. The diff keeps the file's path and label (*Viewed*, the tree and the
 * pending comments go by them) and carries no threads: their lines are the raw file's. `name` is the output's file
 * name, which picks the language the two sides are highlighted in. `drawsBytes` says whether a renderer draws an
 * output of bytes by its name: such outputs are a binary diff, drawn rendered; others fail as before.
 */
export function pipedDiff(file: DrawnFile, old: PipeRun | null, nu: PipeRun | null, drawsBytes: (name: string) => boolean = () => false): PipedDiff {
  const sides: Record<Side, string | Uint8Array | null> = { old: null, new: null };
  let name = file.path;
  for (const [side, run] of [["old", old], ["new", nu]] as const) {
    if (run === null) continue;
    const got = textOf(run, drawsBytes);
    if ("why" in got) return { kind: "failed", side, why: got.why };
    sides[side] = "text" in got ? got.text : got.bytes;
    if (run.outcome === "done" && (side === "new" || nu === null)) name = run.name;
  }
  const label = file.label === undefined ? {} : { label: file.label };
  if (sides.old instanceof Uint8Array || sides.new instanceof Uint8Array) {
    // Bytes on a side: no lines to diff, a binary file drawn rendered.
    return { kind: "diff", name, sides, file: { path: file.path, old_path: file.old_path, change: "binary", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [], ...label } };
  }
  const diffed: DiffFile = lineDiff(file.path, (sides.old as string | null) ?? "", (sides.new as string | null) ?? "");
  const change: DiffFile["change"] =
    file.change === "binary" ? (sides.old === null ? "added" : sides.new === null ? "deleted" : "modified") : file.change;
  return { kind: "diff", name, sides, file: { ...diffed, old_path: file.old_path, change, ...label } };
}
