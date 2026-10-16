/**
 * Build chains (#857, R-9.13) on plain data: the pipeline header's sentence
 * and the run page's boxes.
 */
import { expect, test } from "vitest";

import type { ChainStep, ChainStepRow, RunRow } from "../../ipc/entity";
import { chainBox, chainSentence } from "./chain";

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
