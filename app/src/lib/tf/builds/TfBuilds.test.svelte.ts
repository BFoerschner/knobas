/**
 * Builds (#848) over the IPC mirror: the pipeline list on the left with its
 * filter and chips, the selected pipeline's runs on the right, and the
 * source's word in one header line only.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { PipelineRow, RunRow } from "../../ipc/entity";
import { forgetFilters } from "../filter/filter-state.svelte";
import TfBuilds, { type BuildsPorts } from "./TfBuilds.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
  forgetFilters();
});

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function run(id: string, tone: "ok" | "err", extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: id,
    number: id.split(":").at(-1) ?? null,
    state: { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch: "main",
    started: "2026-08-22T10:05:00Z",
    finished: "2026-08-22T10:09:12Z",
    duration_seconds: 252,
    updated_at: "2026-08-22T10:09:12Z",
    web_url: null,
    ...extra,
  };
}

function pipeline(id: string, name: string, recent: RunRow[]): PipelineRow {
  return {
    entity_id: `teamcity:buildType:${id}`,
    source_id: "teamcity",
    source_name: "TeamCity",
    name,
    group: "Payout",
    web_url: null,
    recent,
  };
}

const LIST: PipelineRow[] = [
  pipeline("Payout_Build", "Payout_Build", [
    run("teamcity:build:1188", "ok", { finished: null, duration_seconds: null, state: { text: "running", tone: "info" } }),
  ]),
  pipeline(
    "Payout_IntegrationTests",
    "Payout_IntegrationTests",
    Array.from({ length: 14 }, (_, i) => run(`teamcity:build:${1187 - i}`, i % 2 === 0 ? "err" : "ok")),
  ),
  pipeline("Payout_Lint", "Payout_Lint", [run("teamcity:build:904", "ok")]),
];

function draw(rest: string[], head = "builds", asked: string[] = []) {
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve(LIST),
    runs: (id) => {
      asked.push(id);
      return Promise.resolve(LIST.find((p) => p.entity_id === id)?.recent ?? []);
    },
  };
  page = mount(TfBuilds, { target, props: { ready: true, head, rest, job: null, ports } });
}

const rows = () => [...target.querySelectorAll<HTMLAnchorElement>(".tf-pi")];
const names = () => rows().map((row) => row.querySelector(".tf-pi-nm")?.textContent);

test("each pipeline is a row: its name, err when its latest run failed, a strip of the last twelve runs, its path on hover", async () => {
  draw([]);
  await settle();
  expect(names()).toEqual(["Payout_Build", "Payout_IntegrationTests", "Payout_Lint"]);
  const tests = rows()[1]!;
  expect(tests.querySelector(".tf-pi-nm")?.classList.contains("tf-bad")).toBe(true);
  expect(rows()[2]!.querySelector(".tf-pi-nm")?.classList.contains("tf-bad")).toBe(false);
  const blocks = tests.querySelectorAll(".tf-strip i");
  expect(blocks).toHaveLength(12);
  // The newest run, failed, is the right-most block.
  expect(blocks[11]?.className).toBe("tf-run-err");
  expect(rows()[0]!.querySelector(".tf-strip i")?.className).toBe("tf-run-run");
  expect(tests.title).toBe("Payout › Payout_IntegrationTests");
  expect(tests.getAttribute("href")).toBe("#/builds/teamcity%3AbuildType%3APayout_IntegrationTests");
  expect(target.querySelector<HTMLInputElement>("[data-filter]")?.placeholder).toBe("Filter 3 pipelines");
});

test("a bare #/builds selects the first failing pipeline and lists its runs newest first", async () => {
  const asked: string[] = [];
  draw([], "builds", asked);
  await settle();
  const current = target.querySelector('.tf-pi[aria-current="true"]');
  expect(current?.querySelector(".tf-pi-nm")?.textContent).toBe("Payout_IntegrationTests");
  expect(asked).toEqual(["teamcity:buildType:Payout_IntegrationTests"]);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("Payout_IntegrationTests");
  const first = target.querySelector(".tf-runs tbody tr");
  expect(first?.textContent).toContain("#1187");
  expect(first?.textContent).toContain("FAILURE");
  expect(first?.textContent).toContain("4 m 12 s");
  expect(target.querySelectorAll(".tf-runs tbody tr")).toHaveLength(14);
});

test("the address's entity id selects that pipeline", async () => {
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Lint");
  expect(target.querySelector(".tf-runs tbody tr")?.textContent).toContain("#904");
});

test("the filter keeps what its words match, and the chips keep the failing or the running ones", async () => {
  draw([]);
  await settle();
  const field = target.querySelector<HTMLInputElement>("[data-filter]")!;
  field.value = "lint";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(names()).toEqual(["Payout_Lint"]);
  expect(target.querySelector(".tf-pi mark")?.textContent).toBe("Lint");

  field.value = "";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  const chip = (word: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === word)!;
  chip("Failing").click();
  flushSync();
  expect(names()).toEqual(["Payout_IntegrationTests"]);
  expect(chip("Failing").getAttribute("aria-pressed")).toBe("true");
  chip("Failing").click();
  chip("Running").click();
  flushSync();
  expect(names()).toEqual(["Payout_Build"]);
});

test("the source's word appears once, as the header line over its pipelines", async () => {
  draw([]);
  await settle();
  expect(target.querySelector(".tf-pl h3")?.textContent).toBe("TeamCity");
  expect(target.textContent?.split("TeamCity")).toHaveLength(2);
});

test("a run's address says the run page is not built yet, and leads back to its pipeline", async () => {
  draw(["teamcity:buildType:Payout_Lint", "teamcity:build:904"], "build");
  await settle();
  expect(target.querySelector(".tf-bmain .tf-empty")?.textContent).toContain("not built in the new shell yet");
  expect(target.querySelector<HTMLAnchorElement>(".tf-bmain .tf-empty a")?.getAttribute("href")).toBe(
    "#/builds/teamcity%3AbuildType%3APayout_Lint",
  );
  expect(target.querySelector('.tf-pi[aria-current="true"]')).toBeNull();
});

test("no pipelines says what would fill the list", async () => {
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: [], job: null, ports: { pipelines: () => Promise.resolve([]), runs: () => Promise.resolve([]) } },
  });
  await settle();
  expect(target.querySelector(".tf-pl-note")?.textContent).toContain("No pipelines yet");
});
