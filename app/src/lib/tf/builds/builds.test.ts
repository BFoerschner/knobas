/**
 * The Builds list's rules (#848, R-9.1, R-9.4a), over the rows `pipelines`
 * answers: what a row says and which rows the filter and chips keep.
 */
import { expect, test } from "vitest";

import type { PipelineRow, RunRow } from "../../ipc/entity";
import {
  duration,
  isFailing,
  isRunning,
  keptPipelines,
  latestTime,
  pipelineHash,
  pipelineOf,
  runHash,
  runTone,
  stripOf,
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

test("the latest time reads as prototype-6 writes it: a time today, yest. and a time, else the date", () => {
  const now = new Date(2026, 7, 22, 14, 32);
  const at = (d: Date) => pipeline("A", [run({ started: d.toISOString(), updated_at: d.toISOString() })]);
  expect(latestTime(at(new Date(2026, 7, 22, 11, 45)), now)).toBe("11:45");
  expect(latestTime(at(new Date(2026, 7, 21, 16, 20)), now)).toBe("yest. 16:20");
  expect(latestTime(at(new Date(2026, 7, 19, 9, 0)), now)).toBe("2026-08-19");
  expect(latestTime(pipeline("B", []), now)).toBe("");
  // A queued run has not started: its time is when the source last touched it.
  const waiting = run({ started: null, finished: null, updated_at: new Date(2026, 7, 22, 14, 20).toISOString() });
  expect(latestTime(pipeline("C", [waiting]), now)).toBe("14:20");
});

test("the filter keeps a pipeline every typed word finds in its name or group; the chips narrow further", () => {
  const list = [
    pipeline("Payout_IntegrationTests", [failed]),
    pipeline("Payout_Build", [running]),
    pipeline("Ledger_Build", [run()], { group: "Ledger" }),
  ];
  const names = (kept: PipelineRow[]) => kept.map((p) => p.name);
  expect(names(keptPipelines(list, "", []))).toHaveLength(3);
  expect(names(keptPipelines(list, "payout build", []))).toEqual(["Payout_Build"]);
  expect(names(keptPipelines(list, "ledger", []))).toEqual(["Ledger_Build"]);
  expect(names(keptPipelines(list, "", ["Failing"]))).toEqual(["Payout_IntegrationTests"]);
  expect(names(keptPipelines(list, "", ["Running"]))).toEqual(["Payout_Build"]);
  expect(names(keptPipelines(list, "", ["Failing", "Running"]))).toEqual([]);
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

test("a duration reads as prototype-6's: seconds under a minute, else minutes and padded seconds", () => {
  expect(duration(38)).toBe("38 s");
  expect(duration(252)).toBe("4 m 12 s");
  expect(duration(65)).toBe("1 m 05 s");
});

test("a run's address carries its pipeline's entity id and its own", () => {
  expect(runHash("teamcity:buildType:A", "teamcity:build:7")).toBe("#/build/teamcity%3AbuildType%3AA/teamcity%3Abuild%3A7");
});
