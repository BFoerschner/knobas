/**
 * The run Overview's *Chain on <branch>* panel (#857, R-9.13) over the
 * `run_chain` read: a box per step, the run on screen outlined, each opening
 * its run or its pipeline, and nothing for a run in no chain.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ChainStepRow, RunRow } from "../../ipc/entity";
import TfRunChain from "./TfRunChain.svelte";

let target: HTMLElement;
let panel: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (panel) unmount(panel);
  panel = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 4; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const FEATURE = "feature/PAY-231-sepa-retry";
const P = (id: string) => `teamcity:buildType:Payout_${id}`;

function run(no: number, tone: "ok" | "err"): RunRow {
  return {
    entity_id: `teamcity:build:${no}`,
    number: String(no),
    state: { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch: FEATURE,
    started: "2026-10-16T08:00:00Z",
    finished: "2026-10-16T08:04:00Z",
    duration_seconds: 240,
    updated_at: "2026-10-16T08:04:00Z",
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
  };
}

const CHAIN: ChainStepRow[] = [
  { pipeline_id: P("Build"), name: "Payout_Build", run: run(1186, "ok") },
  { pipeline_id: P("IntegrationTests"), name: "Payout_IntegrationTests", run: run(1187, "err") },
  { pipeline_id: P("Deploy_Staging"), name: "Payout_Deploy_Staging", run: null },
];

function draw(answer: ChainStepRow[] | Error, runId = "teamcity:build:1187", branch: string | null = FEATURE): string[] {
  const asked: string[] = [];
  panel = mount(TfRunChain, {
    target,
    props: {
      runId,
      branch,
      read: (id: string) => {
        asked.push(id);
        return answer instanceof Error ? Promise.reject(answer) : Promise.resolve(answer);
      },
    },
  });
  return asked;
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim();

test("a chained run shows each step's run on its branch, its own outlined", async () => {
  const asked = draw(CHAIN);
  await settle();
  expect(asked).toEqual(["teamcity:build:1187"]);
  expect(text(target.querySelector(".tf-chain h3"))).toBe(`Chain on ${FEATURE}`);
  const boxes = [...target.querySelectorAll<HTMLAnchorElement>(".tf-chainrow a")];
  expect(boxes.map((a) => text(a))).toEqual([
    "Payout_Build#1186 passed",
    "Payout_IntegrationTests#1187 FAILURE",
    "Payout_Deploy_Stagingnot run on this branch",
  ]);
  expect(boxes.map((a) => a.getAttribute("href"))).toEqual([
    "#/build/teamcity%3AbuildType%3APayout_Build/teamcity%3Abuild%3A1186",
    "#/build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3A1187",
    "#/builds/teamcity%3AbuildType%3APayout_Deploy_Staging",
  ]);
  expect(boxes.map((a) => a.getAttribute("aria-current"))).toEqual([null, "page", null]);
  expect(boxes.map((a) => a.className)).toEqual(["tf-link tf-ok", "tf-link tf-err", "tf-link tf-none"]);
  expect(target.querySelectorAll(".tf-chainrow .tf-arrow")).toHaveLength(2);
  expect(text(target.querySelector(".tf-chain > .tf-dim"))).toBe(
    "Each step starts when the one before it passes. Run… on any step runs the steps before it too.",
  );
});

test("a run in no chain, or whose chain could not be read, draws no panel", async () => {
  draw([]);
  await settle();
  expect(target.querySelector(".tf-chain")).toBeNull();
  unmount(panel!);
  panel = null;
  draw(new Error("connection refused"));
  await settle();
  expect(target.querySelector(".tf-chain")).toBeNull();
});
