/**
 * The Builds list's rules (#848, R-9.1, R-9.4a), over the rows `pipelines`
 * answers: what a row says and which rows the filter and chips keep.
 */
import { expect, test } from "vitest";

import type { PipelineReason, PipelineRow, RunRow } from "../../ipc/entity";
import {
  failedBuildLine,
  testsWords,
  foldOpen,
  pinnedOf,
  reasonsLine,
  treeOf,
  isFailing,
  isRunning,
  keptPipelines,
  latestTime,
  pipelineHash,
  pipelineOf,
  runHash,
  runTone,
  stripOf,
  newestFirst,
  recentNewestFirst,
} from "./builds";

function run(extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: "teamcity:build:1",
    number: "1",
    state: { text: "SUCCESS", tone: "ok" },
    branch: "main",
    started: "2026-08-22T10:00:00Z",
    finished: "2026-08-22T10:04:00Z",
    duration_seconds: 240,
    updated_at: "2026-08-22T10:04:00Z",
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
    ...extra,
  };
}

function pipeline(name: string, recent: RunRow[], extra: Partial<PipelineRow> = {}): PipelineRow {
  return {
    entity_id: `teamcity:buildType:${name}`,
    source_id: "teamcity",
    source_name: "TeamCity",
    name,
    group: "Payout",
    path: [],
    pinned: false,
    web_url: null,
    recent,
    ...extra,
  };
}

const failed = run({ state: { text: "FAILURE", tone: "err" } });
const running = run({ state: { text: "running", tone: "info" }, finished: null, duration_seconds: null });
const queued = run({ state: { text: "queued", tone: "info" }, started: null, finished: null, number: null });

test("a pipeline fails when its latest run failed, not an older one", () => {
  expect(isFailing(pipeline("A", [failed, run()]))).toBe(true);
  expect(isFailing(pipeline("B", [run(), failed]))).toBe(false);
  expect(isFailing(pipeline("C", []))).toBe(false);
});

test("a pipeline is running while one of its runs started and has not finished", () => {
  expect(isRunning(pipeline("A", [running, failed]))).toBe(true);
  expect(isRunning(pipeline("B", [queued, run()]))).toBe(false);
});

test("a run's block tone: finished by its declared tone, in flight as running or queued", () => {
  expect(runTone(failed)).toBe("err");
  expect(runTone(run())).toBe("ok");
  expect(runTone(running)).toBe("run");
  expect(runTone(queued)).toBe("q");
  expect(runTone(run({ state: { text: "UNKNOWN", tone: "muted" } }))).toBe("muted");
  expect(runTone(run({ state: null }))).toBe("muted");
});

test("the strip is the last twelve runs oldest first, so the newest is on the right", () => {
  const recent = Array.from({ length: 14 }, (_, i) => run({ entity_id: `r${i}` }));
  const strip = stripOf(pipeline("A", recent));
  expect(strip).toHaveLength(12);
  expect(strip.at(-1)?.entity_id).toBe("r0");
  expect(strip[0]?.entity_id).toBe("r11");
});

test("the latest time reads as prototype-6 writes it: a time today, yesterday and a time, else the date", () => {
  const now = new Date(2026, 7, 22, 14, 32);
  const at = (d: Date) => pipeline("A", [run({ started: d.toISOString(), updated_at: d.toISOString() })]);
  expect(latestTime(at(new Date(2026, 7, 22, 11, 45)), now)).toBe("11:45");
  expect(latestTime(at(new Date(2026, 7, 21, 16, 20)), now)).toBe("yesterday 16:20");
  expect(latestTime(at(new Date(2026, 7, 19, 9, 0)), now)).toBe("Wed 09:00");
  expect(latestTime(pipeline("B", []), now)).toBe("");
  // A queued run has not started: its time is when the source last touched it.
  const waiting = run({ started: null, finished: null, updated_at: new Date(2026, 7, 22, 14, 20).toISOString() });
  expect(latestTime(pipeline("C", [waiting]), now)).toBe("14:20");
});

test("the chips keep the failing or the running ones, and both together keep what is both", () => {
  const list = [
    pipeline("Payout_IntegrationTests", [failed]),
    pipeline("Payout_Build", [running]),
    pipeline("Ledger_Build", [run()], { group: "Ledger" }),
  ];
  const names = (kept: PipelineRow[]) => kept.map((p) => p.name);
  expect(names(keptPipelines(list, []))).toHaveLength(3);
  expect(names(keptPipelines(list, ["Failing"]))).toEqual(["Payout_IntegrationTests"]);
  expect(names(keptPipelines(list, ["Running"]))).toEqual(["Payout_Build"]);
  expect(names(keptPipelines(list, ["Failing", "Running"]))).toEqual([]);
});

