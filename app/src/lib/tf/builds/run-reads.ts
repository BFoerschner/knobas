/**
 * The run page's Log, Tests and Artifacts rules (#854, #855, #856) as pure
 * functions over the run reads (`run_log`, `run_tests`, `test_history`,
 * `run_artifacts`): the log's blocks and what shows of them, the tests'
 * order and history line, and the artifact tree's words.
 */
import { ipcErrorMessage } from "../../ipc";
import type { ArtifactEntry, DownloadTarget, LogLine, LogStep, RunLog, RunTest, TestOutcome } from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { marks, matchesAll } from "../filter/filter";
import { duration } from "./builds";

// -- the log (#854) -----------------------------------------------------------

/**
 * One block of the log as it is drawn: a step's fold, or the lines before,
 * between or after the steps, which sit in no fold.
 */
export type LogBlock =
  | { kind: "step"; index: number; step: LogStep; lines: LogLine[] }
  | { kind: "loose"; lines: LogLine[] };

/** The log's lines grouped into its blocks, in order. */
export function logBlocks(log: RunLog): LogBlock[] {
  const blocks: LogBlock[] = [];
  for (const line of log.lines) {
    const last = blocks.at(-1);
    if (line.step === null) {
      if (last?.kind === "loose") last.lines.push(line);
      else blocks.push({ kind: "loose", lines: [line] });
      continue;
    }
    if (last?.kind === "step" && last.index === line.step) last.lines.push(line);
    else blocks.push({ kind: "step", index: line.step, step: log.steps[line.step]!, lines: [line] });
  }
  return blocks;
}

/** How a step reads in its fold's summary: `err`, `run` or `ok`. */
export type StepState = "err" | "run" | "ok";

/** A step's state: an error inside it is `err`; the last step of a run in flight is `run`. */
export function stepState(log: RunLog, index: number, inFlight: boolean): StepState {
  const step = log.steps[index]!;
  if (step.tone === "err") return "err";
  if (inFlight && index === log.steps.length - 1) return "run";
  return "ok";
}

/** The fold's right-hand words: "2m 08s, 31 lines", or "running, 4 lines". */
export function stepSummary(step: LogStep, state: StepState): string {
  const lines = step.last - step.first + 1;
  const took = state === "run" ? "running" : step.seconds === null ? null : duration(step.seconds);
  const count = `${lines.toLocaleString("en-US")} line${lines === 1 ? "" : "s"}`;
  return took === null ? count : `${took}, ${count}`;
}

/** One segment of the run page's steps bar (#1003, R-9.10), placed in percent of the bar. */
export interface StepSegment {
  /** The step's place in `RunLog.steps`: the Log tab's fold it opens. */
  index: number;
  name: string;
  state: StepState;
  /** The name and how long it took: "Checkout, 12 s", or "cargo test, running". */
  tip: string;
  x: number;
  width: number;
}

/** The narrowest a segment is drawn, in percent of the bar, so a 0 s step can still be seen and clicked. */
const SEGMENT_MIN = 2;

/**
 * Percent shares summing to 100, none under `SEGMENT_MIN`: the slivers get the
 * minimum, and the rest share what is left by their time, until none of those
 * falls under it.
 */
function withFloor(raw: readonly number[]): number[] {
  const floor = Math.min(SEGMENT_MIN, 100 / raw.length);
  const pinned = new Set<number>();
  for (;;) {
    const free = 100 - floor * pinned.size;
    const freeRaw = raw.reduce((sum, share, i) => (pinned.has(i) ? sum : sum + share), 0);
    const shares = raw.map((share, i) => (pinned.has(i) ? floor : freeRaw === 0 ? free / (raw.length - pinned.size) : (share / freeRaw) * free));
    const under = shares.findIndex((share, i) => !pinned.has(i) && share < floor);
    if (under === -1) return shares;
    pinned.add(under);
  }
}

/**
 * The steps bar over the log's steps: one segment per step, as wide as its
 * share of the time. A step with no duration (running, or unstamped) takes
 * the mean of the others'. No read or no steps is no bar.
 */
export function stepSegments(log: RunLog | null, inFlight: boolean): StepSegment[] {
  if (log === null || log.steps.length === 0) return [];
  const known = log.steps.flatMap((s) => (s.seconds === null ? [] : [s.seconds]));
  const mean = known.length === 0 ? 1 : known.reduce((a, b) => a + b, 0) / known.length || 1;
  const weights = log.steps.map((s) => s.seconds ?? mean);
  const total = weights.reduce((a, b) => a + b, 0);
  const shares = withFloor(weights.map((w) => (total === 0 ? 100 / weights.length : (w / total) * 100)));
  let x = 0;
  return log.steps.map((step, index) => {
    const state = stepState(log, index, inFlight);
    const took = state === "run" ? "running" : step.seconds === null ? null : duration(step.seconds);
    const width = shares[index]!;
    const segment = { index, name: step.name, state, tip: took === null ? step.name : `${step.name}, ${took}`, x, width };
    x += width;
    return segment;
  });
}

/** How many find hits a line holds: the marked runs `marks` draws. */
export function hitsIn(text: string, typed: string): number {
  return typed.trim() === "" ? 0 : marks(text, typed).filter((run) => run.hit).length;
}

/**
 * Whether a line is drawn with the options on: *Errors and warnings only*
 * keeps the toned lines, and with a find too only those that match
 * (prototype-6's `drawLog`).
 */
