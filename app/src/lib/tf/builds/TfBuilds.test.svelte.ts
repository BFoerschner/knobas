/**
 * Builds (#848) over the IPC mirror: the pipeline list on the left with its
 * filter and chips, the selected pipeline's runs on the right, and the
 * source's word in one header line only.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { JobPipeline, PipelineRow, RunRow } from "../../ipc/entity";
import * as entityIpc from "../../ipc/entity";
import TfBuilds, { type BuildsPorts } from "./TfBuilds.svelte";

// Only the default *This job* read (#1537) reaches these; the other tests pass `ports.jobPipelines`.
vi.mock("../../ipc/sources", async (original) => ({
  ...(await original<typeof import("../../ipc/sources")>()),
  listSources: vi.fn(async () => [{ id: "jira" }, { id: "tidewater" }]),
  refreshEntity: vi.fn(async () => ({ outcome: "refreshed" })),
}));
vi.mock("../../ipc/entity", async (original) => ({
  ...(await original<typeof import("../../ipc/entity")>()),
  // The run and pipeline pages read their own item too, for *Refresh* (#2137).
  getEntity: vi.fn(async (id: string) => ({
    row: { entity_id: id, kind: "ticket", title: id },
    source: { id: "tidewater", display_name: "Tidewater CI", adapter_kind: "teamcity", enabled: true },
    refreshes: true,
    deleted_at: null,
    links: [],
  })),
  jobPipelines: vi.fn(async () => []),
}));

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

function pipeline(id: string, name: string, recent: RunRow[]): PipelineRow {
  return {
    entity_id: `teamcity:buildType:${id}`,
    source_id: "teamcity",
    source_name: "TeamCity",
    name,
    group: "Payout",
    path: [],
    pinned: false,
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

function draw(rest: string[], head = "builds", asked: string[] = [], extra: Partial<BuildsPorts> = {}) {
  const ports: BuildsPorts = {
    ...extra,
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

test("the source's word appears once in the list, as the header line over its pipelines", async () => {
  draw([]);
  await settle();
  expect(target.querySelector(".tf-pl h3")?.textContent).toBe("TeamCity");
  // The pipeline page's header names it again, as prototype-6's does (#851).
  expect(target.querySelector(".tf-pl")?.textContent?.split("TeamCity")).toHaveLength(2);
});

test("a run's address draws the run page, with its pipeline selected in the list", async () => {
  draw(["teamcity:buildType:Payout_Lint", "teamcity:build:904"], "build");
  await settle();
  expect(target.querySelector(".tf-bmain h1")?.textContent).toContain("Payout_Lint #904");
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Lint");
});

test("a run's address without its pipeline moves to the full address of the pipeline that ran it", async () => {
  try {
    draw(["teamcity:build:904"], "build");
    await settle();
    expect(location.hash).toBe("#/build/teamcity%3AbuildType%3APayout_Lint/teamcity%3Abuild%3A904");
  } finally {
    location.hash = "";
  }
});

test("a run's address no pipeline ran says so", async () => {
  draw(["teamcity:build:1"], "build", [], { view: () => Promise.resolve(null) });
  await settle();
  expect(target.querySelector(".tf-bmain .tf-empty")?.textContent).toContain("No run at this address.");
});

test("no pipelines says what would fill the list", async () => {
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: [], job: null, ports: { pipelines: () => Promise.resolve([]), runs: () => Promise.resolve([]) } },
  });
  await settle();
  expect(target.querySelector(".tf-pl-note")?.textContent).toContain("No pipelines yet");
});

// -- the project tree and pins (#849), *This job* (#850) ----------------------

const TREE: PipelineRow[] = [
  { ...pipeline("Dbt_Test", "Dbt_Test", [run("teamcity:build:640", "err")]), path: ["Data"] },
  { ...pipeline("Payout_Build", "Payout_Build", [run("teamcity:build:1188", "ok")]), path: ["Payments", "Payouts"], pinned: true },
  {
    ...pipeline("Payout_Deploy_Prod", "Payout_Deploy_Prod", [run("teamcity:build:77", "err")]),
    path: ["Payments", "Payouts", "Deploy"],
  },
  { ...pipeline("Payout_Lint", "Payout_Lint", [run("teamcity:build:904", "ok")]), path: ["Payments", "Payouts"] },
  { ...pipeline("Terraform_Plan", "Terraform_Plan", [run("teamcity:build:1402", "ok")]), path: ["Platform", "Infra"] },
];

function drawTree(
  options: { job?: string | null; rest?: string[]; reasons?: JobPipeline[]; pins?: [string, boolean][] } = {},
) {
  let list = TREE.map((p) => ({ ...p }));
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve(list),
    runs: (id) => Promise.resolve(list.find((p) => p.entity_id === id)?.recent ?? []),
    setPinned: (id, pinned) => {
      options.pins?.push([id, pinned]);
      list = list.map((p) => (p.entity_id === id ? { ...p, pinned } : p));
      return Promise.resolve(pinned);
    },
    jobPipelines: () => Promise.resolve(options.reasons ?? []),
  };
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: options.rest ?? [], job: options.job ?? null, ports },
  });
}

const summaries = () =>
  [...target.querySelectorAll<HTMLElement>(".tf-pl-fold > summary")].map((s) =>
    [s.firstChild?.textContent?.trim(), s.querySelector(".tf-count")?.textContent, s.querySelector(".tf-chip")?.textContent ?? ""].join("|"),
  );
const fold = (name: string) =>
  [...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].find((d) => d.querySelector(":scope > summary")?.textContent?.startsWith(name))!;

test("All: Pinned first, then the tree as deep as it goes, with counts, failures and single children as one row", async () => {
  drawTree();
  await settle();
  const headers = [...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent);
  expect(headers).toEqual(["Pinned", "TeamCity"]);
  const pinned = target.querySelector(".tf-pl h3 + .tf-pi .tf-pi-nm")?.textContent;
  expect(pinned).toBe("Payout_Build");
  expect(summaries()).toEqual(["Data|1|1 failing", "Payments › Payouts|3|1 failing", "Deploy|1|1 failing", "Platform › Infra|1|"]);
  // Own pipelines before the subgroup.
  const payouts = fold("Payments › Payouts");
  const inner = [...payouts.querySelectorAll(":scope > .tf-fold-in > *")].map((el) =>
    el.classList.contains("tf-pi") ? el.querySelector(".tf-pi-nm")?.textContent : "fold",
  );
  expect(inner).toEqual(["Payout_Build", "Payout_Lint", "fold"]);
  // Open on a failure (and the selection, the first failing one); closed otherwise.
  expect(fold("Data").open).toBe(true);
  expect(fold("Platform › Infra").open).toBe(false);
  expect(target.querySelector<HTMLAnchorElement>('.tf-pi[title="Payments › Payouts › Deploy › Payout_Deploy_Prod"]')).not.toBeNull();
});

test("a fold the reader closes stays closed as the list redraws", async () => {
  drawTree();
  await settle();
  const data = fold("Data");
  data.open = false;
  data.dispatchEvent(new Event("toggle"));
  flushSync();
  const field = target.querySelector<HTMLInputElement>("[data-filter]")!;
  field.value = "dbt";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  field.value = "";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(fold("Data").open).toBe(false);
});

test("Pin and Unpin on the selected pipeline write the pin and move it in and out of Pinned", async () => {
  const pins: [string, boolean][] = [];
  drawTree({ rest: ["teamcity:buildType:Payout_Lint"], pins });
  await settle();
  const button = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-fhead button")].find((b) => /Pin|Unpin/.test(b.textContent ?? ""))!;
  expect(button().textContent).toBe("Pin");
  button().click();
  await settle();
  expect(pins).toEqual([["teamcity:buildType:Payout_Lint", true]]);
  expect(button().textContent).toBe("Unpin");
  const pinnedNames = () =>
    [...target.querySelectorAll(".tf-pl-list > .tf-pi .tf-pi-nm")].map((n) => n.textContent);
  expect(pinnedNames()).toEqual(["Payout_Build", "Payout_Lint"]);
  button().click();
  await settle();
  expect(pinnedNames()).toEqual(["Payout_Build"]);
});

const ON_JOB: JobPipeline[] = [
  {
    pipeline_id: "teamcity:buildType:Payout_Build",
    reasons: [
      { kind: "branch", branch: "feature/PAY-231-sepa-retry" },
      { kind: "pr_check", pr: "gitea:svc#142" },
    ],
  },
  { pipeline_id: "teamcity:buildType:Payout_Deploy_Prod", reasons: [{ kind: "pr_check", pr: "gitea:svc#142" }] },
];

test("This job is on by default with a job: the tree pruned to its pipelines, each with its reasons", async () => {
  drawTree({ job: "PAY-231", reasons: ON_JOB });
  await settle();
  const scope = [...target.querySelectorAll<HTMLButtonElement>(".tf-pl .tf-seg button")];
  expect(scope.map((b) => [b.textContent, b.getAttribute("aria-pressed")])).toEqual([
    ["This job PAY-231", "true"],
    ["All", "false"],
  ]);
  expect(target.querySelector(".tf-scope-note")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "2 of 5 pipelines are relevant to PAY-231. Show all",
  );
  expect([...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent)).toEqual(["By project"]);
  expect(names()).toEqual(["Payout_Build", "Payout_Deploy_Prod"]);
  expect(rows()[0]!.querySelector(".tf-pi-why")?.textContent).toBe("your branch, PR check");
  // Every fold is open while the job narrows the list.
  expect([...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].every((d) => d.open)).toBe(true);
  // A bare #/builds opens on the job's failing pipeline.
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Deploy_Prod");

  target.querySelector<HTMLButtonElement>(".tf-scope-note button")!.click();
  flushSync();
  expect(names()).toHaveLength(6);
  expect(target.querySelector(".tf-scope-note")).toBeNull();
  expect(target.querySelector(".tf-pi-why")).toBeNull();
});

test("a job with no pipelines says so and offers to search all of them", async () => {
  drawTree({ job: "PAY-999", reasons: [] });
  await settle();
  const note = [...target.querySelectorAll(".tf-pl-note")].map((n) => n.textContent?.replace(/\s+/g, " ").trim());
  expect(note).toEqual(["No pipeline matches. Search all pipelines"]);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-pl-note button")][0]!.click();
  flushSync();
  expect(target.querySelectorAll(".tf-pl-fold").length).toBeGreaterThan(0);
});

test("Builds draws every pipeline the read answers and drops none by the source list (#1295)", async () => {
  const relay: PipelineRow = {
    ...pipeline("Rocket_Build", "Rocket_Build", [run("relay:run:7", "ok")]),
    entity_id: "relay:pipeline:Rocket_Build",
    source_id: "relay",
    source_name: "Relay CI",
  };
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve([...LIST, relay]),
    runs: (id) => Promise.resolve([...LIST, relay].find((p) => p.entity_id === id)?.recent ?? []),
  };
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: [], job: null, ports } });
  await settle();

  expect(names()).toHaveLength(LIST.length + 1);
  expect(names()).toContain("Rocket_Build");
});

test("This job reads the pipelines of the ticket the reader opened, not the first source's same key (#1537)", async () => {
  page = mount(TfBuilds, {
    target,
    props: {
      ready: true,
      head: "builds",
      rest: [],
      job: "PAY-231",
      jobAt: "tidewater:PAY-231",
      ports: { pipelines: () => Promise.resolve(LIST), runs: () => Promise.resolve([]) },
    },
  });
  await settle();
  expect(entityIpc.jobPipelines).toHaveBeenCalledWith("tidewater:PAY-231");
  expect(entityIpc.jobPipelines).not.toHaveBeenCalledWith("jira:PAY-231");
});

test("/ focuses the pipeline filter, and the runs filter beside it draws no hint (#1453)", async () => {
  draw(["teamcity:buildType:Payout_IntegrationTests"]);
  await settle();
  const fields = [...target.querySelectorAll<HTMLInputElement>("[data-filter]")];
  expect(fields).toHaveLength(2);
  const runs = target.querySelector(".tf-runbar .tf-fbar")!;
  expect(runs.querySelector("kbd")).toBeNull();
  const event = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(fields[0]);
  expect(runs.contains(document.activeElement)).toBe(false);
});

test("Refresh on a pipeline reads the list again, and its row as the read answers it (#2137)", async () => {
  let reads = 0;
  let list = LIST;
  const ports: BuildsPorts = {
    pipelines: () => (reads++, Promise.resolve(list)),
    runs: (id) => Promise.resolve(LIST.find((p) => p.entity_id === id)?.recent ?? []),
  };
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: [LIST[1]!.entity_id], job: null, ports } });
  await settle();
  const before = reads;
  list = LIST.map((p, at) => (at === 1 ? { ...p, name: "Payout_IntegrationTests_v2" } : p));
  target.querySelector<HTMLButtonElement>(".tf-bhead .tf-refresh")!.click();
  await settle();
  expect(reads).toBe(before + 1);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("Payout_IntegrationTests_v2");
});