test("the address carries the entity id, and a bare #/builds selects the first failing pipeline", () => {
  const list = [pipeline("A", [run()]), pipeline("B", [failed]), pipeline("C", [failed])];
  expect(pipelineHash("teamcity:buildType:B")).toBe("#/builds/teamcity%3AbuildType%3AB");
  expect(pipelineOf(list, ["teamcity:buildType:C"])?.name).toBe("C");
  expect(pipelineOf(list, [])?.name).toBe("B");
  expect(pipelineOf([pipeline("A", [run()])], [])?.name).toBe("A");
  expect(pipelineOf(list, ["teamcity:buildType:Gone"])).toBeNull();
  expect(pipelineOf([], [])).toBeNull();
});

test("a run's address carries its pipeline's entity id and its own", () => {
  expect(runHash("teamcity:buildType:A", "teamcity:build:7")).toBe("#/build/teamcity%3AbuildType%3AA/teamcity%3Abuild%3A7");
});

// -- the project tree and pins (#849), *This job* (#850) ----------------------

function at(name: string, path: string[], failing = false, extra: Partial<PipelineRow> = {}): PipelineRow {
  return pipeline(name, [run(failing ? { state: { text: "FAILURE", tone: "err" } } : {})], { path, ...extra });
}

test("the tree is as deep as the source's: a pipeline under its chain of groups, folds by name, own pipelines first", () => {
  const [tree] = treeOf([
    at("Payout_Build", ["Payments", "Payouts"]),
    at("Payout_Deploy_Prod", ["Payments", "Payouts", "Deploy"], true),
    at("Dbt_Test", ["Data"], true),
    at("Ledger_Build", ["Payments", "Ledger"]),
    at("Loose", [], false, { group: null }),
  ]);
  expect(tree!.source).toBe("TeamCity");
  expect(tree!.pipelines.map((p) => p.name)).toEqual(["Loose"]);
  expect(tree!.folds.map((f) => f.name)).toEqual(["Data", "Payments"]);
  const payments = tree!.folds[1]!;
  expect([payments.count, payments.failing]).toEqual([3, 1]);
  expect(payments.pipelines).toEqual([]);
  expect(payments.folds.map((f) => f.name)).toEqual(["Ledger", "Payouts"]);
  const payouts = payments.folds[1]!;
  expect(payouts.pipelines.map((p) => p.name)).toEqual(["Payout_Build"]);
  expect(payouts.folds.map((f) => f.name)).toEqual(["Deploy"]);
  expect(payouts.key).toBe("teamcity/Payments/Payouts");
});

test("a group holding only one subgroup is one row with it, as far as the chain of single children goes", () => {
  const [tree] = treeOf([at("Deep", ["A", "B", "C"]), at("Beside", ["A", "B", "C", "D"])]);
  expect(tree!.folds.map((f) => f.name)).toEqual(["A › B › C"]);
  expect(tree!.folds[0]!.pipelines.map((p) => p.name)).toEqual(["Deep"]);
  expect(tree!.folds[0]!.folds.map((f) => f.name)).toEqual(["D"]);
});

test("a pipeline with no declared chain sits under its group; one sources' tree each", () => {
  const trees = treeOf([
    at("Old", [], false, { group: "Payout" }),
    at("Other", ["X"], false, { source_id: "tc2", source_name: "Second" }),
  ]);
  expect(trees.map((t) => t.source)).toEqual(["TeamCity", "Second"]);
  expect(trees[0]!.folds.map((f) => f.name)).toEqual(["Payout"]);
});

test("a fold opens as the reader left it, else when something narrows, it holds a failure or the selection", () => {
  const [tree] = treeOf([at("Fine", ["Calm"]), at("Bad", ["Loud"], true)]);
  const [calm, loud] = [tree!.folds[0]!, tree!.folds[1]!];
  expect(foldOpen(undefined, calm, null, false)).toBe(false);
  expect(foldOpen(undefined, loud, null, false)).toBe(true);
  expect(foldOpen(undefined, calm, "teamcity:buildType:Fine", false)).toBe(true);
  expect(foldOpen(undefined, calm, null, true)).toBe(true);
  expect(foldOpen(false, loud, null, true)).toBe(false);
  expect(foldOpen(true, calm, null, false)).toBe(true);
});

test("pinned pipelines, in the list's order", () => {
  const list = [at("A", [], false, { pinned: true }), at("B", []), at("C", [], false, { pinned: true })];
  expect(pinnedOf(list).map((p) => p.name)).toEqual(["A", "C"]);
});

