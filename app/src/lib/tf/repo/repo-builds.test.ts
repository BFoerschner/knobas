/**
 * A repository's Builds tab's rules (the repo-builds pass, `mockups/builds/repo-builds.md`): which branches the rail
 * lists and how, and what a branch's page says against the default branch.
 */
import { expect, test } from "vitest";

import type { RepoPipelineRow, RunRow } from "../../ipc/entity";
import { branchWords, compareOn, groupBranches, holdOrder, latestOn, railOf, verdictOf, yoursOf } from "./repo-builds";

function run(id: string, branch: string, tone: "ok" | "err" | "run" | "q", at: string): RunRow {
  const started = tone === "q" ? null : at;
  return {
    entity_id: id,
    number: id,
    state: tone === "run" || tone === "q" ? { text: tone === "run" ? "running" : "queued", tone: "info" } : { text: tone === "ok" ? "SUCCESS" : "FAILURE", tone },
    branch,
    started,
    finished: tone === "ok" || tone === "err" ? at : null,
    duration_seconds: null,
    updated_at: at,
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
  };
}

function pipeline(name: string, latest: RunRow[]): RepoPipelineRow {
  return { entity_id: `tc:buildType:${name}`, source_id: "tc", source_name: "TeamCity", name, group: null, path: [], web_url: null, latest };
}

const ROWS = [
  pipeline("Build", [run("3", "feature/x", "run", "2026-08-22T11:00:00Z"), run("2", "main", "ok", "2026-08-22T10:00:00Z")]),
  pipeline("Tests", [run("6", "refs/heads/feature/x", "err", "2026-08-22T09:30:00Z"), run("5", "main", "ok", "2026-08-22T09:00:00Z")]),
  pipeline("Lint", [run("8", "main", "err", "2026-08-22T08:00:00Z"), run("9", "fix/y", "ok", "2026-08-22T08:30:00Z")]),
  pipeline("Deploy", [run("10", "main", "ok", "2026-08-22T07:00:00Z")]),
];

test("a run's branch is read as the repository names it: refs/heads/ is the source's spelling, not the name", () => {
  expect(latestOn(ROWS[1]!, "feature/x")?.entity_id).toBe("6");
  expect(latestOn(ROWS[1]!, "refs/heads/feature/x")).toBeNull();
});

test("a ref that is no branch is not one of the rail's, and a cancelled run fixed nothing", () => {
  const pulled = pipeline("Pr", [run("20", "refs/pull/12/head", "ok", "2026-08-22T12:00:00Z")]);
  expect(railOf([pulled], ["main"]).map((b) => b.name)).toEqual(["main"]);
  const cancelled = { ...run("21", "fix/z", "ok", "2026-08-22T12:00:00Z"), state: { text: "cancelled", tone: "muted" as const } };
  const broken = pipeline("Broken", [cancelled, run("22", "main", "err", "2026-08-22T11:00:00Z")]);
  expect(verdictOf(broken, "fix/z", "main")).toBe("pass");
});

test("rows keep the place they first drew: a later order moves nothing drawn, and a new row joins the end", () => {
  const held = new Map<string, number>();
  expect(holdOrder(held, ["b", "a"], (x) => x)).toEqual(["b", "a"]);
  expect(holdOrder(held, ["a", "c", "b"], (x) => x)).toEqual(["b", "a", "c"]);
});

test("against the default branch: a failure only here, one main shares, one fixed here, what still runs, and a pass", () => {
  const [build, tests, lint, deploy] = ROWS;
  expect(verdictOf(tests!, "feature/x", "main")).toBe("only");
  expect(verdictOf(build!, "feature/x", "main")).toBe("running");
  expect(verdictOf(lint!, "fix/y", "main")).toBe("fixed");
  expect(verdictOf(lint!, "main", "main")).toBe("only");
  expect(verdictOf(deploy!, "main", "main")).toBe("pass");
  // A pipeline that never ran on the branch has no verdict there.
  expect(verdictOf(deploy!, "feature/x", "main")).toBeNull();
  const shared = pipeline("Shared", [run("11", "feature/z", "err", "2026-08-22T06:00:00Z"), run("12", "main", "err", "2026-08-22T05:00:00Z")]);
  expect(verdictOf(shared, "feature/z", "main")).toBe("also");
});

test("a branch's page leads with what fails only there, then what still runs, then the rest; what never ran there is apart", () => {
  const page = compareOn(ROWS, "feature/x", "main");
  expect(page.ran.map(({ row, verdict }) => [row.name, verdict])).toEqual([
    ["Tests", "only"],
    ["Build", "running"],
  ]);
  expect(page.notRun.map((row) => row.name)).toEqual(["Lint", "Deploy"]);
});

test("the rail: every branch the repository lists or a run names, each with the worst of its latest runs", () => {
  const rail = railOf(ROWS, ["main", "feature/x", "chore/quiet"]);
  const seen = rail.map((b) => [b.name, b.tone, b.failing]);
  expect(seen).toEqual([
    ["main", "err", 1],
    ["feature/x", "err", 1],
    ["chore/quiet", "none", 0],
    ["fix/y", "ok", 0],
  ]);
  // A branch with runs says when its newest ran, for the order of its group.
  expect(rail.find((b) => b.name === "feature/x")?.lastRun).toBe(Date.parse("2026-08-22T11:00:00Z"));
});

test("the rail's groups: the default branch, yours, then every other by its newest run; an empty group is left out", () => {
  const groups = groupBranches(railOf(ROWS, ["main", "feature/x", "chore/quiet"]), "main", ["fix/y"]);
  expect(groups.map((g) => [g.head, g.branches.map((b) => b.name)])).toEqual([
    ["Default branch", ["main"]],
    ["Yours", ["fix/y"]],
    ["Other branches", ["feature/x", "chore/quiet"]],
  ]);
  expect(groupBranches(railOf(ROWS, ["main"]), "main", []).map((g) => g.head)).toEqual(["Default branch", "Other branches"]);
});

test("yours: the branch the checkout is on, and every branch naming the job, whatever its case", () => {
  expect(yoursOf(["main", "feat/PAY-231-retry", "pay-231-docs", "fix/y"], "PAY-231", "fix/y")).toEqual(["fix/y", "feat/PAY-231-retry", "pay-231-docs"]);
  expect(yoursOf(["main", "fix/y"], null, null)).toEqual([]);
  // The checkout on the default branch is not a branch of yours.
  expect(yoursOf(["main"], null, "main", "main")).toEqual([]);
});

test("a branch an open pull request is opened from is in Pull requests, unless it is yours", () => {
  const rail = railOf(ROWS, ["main", "feature/x", "chore/quiet"]);
  expect(groupBranches(rail, "main", ["fix/y"], ["feature/x", "fix/y"]).map((g) => [g.head, g.branches.map((b) => b.name)])).toEqual([
    ["Default branch", ["main"]],
    ["Yours", ["fix/y"]],
    ["Pull requests", ["feature/x"]],
    ["Other branches", ["chore/quiet"]],
  ]);
});

test("a branch's builds in words: what failed of how many, else what runs, else that all passed", () => {
  const words = Object.fromEntries(railOf(ROWS, ["main", "feature/x", "chore/quiet", "fix/y"]).map((b) => [b.name, branchWords(b)]));
  expect(words).toEqual({ main: "1 of 4 failing", "feature/x": "1 of 2 failing", "chore/quiet": "No builds", "fix/y": "All 1 passed" });
  const running = railOf([ROWS[0]!], ["feature/x"]).find((b) => b.name === "feature/x")!;
  expect(branchWords(running)).toBe("1 running");
});