export function lineShown(line: LogLine, typed: string, errorsOnly: boolean): boolean {
  if (!errorsOnly) return true;
  if (line.tone !== "err" && line.tone !== "warn") return false;
  return typed.trim() === "" || hitsIn(line.text, typed) > 0;
}

/** A step's fold opens by itself when it failed or is running, holds an error or a find hit, or the errors-only view is on. */
export function stepOpens(block: Extract<LogBlock, { kind: "step" }>, state: StepState, typed: string, errorsOnly: boolean): boolean {
  if (state !== "ok" || errorsOnly) return true;
  return block.lines.some((l) => l.tone === "err" || hitsIn(l.text, typed) > 0);
}

/** The find's count: "" before anything is typed, "0 found", or "2 of 5". */
export function findCount(typed: string, total: number, current: number): string {
  if (typed.trim() === "") return "";
  if (total === 0) return "0 found";
  return `${current + 1} of ${total}`;
}

/** The next match from `current`, `step` +1 or -1, wrapping. */
export function stepMatch(current: number, step: 1 | -1, total: number): number {
  return total === 0 ? 0 : (current + step + total) % total;
}

/** The line under the log: "The whole log, 412 lines." or, past the cap, the part shown. */
export function logLine(log: RunLog): string {
  const total = log.total.toLocaleString("en-US");
  if (log.lines.length >= log.total) return `The whole log, ${total} line${log.total === 1 ? "" : "s"}.`;
  const shown = log.lines.length.toLocaleString("en-US");
  return `The last ${shown} of ${total} lines. Download log has all of them.`;
}

// -- the tests (#855) ---------------------------------------------------------

/** The words for a result chip, and back. */
export const RESULT_CHIPS = [
  { value: "failed", label: "Failed" },
  { value: "passed", label: "Passed" },
] as const;

/** The tests the filter keeps, failed first: results switched on (any), the module, and the typed words. */
export function keptTests(
  tests: readonly RunTest[],
  typed: string,
  results: readonly string[],
  module: string | null,
): RunTest[] {
  return tests
    .filter((t) => results.length === 0 || results.includes(t.status))
    .filter((t) => module === null || t.module === module)
    .filter((t) => matchesAll(t.name, typed))
    .sort((a, b) => Number(b.status === "failed" && !b.muted) - Number(a.status === "failed" && !a.muted));
}

/** The modules the tests are in, in their first order. */
export function modulesOf(tests: readonly RunTest[]): string[] {
  return [...new Set(tests.flatMap((t) => (t.module === null ? [] : [t.module])))];
}

/**
 * How often a test failed in its history: "Failed in 2 of the last 10 runs,
 * both on this branch." (prototype-6). The branch is named when every
 * failure was on it.
 */
export function historyLine(history: readonly TestOutcome[], branch: string | null): string {
  const runs = history.length;
  if (runs === 0) return "No earlier run has this test.";
  const failed = history.filter((o) => o.status === "failed");
  const of = `${failed.length} of the last ${runs} run${runs === 1 ? "" : "s"}`;
  if (failed.length === 0) return `Passed in all of the last ${runs} run${runs === 1 ? "" : "s"}.`;
  const here = branch !== null && failed.every((o) => o.branch === branch);
  const where = !here ? "" : failed.length === 1 ? ", on this branch" : failed.length === 2 ? ", both on this branch" : ", all on this branch";
  return `Failed in ${of}${where}.`;
}

/** A test's time: "42 ms", or "1m 02s" past a minute. */
export function testTime(ms: number | null): string | null {
  if (ms === null) return null;
  return ms < 60_000 ? `${ms} ms` : duration(Math.round(ms / 1000));
}

// -- the artifacts (#856) -----------------------------------------------------

/** A size as the adapter's view writes one: "980 B", "14.2 KB", "1.1 MB". */
export function sizeText(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = bytes / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(1)} ${units[unit]}`;
}

/** A folder's count: "2 files", "1 file". */
export function filesText(entry: ArtifactEntry): string | null {
  return entry.children === null ? null : `${entry.children} file${entry.children === 1 ? "" : "s"}`;
}

const TEXT_EXTENSIONS = new Set(
  "txt log xml html htm json md csv tsv yml yaml toml ini cfg conf properties sh info out err sql js ts css svg gradle kt java rs py go rb diff patch".split(" "),
);

/** Whether *Preview* is offered: a file whose extension is a text one, or that has none (`SHA256SUMS`). */
export function previewable(entry: ArtifactEntry): boolean {
  if (entry.folder) return false;
  const dot = entry.name.lastIndexOf(".");
  return dot <= 0 || TEXT_EXTENSIONS.has(entry.name.slice(dot + 1).toLowerCase());
}

/** The toast after a download: "Saved junit.xml to ~/Downloads". */
export function savedText(path: string): string {
  const name = path.split(/[\\/]/).at(-1) ?? path;
  return `Saved ${name} to ~/Downloads`;
}

/**
 * One download through knobas (D8), said in a toast either way: where it was
 * saved, or what failed. `what` names it in the failure: "the log", "junit.xml".
 */
export async function downloadAndSay(
  download: (runId: string, target: DownloadTarget) => Promise<string | null>,
  runId: string,
  target: DownloadTarget,
  what: string,
): Promise<void> {
  try {
    const saved = await download(runId, target);
    push(saved === null ? { text: `This source has no ${what} to download.`, tone: "err" } : { text: savedText(saved) });
  } catch (cause: unknown) {
    push({ text: `Could not download ${what}: ${ipcErrorMessage(cause)}`, tone: "err" });
  }
}