test("a reason reads in prototype-6's words, each once", () => {
  const reasons: PipelineReason[] = [
    { kind: "branch", branch: "feature/PAY-231-sepa-retry" },
    { kind: "branch", branch: "feature/PAY-231-temp" },
    { kind: "pr_check", pr: "gitea:svc#142" },
    { kind: "linked", step: "jira:PAY-230", origin: { kind: "parent", ticket: "jira:PAY-230" } },
    { kind: "linked", step: "teamcity:buildType:X", origin: { kind: "own" } },
  ];
  expect(reasonsLine(reasons, "PAY-231")).toBe("your branch, PR check, Parent PAY-230, linked to PAY-231");
});

test("with This job on, a bare #/builds selects the job's failing pipeline, else its first", () => {
  const list = [at("Elsewhere", [], true), at("Mine", []), at("MineBad", [], true)];
  expect(pipelineOf(list, [], [list[1]!, list[2]!])?.name).toBe("MineBad");
  expect(pipelineOf(list, [], [list[1]!])?.name).toBe("Mine");
  expect(pipelineOf(list, [], [])?.name).toBe("Elsewhere");
});

test("the rail's time climbs the one ladder at every width (F-049): the clock today, yesterday, the weekday this past week, else the date", () => {
  const now = new Date(2026, 7, 22, 15, 0);
  const at = (d: Date) => pipeline("A", [run({ started: d.toISOString(), updated_at: d.toISOString() })]);
  expect(latestTime(at(new Date(2026, 7, 22, 11, 45)), now)).toBe("11:45");
  expect(latestTime(at(new Date(2026, 7, 21, 16, 20)), now)).toBe("yesterday 16:20");
  expect(latestTime(at(new Date(2026, 7, 16, 9, 0)), now)).toBe("Sun 09:00");
  expect(latestTime(at(new Date(2026, 7, 15, 9, 0)), now)).toBe("15 Aug");
  expect(latestTime(pipeline("B", []), now)).toBe("");
});

test("a run's tests read one way, and only TeamCity's own failed-build line is read as counts (F-010)", () => {
  expect(testsWords({ failed: 1, passed: 41, ignored: 0 })).toBe("1 failed, 41 passed");
  expect(testsWords({ failed: 0, passed: 41, ignored: 2 })).toBe("41 passed, 2 ignored");
  expect(testsWords({ failed: 0, passed: 0, ignored: 0 })).toBe("0 passed");
  expect(failedBuildLine("Tests failed: 3 (3 new), passed: 212")).toBe("3 failed, 212 passed");
  expect(failedBuildLine("Tests failed: 3")).toBe("3 failed");
  expect(failedBuildLine("Tests failed: 1, passed: 9, ignored: 2")).toBe("1 failed, 9 passed, 2 ignored");
  expect(failedBuildLine("Tests failed: 1, passed: 9, muted: 2"), "a shape it doesn't know stays the source's").toBe("Tests failed: 1, passed: 9, muted: 2");
  expect(failedBuildLine("Compilation error")).toBe("Compilation error");
});

test("runs sort newest first by when they started, a queued one by its queue time, the read's order between equals", () => {
  const a = run({ entity_id: "a", started: "2026-08-22T10:00:00Z", finished: "2026-08-22T11:00:00Z", updated_at: "2026-08-22T11:00:00Z" });
  const b = run({ entity_id: "b", started: "2026-08-22T10:30:00Z", finished: "2026-08-22T10:35:00Z", updated_at: "2026-08-22T10:35:00Z" });
  const q = run({ entity_id: "q", started: null, finished: null, updated_at: "2026-08-22T10:50:00Z" });
  const same = run({ entity_id: "same", started: "2026-08-22T10:30:00Z" });
  const undated = run({ entity_id: "u", started: null, finished: null, updated_at: null });
  expect(newestFirst([undated, a, b, same, q]).map((r) => r.entity_id)).toEqual(["q", "b", "same", "a", "u"]);
});

test("the Builds list's strip and latest run read the recent runs newest first", () => {
  const older = run({ entity_id: "older", started: "2026-08-22T09:00:00Z", finished: "2026-08-22T12:00:00Z", state: { text: "FAILURE", tone: "err" } });
  const newer = run({ entity_id: "newer", started: "2026-08-22T10:00:00Z", finished: "2026-08-22T10:05:00Z" });
  const row = recentNewestFirst(pipeline("Payout", [older, newer]));
  expect(row.recent.map((r) => r.entity_id)).toEqual(["newer", "older"]);
  expect(isFailing(row)).toBe(false);
  expect(stripOf(row).at(-1)?.entity_id).toBe("newer");
});
