import { expect, test } from "vitest";

import type { ArtifactEntry, LogLine, RunLog, RunTest, TestOutcome } from "../../ipc/entity";
import {
  filesText,
  findCount,
  historyLine,
  keptTests,
  lineShown,
  logBlocks,
  logLine,
  modulesOf,
  previewable,
  savedText,
  sizeText,
  stepMatch,
  stepOpens,
  stepState,
  stepSummary,
} from "./run-reads";

const line = (number: number, step: number | null, text: string, tone: LogLine["tone"] = null): LogLine => ({
  number,
  time: "10:06:00",
  text,
  tone,
  step,
});

/** A preamble line, a passing step, a failing one, and a closing line. */
const LOG: RunLog = {
  total: 7,
  steps: [
    { name: "Checkout", first: 2, last: 3, seconds: 12, tone: null },
    { name: "cargo test", first: 4, last: 6, seconds: 82, tone: "err" },
  ],
  lines: [
    line(1, null, "TeamCity server version is 2026.1.3"),
    line(2, 0, "Step 1/2: Checkout"),
    line(3, 0, "Updating sources"),
    line(4, 1, "Step 2/2: cargo test"),
    line(5, 1, "warning: unused variable", "warn"),
    line(6, 1, "test sepa::retry::gives_up ... FAILED", "err"),
    line(7, null, "Build finished"),
  ],
};

test("the log is its steps' folds with the lines outside them loose, in order", () => {
  const blocks = logBlocks(LOG);
  expect(blocks.map((b) => (b.kind === "step" ? `step ${b.step.name} ${b.lines.length}` : `loose ${b.lines.length}`))).toEqual([
    "loose 1",
    "step Checkout 2",
    "step cargo test 3",
    "loose 1",
  ]);
});

test("a step is err with an error in it, running when it is the last of a run in flight, else ok", () => {
  expect(stepState(LOG, 0, false)).toBe("ok");
  expect(stepState(LOG, 1, true)).toBe("err");
  const passing: RunLog = { ...LOG, steps: [LOG.steps[0]!, { ...LOG.steps[1]!, tone: "warn" }] };
  expect(stepState(passing, 1, true)).toBe("run");
  expect(stepState(passing, 1, false)).toBe("ok");
  expect(stepSummary(LOG.steps[1]!, "err")).toBe("1 m 22 s, 3 lines");
  expect(stepSummary(LOG.steps[0]!, "run")).toBe("running, 2 lines");
  expect(stepSummary({ ...LOG.steps[0]!, seconds: null, first: 1, last: 1 }, "ok")).toBe("1 line");
});

test("a fold opens on failure, on a find hit inside it, or with errors only on", () => {
  const [, checkout, tests] = logBlocks(LOG) as Extract<ReturnType<typeof logBlocks>[number], { kind: "step" }>[];
  expect(stepOpens(checkout!, "ok", "", false)).toBe(false);
  expect(stepOpens(checkout!, "ok", "sources", false)).toBe(true);
  expect(stepOpens(checkout!, "ok", "", true)).toBe(true);
  expect(stepOpens(tests!, "err", "", false)).toBe(true);
});

test("errors and warnings only keeps the toned lines, and with a find only those that match", () => {
  const shown = (typed: string, errors: boolean) => LOG.lines.filter((l) => lineShown(l, typed, errors)).map((l) => l.number);
  expect(shown("", false)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  expect(shown("", true)).toEqual([5, 6]);
  expect(shown("failed", true)).toEqual([6]);
  expect(shown("failed", false)).toEqual([1, 2, 3, 4, 5, 6, 7]);
});

test("the find counts N of M and wraps both ways", () => {
  expect(findCount("", 0, 0)).toBe("");
  expect(findCount("nope", 0, 0)).toBe("0 found");
  expect(findCount("step", 5, 1)).toBe("2 of 5");
  expect(stepMatch(4, 1, 5)).toBe(0);
  expect(stepMatch(0, -1, 5)).toBe(4);
});

test("the line under the log says whole, or which part past the cap", () => {
  expect(logLine(LOG)).toBe("The whole log, 7 lines.");
  expect(logLine({ ...LOG, total: 31_402 })).toBe("The last 7 of 31,402 lines. Download log has all of them.");
});

const t = (name: string, status: RunTest["status"], module: string | null = null, muted = false): RunTest => ({
  id: null,
  name,
  module,
  status,
  muted,
  new_failure: false,
  duration_ms: 40,
  details: null,
});

test("tests are failed first and filter by result, module and words", () => {
  const tests = [
    t("sepa::retry::backs_off", "passed", "sepa::retry"),
    t("ledger::books", "failed", "ledger", true),
    t("sepa::retry::gives_up", "failed", "sepa::retry"),
    t("smoke", "ignored"),
  ];
  expect(keptTests(tests, "", [], null).map((x) => x.name)).toEqual(["sepa::retry::gives_up", "sepa::retry::backs_off", "ledger::books", "smoke"]);
  expect(keptTests(tests, "", ["failed"], null).map((x) => x.name)).toEqual(["sepa::retry::gives_up", "ledger::books"]);
  expect(keptTests(tests, "", [], "sepa::retry").map((x) => x.name)).toEqual(["sepa::retry::gives_up", "sepa::retry::backs_off"]);
  expect(keptTests(tests, "retry back", [], null).map((x) => x.name)).toEqual(["sepa::retry::backs_off"]);
  expect(modulesOf(tests)).toEqual(["sepa::retry", "ledger"]);
});

const o = (status: TestOutcome["status"], branch: string): TestOutcome => ({ run: "r", number: null, branch, status });

test("a test's history says how often it failed, and where when every failure was on this branch", () => {
  const feature = "feature/PAY-231";
  const history = [o("failed", feature), o("passed", "main"), o("failed", feature), ...Array.from({ length: 7 }, () => o("passed", "main"))];
  expect(historyLine(history, feature)).toBe("Failed in 2 of the last 10 runs, both on this branch.");
  expect(historyLine([o("failed", feature), o("failed", "main")], feature)).toBe("Failed in 2 of the last 2 runs.");
  expect(historyLine([o("failed", feature)], feature)).toBe("Failed in 1 of the last 1 run, on this branch.");
  expect(historyLine([o("failed", feature), o("failed", feature), o("failed", feature)], feature)).toBe("Failed in 3 of the last 3 runs, all on this branch.");
  expect(historyLine([o("passed", "main")], feature)).toBe("Passed in all of the last 1 run.");
  expect(historyLine([], feature)).toBe("No earlier run has this test.");
});

const entry = (name: string, folder = false, size: number | null = 10, children: number | null = null): ArtifactEntry => ({
  path: name,
  name,
  size,
  folder,
  children,
});

test("artifact words: sizes as the view writes them, folder counts, what previews, and the saved toast", () => {
  expect([980, 14_541, 90_112, 1_153_434].map(sizeText)).toEqual(["980 B", "14.2 KB", "88.0 KB", "1.1 MB"]);
  expect(filesText(entry("logs", true, null, 2))).toBe("2 files");
  expect(filesText(entry("one", true, null, 1))).toBe("1 file");
  expect(filesText(entry("x.log"))).toBe(null);
  expect(["junit.xml", "integration.log", "SHA256SUMS", "payout-service-1.9.0.tar.gz", "payout-service.bin"].map((n) => previewable(entry(n)))).toEqual([
    true,
    true,
    true,
    false,
    false,
  ]);
  expect(previewable(entry("logs", true))).toBe(false);
  expect(savedText("/Users/mara/Downloads/junit (1).xml")).toBe("Saved junit (1).xml to ~/Downloads");
});
