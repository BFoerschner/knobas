/**
 * Builds' *Recent* rules (2026-11-25): its order, what a read again of its head keeps, what the backend is asked, the
 * quiet text after a pipeline's name and what typed words keep.
 */
import { expect, test } from "vitest";

import type { RecentPipeline, RecentRun, RunRow } from "../../ipc/entity";
import { pipelineHash } from "./builds";
import { runText } from "./pipeline";
import {
  RECENT,
  RECENT_HASH,
  RECENT_HOLD,
  isRecent,
  placeWords,
  recentFilterOf,
  recentMarks,
  recentText,
  recentOrder,
  withHead,
  withOlder,
  withPipelines,
  withRun,
} from "./recent";

function run(id: string, started: string | null, extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: id,
    number: id.split(":").at(-1) ?? null,
    state: { text: "SUCCESS", tone: "ok" },
    branch: "main",
    started,
    finished: started,
    duration_seconds: 60,
    updated_at: "2026-08-22T00:00:00Z",
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

const at = (id: string, hour: number, pipeline = "tc:buildType:Lint"): RecentRun => ({
  pipeline_id: pipeline,
  run: run(`tc:build:${id}`, `2026-08-22T${String(hour).padStart(2, "0")}:00:00Z`),
});

const ids = (runs: readonly RecentRun[]) => runs.map((one) => one.run.entity_id.split(":").at(-1));

const lint: RecentPipeline = { entity_id: "tc:buildType:Lint", source_id: "tc", source_name: "Tidewater TeamCity", name: "Lint", group: "Payout", path: ["Payments", "Payout"] };

test("the address is a word no pipeline's id is", () => {
  expect(RECENT_HASH).toBe("#/builds/recent");
  expect(isRecent([RECENT])).toBe(true);
  expect(isRecent(["recent", "runs"])).toBe(false);
  expect(isRecent(["tc:buildType:recent"])).toBe(false);
  // Every pipeline's address names its source: a pipeline can never be the word.
  expect(pipelineHash("tc:buildType:recent")).not.toBe(RECENT_HASH);
});

test("newest first by when a run started, a queued one by when it was queued, the greater id on a tie", () => {
  const long = run("tc:build:1", "2026-08-22T08:00:00Z", { finished: "2026-08-22T12:00:00Z", updated_at: "2026-08-22T12:00:00Z" });
  const short = run("tc:build:2", "2026-08-22T09:00:00Z");
  const queued = run("tc:build:3", null, { finished: null, updated_at: "2026-08-22T10:00:00Z" });
  const twin = run("tc:build:4", "2026-08-22T09:00:00Z");
  expect([long, short, queued, twin].sort(recentOrder).map((one) => one.entity_id)).toEqual(["tc:build:3", "tc:build:4", "tc:build:2", "tc:build:1"]);
  expect(recentOrder(short, short)).toBe(0);
});

test("a head read again keeps the older runs it held, and drops what left inside its span", () => {
  const held = [at("9", 9), at("8", 8), at("7", 7), at("6", 6), at("5", 5)];
  // The head of two: a new run at 10, 9 as it was; 8 is no longer in it, but past its span it stays held.
  const head = [at("10", 10), at("9", 9)];
  expect(ids(withHead(held, head, false))).toEqual(["10", "9", "8", "7", "6", "5"]);
  // 9 left within the head's span: it goes.
  expect(ids(withHead(held, [at("10", 10), at("8", 8)], false))).toEqual(["10", "8", "7", "6", "5"]);
  // A whole answer holds alone.
  expect(ids(withHead(held, head, true))).toEqual(["10", "9"]);
  expect(withHead(held, [], false)).toEqual([]);
});

test("an older page follows what is held, nothing twice, never past the hold", () => {
  expect(ids(withOlder([at("9", 9), at("8", 8)], [at("8", 8), at("7", 7)]))).toEqual(["9", "8", "7"]);
  const many = Array.from({ length: RECENT_HOLD + 10 }, (_, n) => at(String(n), 1));
  expect(withOlder([], many)).toHaveLength(RECENT_HOLD);
  expect(withHead([], many, true)).toHaveLength(RECENT_HOLD);
  expect(withHead(many.slice(0, 5), many, false)).toHaveLength(RECENT_HOLD);
});

test("a run read again at its source is laid in where it is held", () => {
  const moved = run("tc:build:8", "2026-08-22T08:00:00Z", { state: { text: "FAILURE", tone: "err" } });
  const after = withRun([at("9", 9), at("8", 8)], moved);
  expect(after[1]!.run.state?.tone).toBe("err");
  expect(after[0]!.run.state?.tone).toBe("ok");
});

test("the backend is asked the scope, the results and the source", () => {
  expect(recentFilterOf({ scope: "all", results: [], source: null }, null)).toEqual({ sources: [], results: [], mine: false, branches: null });
  expect(recentFilterOf({ scope: "mine", results: ["Failed", "Bogus"], source: "tc" }, null)).toEqual({
    sources: ["tc"],
    results: ["Failed"],
    mine: true,
    branches: null,
  });
  expect(recentFilterOf({ scope: "job", results: [], source: null }, ["feature/PAY-231"]).branches).toEqual(["feature/PAY-231"]);
  // *This job* before its branches are read asks for none, not for every run.
  expect(recentFilterOf({ scope: "job", results: [], source: null }, null).branches).toEqual([]);
});

test("a pipeline's place is its folders, its source first where two or more list", () => {
  expect(placeWords(lint, false)).toBe("Payments › Payout");
  expect(placeWords(lint, true)).toBe("Tidewater TeamCity › Payments › Payout");
  expect(placeWords({ ...lint, path: [], group: "Payout" }, false)).toBe("Payout");
  expect(placeWords({ ...lint, path: [], group: null }, false)).toBe("");
  expect(placeWords(undefined, true)).toBe("");
});

test("a run's text is its own words, then its pipeline's name and place as the row draws them, and its source (#2535)", () => {
  const pipes = withPipelines(new Map(), [lint, { ...lint, entity_id: "tc:buildType:Deploy", name: "Deploy", path: ["Ops"] }]);
  const deploy = at("2", 2, "tc:buildType:Deploy");
  const own = runText(deploy.run);
  const name = own.length + 1;
  expect(recentText(deploy, pipes, false)).toBe(`${own} Deploy ${placeWords(pipes.get("tc:buildType:Deploy"), false)} Tidewater TeamCity`);
  expect(recentText(deploy, pipes, true)).toBe(`${own} Deploy ${placeWords(pipes.get("tc:buildType:Deploy"), true)}`);
  // "De" of the name and "Op" of its place, past the run's own words.
  const place = name + "Deploy".length + 1;
  const marks = recentMarks(deploy, pipes, false, [name, name + 2, place, place + 2]);
  expect(marks.name).toEqual([0, 2]);
  expect(marks.place).toEqual([0, 2]);
  expect(marks.branch).toEqual([]);
  expect(recentText({ pipeline_id: "gone", run: deploy.run }, pipes, false)).toBe(`${own}  `);
});

test("a page's pipelines are laid over those held", () => {
  const held = withPipelines(new Map(), [lint]);
  const after = withPipelines(held, [{ ...lint, name: "Lint again" }]);
  expect(after.get(lint.entity_id)?.name).toBe("Lint again");
  expect(held.get(lint.entity_id)?.name).toBe("Lint");
});
