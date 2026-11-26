/**
 * Build chains (#857, R-9.13) on plain data: the pipeline header's sentence
 * and the run page's boxes.
 */
import { expect, test } from "vitest";

import type { ChainStep, ChainStepRow, RunRow } from "../../ipc/entity";
import { chainBox, chainPlace, chainSentence, chainWindow, repositoryShortName } from "./chain";

const P = (id: string) => `teamcity:buildType:Payout_${id}`;
const CHAIN: ChainStep[] = [
  { pipeline: P("Build"), name: "Payout_Build" },
  { pipeline: P("Docker"), name: "Payout_Docker" },
  { pipeline: P("Deploy_Staging"), name: "Payout_Deploy_Staging" },
];

function run(no: number | null, tone: "ok" | "err" | "run" | "q"): RunRow {
  const done = tone === "ok" || tone === "err";
  return {
    entity_id: `teamcity:build:${no ?? "q"}`,
    number: no === null ? null : String(no),
    state: done ? { text: tone === "err" ? "FAILURE" : "SUCCESS", tone } : null,
    branch: "feature/PAY-231-sepa-retry",
    started: tone === "q" ? null : "2026-10-16T08:00:00Z",
    finished: done ? "2026-10-16T08:04:00Z" : null,
    duration_seconds: done ? 240 : null,
    updated_at: "2026-10-16T08:04:00Z",
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

test("the header names the pipeline's step and the whole chain in running order", () => {
  expect(chainSentence(CHAIN, P("Docker"))).toBe(
    "Step 2 of the chain Payout_Build › Payout_Docker › Payout_Deploy_Staging.",
  );
  expect(chainSentence(CHAIN, P("Build"))).toBe(
    "Step 1 of the chain Payout_Build › Payout_Docker › Payout_Deploy_Staging.",
  );
});

test("a pipeline in no chain, or one the chain does not name, says nothing", () => {
  expect(chainSentence([], P("Docker"))).toBe("");
  expect(chainSentence(CHAIN, P("Elsewhere"))).toBe("");
  expect(chainSentence([CHAIN[1]!], P("Docker"))).toBe("");
});

test("a step's box opens its run, says its number and result, and marks the run on screen", () => {
  const step = (r: RunRow | null): ChainStepRow => ({ pipeline_id: P("Docker"), name: "Payout_Docker", run: r });
  expect(chainBox(step(run(88, "ok")), "teamcity:build:90")).toEqual({
    name: "Payout_Docker",
    href: "#/build/teamcity%3AbuildType%3APayout_Docker/teamcity%3Abuild%3A88",
    line: "#88 passed",
    tone: "ok",
    here: false,
  });
  const failed = chainBox(step(run(90, "err")), "teamcity:build:90");
  expect([failed.line, failed.tone, failed.here]).toEqual(["#90 FAILURE", "err", true]);
  const running = chainBox(step(run(91, "run")), "teamcity:build:90");
  expect([running.line, running.tone]).toEqual(["#91 running", "run"]);
  const queued = chainBox(step(run(null, "q")), "teamcity:build:90");
  expect([queued.line, queued.tone]).toEqual(["queued", "q"]);
});

test("a step that has not run on the branch opens its pipeline", () => {
  expect(chainBox({ pipeline_id: P("Deploy_Staging"), name: "Payout_Deploy_Staging", run: null }, "x")).toEqual({
    name: "Payout_Deploy_Staging",
    href: "#/builds/teamcity%3AbuildType%3APayout_Deploy_Staging",
    line: "not run on this branch",
    tone: "none",
    here: false,
  });
});

test("the header's sign says the step and how many there are, nothing outside a chain", () => {
  expect(chainPlace(CHAIN, P("Docker"))).toEqual({ step: 2, of: 3 });
  expect(chainPlace(CHAIN, P("Elsewhere"))).toBeNull();
  expect(chainPlace([CHAIN[1]!], P("Docker"))).toBeNull();
});

test("a chain of up to seven steps draws every box; a longer one folds its middle around the run's own", () => {
  expect(chainWindow(7, 3)).toEqual([0, 1, 2, 3, 4, 5, 6]);
  // Step 64 of 70: the first, 62 to 66 around it, the last.
  expect(chainWindow(70, 63)).toEqual([0, null, 61, 62, 63, 64, 65, null, 69]);
  // Near an end, a gap of one step draws the step, not a fold.
  expect(chainWindow(10, 3)).toEqual([0, 1, 2, 3, 4, 5, null, 9]);
  expect(chainWindow(10, 0)).toEqual([0, 1, 2, null, 9]);
  expect(chainWindow(10, 9)).toEqual([0, null, 7, 8, 9]);
  // No step of the run's own: the first steps and the last.
  expect(chainWindow(10, -1)).toEqual([0, 1, 2, 3, 4, null, 9]);
});

test("a repository named by its address is its own name; any other name is itself", () => {
  expect(repositoryShortName("https://git.example.com/org/very-long-repo.git#refs/heads/main")).toBe("very-long-repo");
  expect(repositoryShortName("ssh://git@git.example.com:7999/team/sub/repo.git")).toBe("repo");
  expect(repositoryShortName("git@github.com:acme/widgets.git")).toBe("widgets");
  expect(repositoryShortName("http://gitea.example/tidewater/payout-service/")).toBe("payout-service");
  expect(repositoryShortName("https://git.example.com/")).toBe("https://git.example.com/");
  expect(repositoryShortName("payout-service")).toBe("payout-service");
  expect(repositoryShortName("Payouts / main repo")).toBe("Payouts / main repo");
});
