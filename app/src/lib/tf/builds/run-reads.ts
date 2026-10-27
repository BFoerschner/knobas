/**
 * The run page's Log, Tests and Artifacts rules (#854, #855, #856) as pure
 * functions over the run reads (`run_log`, `run_tests`, `test_history`,
 * `run_artifacts`): the log's blocks and what shows of them, the tests'
 * order and history line, and the artifact tree's words.
 */
import { secondsLabel } from "../../time/format";
import { ipcErrorMessage } from "../../ipc";
import type { ArtifactEntry, DownloadTarget, LogLine, LogStep, RunLog, RunTest, TestOutcome } from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { marks, matchesAll } from "../filter/filter";

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

/** A step's block. */
export type StepBlock = Extract<LogBlock, { kind: "step" }>;

/**
 * One row of the log as the windowed list draws it (#2211): a step's
 * summary, a shown line, or the edges of a run of loose lines (the rule and
 * padding their wrapper drew). `key` is unique: a line's number, else below 0.
 */
export type LogRow =
  | { kind: "line"; key: number; line: LogLine }
  | { kind: "step"; key: number; block: StepBlock }
  | { kind: "loose-top" | "loose-end"; key: number };

/**
 * The blocks flattened into the rows drawn, in order: each step's summary,
 * then its shown lines while it is `open`; each loose run's shown lines
 * between its edges.
 */
export function logRows(blocks: readonly LogBlock[], open: (block: StepBlock) => boolean, shown: (line: LogLine) => boolean): LogRow[] {
  const rows: LogRow[] = [];
  const lines = (list: readonly LogLine[]) => {
    for (const line of list) if (shown(line)) rows.push({ kind: "line", key: line.number, line });
  };
  blocks.forEach((block, b) => {
    if (block.kind === "loose") {
      rows.push({ kind: "loose-top", key: -2 - 3 * b });
      lines(block.lines);
      rows.push({ kind: "loose-end", key: -3 - 3 * b });
      return;
    }
    rows.push({ kind: "step", key: -1 - 3 * b, block });
    if (open(block)) lines(block.lines);
  });
  return rows;
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
  const took = state === "run" ? "running" : step.seconds === null ? null : secondsLabel(step.seconds);
  const count = `${lines.toLocaleString("en-US")} line${lines === 1 ? "" : "s"}`;
  return took === null ? count : `${took}, ${count}`;
}

/** One segment of the run page's steps bar (#1003, R-9.10), placed in percent of the bar. */
export interface StepSegment {
  /**
   * The step's place in the run: in `RunLog.steps`, the Log tab's fold it
   * opens; past them, a step not started (#1016), which has no fold.
   */
  index: number;
  name: string;
  /** `wait`: a step of a running run its log has not reached yet (#1016). */
  state: StepState | "wait";
  /** The name and how long it took: "Checkout, 12 s", "cargo test, running", or "Publish, not started". */
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
 * The steps of a run in flight its log has not reached yet (#1016): the
 * pipeline's steps (`PipelineSettings.steps`) past the ones the log heads,
 * when the log heads its steps by those names in that order. A pipeline
 * changed since the run started, or one whose source names no steps ahead of
 * time, has none.
 */
function notStarted(log: RunLog, inFlight: boolean, planned: readonly string[]): string[] {
  if (!inFlight || log.steps.length >= planned.length) return [];
  return log.steps.every((step, i) => step.name === planned[i]) ? planned.slice(log.steps.length) : [];
}

/**
 * The steps bar over the log's steps: one segment per step, as wide as its
 * share of the time, then a running run's steps not started yet (#1016). A
 * step with no duration (running, unstamped or not started) takes the mean
 * of the others'. No read, or no step to draw, is no bar.
 */
export function stepSegments(log: RunLog | null, inFlight: boolean, planned: readonly string[] = []): StepSegment[] {
  if (log === null) return [];
  const waiting = notStarted(log, inFlight, planned);
  if (log.steps.length + waiting.length === 0) return [];
  const known = log.steps.flatMap((s) => (s.seconds === null ? [] : [s.seconds]));
  const mean = known.length === 0 ? 1 : known.reduce((a, b) => a + b, 0) / known.length || 1;
  const weights = [...log.steps.map((s) => s.seconds ?? mean), ...waiting.map(() => mean)];
  const total = weights.reduce((a, b) => a + b, 0);
  const shares = withFloor(weights.map((w) => (total === 0 ? 100 / weights.length : (w / total) * 100)));
  let x = 0;
  const place = (index: number, name: string, state: StepSegment["state"], took: string | null): StepSegment => {
    const width = shares[index]!;
    const segment = { index, name, state, tip: took === null ? name : `${name}, ${took}`, x, width };
    x += width;
    return segment;
  };
  const drawn = log.steps.map((step, index) => {
    const state = stepState(log, index, inFlight);
    return place(index, step.name, state, state === "run" ? "running" : step.seconds === null ? null : secondsLabel(step.seconds));
  });
  return [...drawn, ...waiting.map((name, i) => place(log.steps.length + i, name, "wait", "not started"))];
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
  return ms < 60_000 ? `${ms} ms` : secondsLabel(Math.round(ms / 1000));
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
