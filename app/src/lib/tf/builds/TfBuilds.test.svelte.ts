/**
 * Builds (#848) over the IPC mirror: the pipeline list on the left with its
 * filter and chips, the selected pipeline's runs on the right, and the
 * source's word in one header line only.
 */
import { layers as sharedLayers } from "../layers.svelte";
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { JobPipeline, RailPipeline, RunRow } from "../../ipc/entity";
import * as entityIpc from "../../ipc/entity";
import { sessionFilter, sessionValue } from "../filter/filter-state.svelte";
import TfBuilds, { type BuildsPorts } from "./TfBuilds.svelte";
import { runsRead } from "./runs-read.svelte";
import { thingAt } from "../context/thing";
import { toasts } from "../../shell/toasts.svelte";
import { echoEvents } from "../echo-events.fixture";
import { echoWrite, forgetEchoes } from "../queued.svelte";
import { QUEUED_RUN } from "./pipeline";
import { failedRun, railOf } from "./builds";

// Only the default *This job* read (#1537) reaches these; the other tests pass `ports.jobPipelines`.
vi.mock("../../ipc/sources", async (original) => ({
  ...(await original<typeof import("../../ipc/sources")>()),
  listSources: vi.fn(async () => [{ id: "jira" }, { id: "tidewater" }]),
  refreshEntity: vi.fn(async () => ({ outcome: "refreshed" })),
}));
vi.mock("../../ipc/entity", async (original) => ({
  ...(await original<typeof import("../../ipc/entity")>()),
  // The run and pipeline pages read their own item too, for *Refresh* (#2137).
  // A run's id answers a run (`build`), a pipeline's a pipeline (`build_config`), any other the ticket *This job* resolves.
  getEntity: vi.fn(async (id: string) =>
    (await import("../refresh.fixture")).detailOf(id, {}, id.includes(":build:") ? "build" : id.includes(":buildType:") ? "build_config" : "ticket"),
  ),
  jobPipelines: vi.fn(async () => []),
}));

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

const QUIET_RECENT = {
  recent: () => Promise.resolve({ runs: [], pipelines: [], next: null }),
  listen: () => Promise.resolve(() => {}),
  focused: () => false,
};

/** Frames asked for, run by `settle` (as `TfRecent`'s tests do): jsdom's own come on a 16 ms timer a loaded machine misses. */
let frames: FrameRequestCallback[] = [];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (run: FrameRequestCallback) => frames.push(run));
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
  runsRead.forget();
  // A new session: a bare #/builds opens *All pipelines* again (#2570).
  sessionValue("builds-place").value = null;
  sessionFilter("builds-group").scope = null;
  sessionValue("builds-group").value = null;
  vi.unstubAllGlobals();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
    // *All pipelines* hears its first rows painted a frame later, and the rail's list is read then (#2570).
    frames.splice(0).forEach((run) => run(performance.now()));
  }
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

/** Each fixture pipeline's whole runs, by entity id: what its page reads (`pipeline_runs`), not what the rail does. */
const RUNS = new Map<string, RunRow[]>();
const runsOf = (id: string): RunRow[] => RUNS.get(id) ?? [];

/**
 * A pipeline as the rail's read answers it (#2571): `recent` the runs of its last twelve the rail reads, `commits` what
 * they built, `unread` its failures (the backend's are over the whole history; here, those among `recent`).
 */
function pipeline(id: string, name: string, recent: RunRow[]): RailPipeline {
  RUNS.set(`teamcity:buildType:${id}`, recent);
  return railOf({
    entity_id: `teamcity:buildType:${id}`,
    source_id: "teamcity",
    source_name: "TeamCity",
    name,
    group: "Payout",
    path: [],
    pinned: false,
    web_url: null,
    recent,
    unread: recent.filter(failedRun).map((run) => run.entity_id),
  });
}

const LIST: RailPipeline[] = [
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

/** The role frame's *Sync now* (#2106), as `TfKeptPage` hands it. */
const syncNow = createRawSnippet(() => ({ render: () => '<button type="button" class="tf-btn tf-syncnow">Sync now</button>' }));

function draw(rest: string[], head = "builds", asked: string[] = [], extra: Partial<BuildsPorts> = {}, sync = false) {
  const ports: BuildsPorts = {
    // *All pipelines*' table answers nothing unless a test says (#2570): a bare #/builds draws it.
    recent: QUIET_RECENT,
    ...extra,
    pipelines: extra.pipelines ?? (() => Promise.resolve(LIST)),
    runs: (id) => {
      asked.push(id);
      return Promise.resolve(runsOf(id));
    },
  };
  page = mount(TfBuilds, { target, props: { ready: true, head, rest, job: null, ports, ...(sync ? { sync: syncNow } : {}) } });
}

const rows = () => [...target.querySelectorAll<HTMLAnchorElement>(".tf-pl-rows .tf-pi")];
const names = () => rows().map((row) => row.querySelector(".tf-pi-nm")?.textContent);

test("an open Pipeline fold is a panel: Esc folds it first, and focus returns to its bar (F-619)", async () => {
  draw([]);
  await settle();
  const before = sharedLayers.open.length;
  const bar = target.querySelector<HTMLButtonElement>(".tf-railbar button")!;
  bar.click();
  flushSync();
  expect(bar.getAttribute("aria-expanded")).toBe("true");
  expect(sharedLayers.open).toHaveLength(before + 1);
  sharedLayers.open.at(-1)!.close();
  flushSync();
  expect(bar.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(bar);
  expect(sharedLayers.open).toHaveLength(before);
});

test("each pipeline is one line: a glyph of its latest run, its name, err when that run failed, its path on hover", async () => {
  draw([]);
  await settle();
  expect(names()).toEqual(["Payout_Build", "Payout_IntegrationTests", "Payout_Lint"]);
  const [build, tests, lint] = rows();
  expect(tests!.querySelector(".tf-pi-nm")?.classList.contains("tf-bad")).toBe(true);
  expect(lint!.querySelector(".tf-pi-nm")?.classList.contains("tf-bad")).toBe(false);
  // No run strip: the latest run is one glyph, named for a reader that cannot see it.
  expect(target.querySelector(".tf-strip")).toBeNull();
  const glyph = (row: Element) => row.querySelector(".tf-rg")!;
  expect([build, tests, lint].map((row) => glyph(row!).getAttribute("aria-label"))).toEqual(["Running", "Failed", "Passed"]);
  expect(glyph(tests!).classList.contains("tf-rg-err")).toBe(true);
  // The path on hover, then the latest run's word.
  expect(tests!.title.split("\n")).toEqual(["Payout › Payout_IntegrationTests", expect.stringMatching(/^Failed /)]);
  expect(tests!.getAttribute("href")).toBe("#/builds/teamcity%3AbuildType%3APayout_IntegrationTests");
});

test("a row shows a time only where it says something, and dims the prefix its folder's pipelines share", async () => {
  draw([]);
  await settle();
  const [build, tests, lint] = rows();
  expect(tests!.querySelector(".tf-pi-when")?.classList.contains("tf-err")).toBe(true);
  expect(build!.querySelector(".tf-pi-when")?.classList.contains("tf-info")).toBe(true);
  // A pass is quiet: its time is in the title.
  expect(lint!.querySelector(".tf-pi-when")).toBeNull();
  expect(lint!.querySelector(".tf-pi-pre")?.textContent).toBe("Payout_");
  expect(lint!.querySelector(".tf-pi-nm")?.textContent).toBe("Payout_Lint");
});

test("a pipeline's address selects it and lists its runs newest first", async () => {
  const asked: string[] = [];
  draw(["teamcity:buildType:Payout_IntegrationTests"], "builds", asked);
  await settle();
  const current = target.querySelector('.tf-pi[aria-current="true"]');
  expect(current?.querySelector(".tf-pi-nm")?.textContent).toBe("Payout_IntegrationTests");
  expect(asked).toEqual(["teamcity:buildType:Payout_IntegrationTests"]);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("Payout_IntegrationTests");
  const first = target.querySelector(".tf-runs tbody tr");
  expect(first?.textContent).toContain("#1187");
  expect(first?.textContent).toContain("Failed");
  expect(first?.textContent).toContain("4m 12s");
  expect(target.querySelectorAll(".tf-runs tbody tr")).toHaveLength(14);
});

test("the address's entity id selects that pipeline", async () => {
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Lint");
  expect(target.querySelector(".tf-runs tbody tr")?.textContent).toContain("#904");
});

test("the chips keep the failing or the running ones", async () => {
  draw([]);
  await settle();
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

test("a filter that hides the pipeline you are on keeps it pinned over the list, saying why, until you leave it", async () => {
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  const pinned = () => target.querySelector<HTMLAnchorElement>(".tf-pl-pin a");
  expect(pinned()).toBeNull();
  const chip = (word: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === word)!;
  chip("Failing").click();
  flushSync();
  expect(names()).not.toContain("Payout_Lint");
  expect(pinned()?.getAttribute("aria-current")).toBe("true");
  expect(pinned()?.textContent).toContain("Payout_Lint");
  expect(pinned()?.textContent).toContain("Hidden by the filter");
  chip("Failing").click();
  flushSync();
  expect(pinned()).toBeNull();
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
  expect(target.querySelector(".tf-bmain .tf-empty")?.textContent).toContain("knobas does not hold this run.");
});

test("no pipelines says what would fill the list", async () => {
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: [], job: null, ports: { pipelines: () => Promise.resolve([]), runs: () => Promise.resolve([]) } },
  });
  await settle();
  expect(target.querySelector(".tf-pl-note")?.textContent).toContain("No pipelines yet");
});

// -- *Sync now* in the header (#2287) ------------------------------------------

/** The header's actions on the right of the Builds page, in their order. */
const headerActs = () => [...target.querySelectorAll<HTMLElement>(".tf-bmain .tf-fhead .tf-acts > *")];

test.each([
  ["a pipeline", "builds", ["teamcity:buildType:Payout_Lint"]],
  ["a run", "build", ["teamcity:buildType:Payout_Lint", "teamcity:build:904"]],
  ["no pipeline at the address", "builds", ["teamcity:buildType:Gone"]],
  ["no run at the address", "build", ["teamcity:build:1"]],
])("Sync now is last in the Builds header on %s, and not in the rail", async (_, head, rest) => {
  draw(rest, head, [], { view: () => Promise.resolve(null) }, true);
  await settle();
  expect(target.querySelectorAll(".tf-syncnow")).toHaveLength(1);
  expect(target.querySelector(".tf-pl .tf-syncnow")).toBeNull();
  expect(headerActs().at(-1)?.textContent).toBe("Sync now");
});

test("Sync now is in the header of All pipelines while no pipeline has synced yet", async () => {
  page = mount(TfBuilds, {
    target,
    props: {
      ready: true,
      head: "builds",
      rest: [],
      job: null,
      ports: { pipelines: () => Promise.resolve([]), runs: () => Promise.resolve([]), recent: QUIET_RECENT },
      sync: syncNow,
    },
  });
  await settle();
  expect(target.querySelector(".tf-pl .tf-syncnow")).toBeNull();
  expect(target.querySelector(".tf-bmain .tf-fhead h1")?.textContent).toBe("All pipelines");
  expect(target.querySelector(".tf-bmain")?.textContent, "No runs yet stands in the table's box").toContain("No runs yet.");
  expect(headerActs().at(-1)?.textContent).toBe("Sync now");
});

// -- the project tree and pins (#849), *This job* (#850) ----------------------

const TREE: RailPipeline[] = [
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
    runs: (id) => Promise.resolve(runsOf(id)),
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

/** Each fold's summary as its name, failing, running and total, as a reader hears them. */
const summaries = () =>
  [...target.querySelectorAll<HTMLElement>(".tf-pl-fold > summary")].map((s) =>
    [
      s.firstChild?.textContent?.trim(),
      s.querySelector(".tf-count.tf-err")?.textContent ?? "",
      s.querySelector(".tf-count.tf-info")?.textContent ?? "",
      s.querySelector(".tf-fold-n")?.textContent,
    ].join("|"),
  );
/** The `n` rows after a fold's summary, each as its indent and name. */
const after = (from: Element, n: number) => {
  const out: string[] = [];
  for (let at = from.nextElementSibling; at !== null && out.length < n; at = at.nextElementSibling) {
    const summary = at.querySelector(":scope > summary");
    const indent = [...(summary ?? at).classList].find((c) => /^tf-d\d$/.test(c)) ?? "tf-d0";
    out.push(`${indent} ${summary ? summary.firstChild?.textContent?.trim() : at.querySelector(".tf-pi-nm")?.textContent}`);
  }
  return out;
};
const fold = (name: string) =>
  [...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].find((d) => d.querySelector(":scope > summary")?.textContent?.startsWith(name))!;

test("All: Pinned first, then the tree as deep as it goes, with counts, failures and single children as one row", async () => {
  drawTree();
  await settle();
  const headers = [...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent);
  expect(headers).toEqual(["Pinned", "TeamCity"]);
  const pinned = target.querySelector(".tf-pl h3 + .tf-pi .tf-pi-nm")?.textContent;
  expect(pinned).toBe("Payout_Build");
  // A fold's rows follow its summary, one level deeper (#2211).
  expect(summaries()).toEqual([
    "Data|1 failing||1 pipeline",
    "Payments › Payouts|1 failing||3 pipelines",
    "Deploy|1 failing||1 pipeline",
    "Platform › Infra|||1 pipeline",
  ]);
  // Own pipelines before the subgroup.
  expect(after(fold("Payments › Payouts"), 3)).toEqual(["tf-d1 Payout_Build", "tf-d1 Payout_Lint", "tf-d1 Deploy"]);
  // Open on a failure (and the selection, the first failing one); closed otherwise.
  expect(fold("Data").open).toBe(true);
  expect(fold("Platform › Infra").open).toBe(false);
  expect(rows().some((row) => row.title.startsWith("Payments › Payouts › Deploy › Payout_Deploy_Prod\n"))).toBe(true);
});

test("a fold the reader closes stays closed as the list redraws", async () => {
  drawTree();
  await settle();
  const data = fold("Data");
  data.open = false;
  data.dispatchEvent(new Event("toggle"));
  flushSync();
  const running = [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!;
  running.click();
  flushSync();
  running.click();
  flushSync();
  expect(fold("Data").open).toBe(false);
});

test("Collapse all shuts every fold and Expand all opens them, and a redraw keeps what the reader chose", async () => {
  drawTree();
  await settle();
  const button = () => target.querySelector<HTMLButtonElement>(".tf-pl-tools .tf-trail > .tf-btn")!;
  const opened = () => [...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].map((d) => d.open);
  expect(button().textContent).toBe("Collapse all");
  button().click();
  flushSync();
  expect(opened().every((open) => !open)).toBe(true);
  expect(button().textContent).toBe("Expand all");
  button().click();
  flushSync();
  expect(opened().every((open) => open)).toBe(true);
  // Infra, closed by default, stays open through a chip's redraw: Expand all is the reader's own choice.
  const running = [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!;
  running.click();
  flushSync();
  running.click();
  flushSync();
  expect(fold("Platform › Infra").open).toBe(true);
});

test("the rail's keys are the docs tree's: Up and Down step, Home and End, Right opens then enters, Left shuts or goes up", async () => {
  drawTree();
  await settle();
  const key = (el: Element, k: string) => {
    const event = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true });
    el.dispatchEvent(event);
    return event.defaultPrevented;
  };
  const focused = () => document.activeElement?.querySelector(".tf-pi-nm, .tf-fold-nm")?.textContent;
  const data = fold("Data").querySelector<HTMLElement>(":scope > summary")!;
  data.focus();
  key(data, "ArrowDown");
  expect(focused()).toBe("Dbt_Test");
  // Left from a pipeline goes to its fold.
  key(document.activeElement!, "ArrowLeft");
  expect(document.activeElement).toBe(data);
  // Left shuts an open fold, Right opens it, and Right again steps into it.
  key(data, "ArrowLeft");
  flushSync();
  expect(fold("Data").open).toBe(false);
  expect(names()).not.toContain("Dbt_Test");
  const shut = fold("Data").querySelector<HTMLElement>(":scope > summary")!;
  key(shut, "ArrowRight");
  flushSync();
  expect(fold("Data").open).toBe(true);
  key(fold("Data").querySelector(":scope > summary")!, "ArrowRight");
  expect(focused()).toBe("Dbt_Test");
  // Home and End go to the ends; a step past the first row is taken, so the list does not scroll.
  key(document.activeElement!, "End");
  expect(focused()).toBe("Platform › Infra");
  key(document.activeElement!, "Home");
  expect(focused()).toBe("Payout_Build");
  expect(key(document.activeElement!, "ArrowUp")).toBe(true);
  expect(focused()).toBe("Payout_Build");
});

test("a closed fold draws nothing under it until the reader opens it, so a big CI draws only what is open (#2200's class)", async () => {
  drawTree();
  await settle();
  const infra = fold("Platform › Infra");
  expect(infra.open).toBe(false);
  expect(names()).not.toContain("Terraform_Plan");
  infra.open = true;
  infra.dispatchEvent(new Event("toggle"));
  flushSync();
  expect(after(fold("Platform › Infra"), 1)).toEqual(["tf-d1 Terraform_Plan"]);
  const again = fold("Platform › Infra");
  again.open = false;
  again.dispatchEvent(new Event("toggle"));
  flushSync();
  expect(names()).not.toContain("Terraform_Plan");
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
  // The rows under the *Pinned* heading, up to the next heading.
  const pinnedNames = () => {
    const out: (string | null | undefined)[] = [];
    const head = [...target.querySelectorAll(".tf-pl h3")].find((h) => h.textContent === "Pinned");
    for (let at = head?.nextElementSibling; at?.classList.contains("tf-pi"); at = at.nextElementSibling)
      out.push(at.querySelector(".tf-pi-nm")?.textContent);
    return out;
  };
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

test("a pipeline This job leaves out stays pinned over the list while you are on it, and says it is not on the job", async () => {
  const off = TREE.find((p) => !ON_JOB.some((row) => row.pipeline_id === p.entity_id))!;
  sessionFilter("builds-pipelines").scope = "job";
  drawTree({ job: "PAY-231", reasons: ON_JOB, rest: [off.entity_id] });
  await settle();
  const pinned = target.querySelector<HTMLAnchorElement>(".tf-pl-pin a");
  expect(pinned?.getAttribute("aria-current")).toBe("true");
  expect(pinned?.textContent).toContain(off.name);
  expect(pinned?.textContent).toContain("Not in PAY-231");
});

test("All is the default, as on every page; This job prunes the tree to its pipelines, each with its reasons", async () => {
  drawTree({ job: "PAY-231", reasons: ON_JOB });
  await settle();
  const scope = [...target.querySelectorAll<HTMLButtonElement>('.tf-pl .tf-seg[aria-label="Scope"] button')];
  expect(scope.map((b) => [b.textContent?.replace(/\s+/g, " ").trim(), b.getAttribute("aria-pressed")])).toEqual([
    [`This job ${ON_JOB.length}`, "false"],
    ["All", "true"],
  ]);
  scope[0]!.click();
  flushSync();
  await settle();
  expect(target.querySelector(".tf-scope-note")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "2 of 5 pipelines are relevant to PAY-231. Show all",
  );
  expect([...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent)).toEqual(["By project"]);
  expect(names()).toEqual(["Payout_Build", "Payout_Deploy_Prod"]);
  expect(rows()[0]!.querySelector(".tf-pi-why")?.textContent).toBe("your branch, PR check");
  // Every fold is open while the job narrows the list.
  expect([...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].every((d) => d.open)).toBe(true);
  // A bare #/builds opens on All pipelines, a job or not (#2570, Björn's answer 4).
  expect(target.querySelector('.tf-pl-home a[aria-current="true"]')).not.toBeNull();

  target.querySelector<HTMLButtonElement>(".tf-scope-note button")!.click();
  flushSync();
  // Every pipeline is back (Payout_Build twice, pinned and in its fold); the
  // closed fold holds its one, undrawn until opened.
  expect(names()).toHaveLength(5);
  expect(summaries()).toContain("Platform › Infra|||1 pipeline");
  expect(target.querySelector(".tf-scope-note")).toBeNull();
  expect(target.querySelector(".tf-pi-why")).toBeNull();
});

test("a job with no pipelines says so and offers to search all of them", async () => {
  sessionFilter("builds-pipelines").scope = "job";
  drawTree({ job: "PAY-999", reasons: [] });
  await settle();
  const note = [...target.querySelectorAll(".tf-pl-note")].map((n) => n.textContent?.replace(/\s+/g, " ").trim());
  expect(note).toEqual(["No pipeline matches. Search all pipelines"]);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-pl-note button")][0]!.click();
  flushSync();
  expect(target.querySelectorAll(".tf-pl-fold").length).toBeGreaterThan(0);
});

test("Builds draws every pipeline the read answers and drops none by the source list (#1295)", async () => {
  const relay: RailPipeline = {
    ...pipeline("Rocket_Build", "Rocket_Build", [run("relay:run:7", "ok")]),
    entity_id: "relay:pipeline:Rocket_Build",
    source_id: "relay",
    source_name: "Relay CI",
  };
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve([...LIST, relay]),
    runs: (id) => Promise.resolve(runsOf(id)),
  };
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: [], job: null, ports } });
  await settle();

  // Relay's one pipeline sits in its closed *Payout* fold, drawn once opened.
  expect([...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent)).toContain("Relay CI");
  const relayFold = [...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].at(-1)!;
  relayFold.open = true;
  relayFold.dispatchEvent(new Event("toggle"));
  flushSync();
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

test("the pipeline list draws no search field and leaves / to the shell, which opens the launcher on #b (#2425)", async () => {
  draw(["teamcity:buildType:Payout_IntegrationTests"]);
  await settle();
  expect(target.querySelector(".tf-pl input")).toBeNull();
  expect(target.querySelector(".tf-pl .tf-fsearch")).toBeNull();
  // The runs filter of the selected pipeline is the page's, not a search; it keeps its field.
  const fields = [...target.querySelectorAll<HTMLInputElement>("[data-filter]")];
  expect(fields).toHaveLength(1);
  expect(target.querySelector(".tf-runbar")!.contains(fields[0]!)).toBe(true);
  const event = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  expect(event.defaultPrevented, "the shell's TfShell test witnesses what / opens").toBe(false);
  expect(document.activeElement).not.toBe(fields[0]);
});

test("Refresh on a pipeline reads the list again, and its row as the read answers it (#2137)", async () => {
  let reads = 0;
  let list = LIST;
  const ports: BuildsPorts = {
    pipelines: () => (reads++, Promise.resolve(list)),
    runs: (id) => Promise.resolve(runsOf(id)),
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

test("a read again of the list that answers takes away the note an earlier failed one left (#2137)", async () => {
  let fail = false;
  const ports: BuildsPorts = {
    pipelines: () => (fail ? Promise.reject(new Error("database is locked")) : Promise.resolve(LIST)),
    runs: (id) => Promise.resolve(runsOf(id)),
  };
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: [LIST[1]!.entity_id], job: null, ports } });
  await settle();
  fail = true;
  target.querySelector<HTMLButtonElement>(".tf-bhead .tf-refresh")!.click();
  await settle();
  expect(target.querySelector(".tf-pl-note")?.textContent).toContain("database is locked");
  fail = false;
  target.querySelector<HTMLButtonElement>(".tf-bhead .tf-refresh")!.click();
  await settle();
  expect(target.querySelector(".tf-pl-note")).toBeNull();
});

test("a filter that opens every fold of a big CI draws only the rows near the viewport, the selection among them (#2211)", async () => {
  const many = Array.from({ length: 1000 }, (_, i) => {
    const name = `Bulk_${String(i).padStart(4, "0")}`;
    return { ...pipeline(name, name, [run(`teamcity:build:${i}`, "ok", { finished: null })]), path: [`Area ${i % 10}`, `Service ${Math.floor(i / 8)}`] };
  });
  const open = () =>
    mount(TfBuilds, {
      target,
      props: {
        ready: true,
        head: "builds",
        rest: [many[997]!.entity_id],
        job: null,
        ports: { pipelines: () => Promise.resolve(many), runs: () => Promise.resolve([]) },
      },
    });
  page = open();
  await settle();
  // Every one of them is running, so the chip keeps them all and opens every fold.
  const running = () => {
    [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!.click();
    flushSync();
  };
  running();
  expect([...target.querySelectorAll<HTMLDetailsElement>(".tf-pl-fold")].every((d) => d.open)).toBe(true);
  expect(names().length).toBeGreaterThan(5);
  expect(names().length).toBeLessThan(200);
  expect(names()[0]).toBe("Bulk_0000");
  // Back on the page with the filter kept: the selected pipeline, far below the first rows, is drawn and marked.
  unmount(page);
  page = open();
  await settle();
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Bulk_0997");
  expect(names()).not.toContain("Bulk_0000");
  running();
});

// The Windows pass (2026-11-14): Builds' first open on the scale estate built 2,200 elements in one task, 194 ms at 4x
// in headless Chrome and drawn 466 ms after the press in WebView2: its 75 rail rows and one day's 76 runs drawn whole.
test("a rail of a hundred pipelines draws the rows near the viewport, as the Inbox does from 40", async () => {
  const many = Array.from({ length: 120 }, (_, i) => {
    const name = `Flat_${String(i).padStart(3, "0")}`;
    // At the tree's top, so the rows are drawn with no fold to open.
    return { ...pipeline(name, name, [run(`teamcity:build:${i}`, "ok")]), group: null };
  });
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: [], job: null, ports: { pipelines: () => Promise.resolve(many), runs: () => Promise.resolve([]), recent: QUIET_RECENT } },
  });
  await settle();
  expect(names().length).toBeGreaterThan(5);
  expect(names().length).toBeLessThan(120);
  expect(names()[0]).toBe("Flat_000");
});

test("a day of a hundred runs draws the runs near the viewport, with room for the rest", async () => {
  const runs = Array.from({ length: 120 }, (_, i) => run(`teamcity:build:${5000 - i}`, "ok", { started: `2026-08-22T${String(11 - Math.floor(i / 60)).padStart(2, "0")}:${String(59 - (i % 60)).padStart(2, "0")}:00Z` }));
  const busy = [pipeline("Busy_Build", "Busy_Build", runs)];
  page = mount(TfBuilds, {
    target,
    props: { ready: true, head: "builds", rest: [busy[0]!.entity_id], job: null, ports: { pipelines: () => Promise.resolve(busy), runs: () => Promise.resolve(runs) } },
  });
  await settle();
  const drawn = target.querySelectorAll(".tf-runtbl tr.tf-row").length;
  expect(drawn).toBeGreaterThan(5);
  expect(drawn).toBeLessThan(120);
  expect(target.querySelector(".tf-runtbl tr.tf-row a")?.textContent).toBe("#5000");
  const room = target.querySelector<HTMLElement>(".tf-runtbl tr.tf-wgap td")!;
  expect(room.style.height).toBe(`${(120 - drawn) * 34}px`);
});

test("By repository folds the pipelines by the repositories their runs built, those that built none last", async () => {
  const commit = (repo: string) => `git:${repo}@abc`;
  const at = (id: string, repo: string | null, failing = false): RailPipeline => ({
    ...pipeline(id, id, [{ ...run(`teamcity:build:${id}`, failing ? "err" : "ok"), commit_id: repo === null ? null : commit(repo) }]),
  });
  const list = [at("Payout_Build", "tidewater/payout-service", true), at("Ledger_Build", "tidewater/ledger-api"), at("Nightly", null)];
  const asked: string[][] = [];
  page = mount(TfBuilds, {
    target,
    props: {
      ready: true,
      head: "builds",
      rest: [],
      job: null,
      ports: {
        pipelines: () => Promise.resolve(list),
        runs: () => Promise.resolve([]),
        commitRepositories: (ids: string[]) => {
          asked.push(ids);
          return Promise.resolve(
            ids.map((id) => ({ commit_id: id, repo_id: id.replace(/@.*/, ""), repo_title: id.replace(/^git:|@.*$/g, "") })),
          );
        },
      },
    },
  });
  await settle();
  expect(asked).toEqual([]);
  [...target.querySelectorAll<HTMLButtonElement>('.tf-seg[aria-label="Group by"] button')].find((b) => b.textContent === "By repository")!.click();
  await settle();
  expect(asked).toEqual([[commit("tidewater/ledger-api"), commit("tidewater/payout-service")]]);
  expect([...target.querySelectorAll(".tf-pl h3")].map((h) => h.textContent)).toEqual(["By repository"]);
  const folds = [...target.querySelectorAll(".tf-pl-fold > summary")].map((s) => s.querySelector(".tf-fold-nm")?.textContent);
  expect(folds).toEqual(["tidewater/ledger-api", "tidewater/payout-service", "No repository"]);
});

// -- Mark builds as read (ux18) -----------------------------------------------

function marks(held: string[]) {
  const asked: [string[], string[]][] = [];
  const marked = new Set(held);
  runsRead.ports = {
    // The counts and the instants as the bridge answers them (#2568): not what this case reads.
    ...runsRead.ports,
    read: async () => ({ read: [...marked], while_running: [] }),
    mark: async ({ runs, pipelines }) => {
      asked.push([runs, pipelines]);
      const wrote = runs.filter((id) => !marked.has(id));
      for (const id of wrote) marked.add(id);
      return { read: wrote, while_running: [] };
    },
    unmark: async (runs) => runs,
    open: async () => false,
  };
  return asked;
}

const nameOf = (words: string) => rows().find((row) => row.querySelector(".tf-pi-nm")?.textContent === words)!;

test("a failing pipeline's name is err while its latest failure is unread; read, the glyph stays red and the name is ink", async () => {
  marks([]);
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  const tests = () => nameOf("Payout_IntegrationTests");
  expect(tests().querySelector(".tf-pi-nm")!.classList.contains("tf-bad")).toBe(true);
  expect(tests().querySelector(".tf-pi-when")!.classList.contains("tf-err")).toBe(true);
  void runsRead.markRead(["teamcity:build:1187"]);
  flushSync();
  expect(tests().querySelector(".tf-pi-nm")!.classList.contains("tf-bad")).toBe(false);
  expect(tests().querySelector(".tf-pi-when")!.classList.contains("tf-err")).toBe(false);
  expect(tests().querySelector(".tf-pi-when")!.textContent, "the time is still said").not.toBe("");
  expect(tests().title).toContain("Failed");
});

test("the rail's Mark all as read acts on the pipelines it lists: their unread failures drawn, every one asked of the backend", async () => {
  const asked = marks([]);
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  const press = target.querySelector<HTMLButtonElement>(".tf-pl-tools button[aria-label='Mark all as read']")!;
  press.click();
  flushSync();
  expect(nameOf("Payout_IntegrationTests").querySelector(".tf-pi-nm")!.classList.contains("tf-bad")).toBe(false);
  await settle();
  const [runs, pipelines] = asked[0]!;
  expect(runs).toEqual(["teamcity:build:1187", "teamcity:build:1185", "teamcity:build:1183", "teamcity:build:1181", "teamcity:build:1179", "teamcity:build:1177", "teamcity:build:1175"]);
  expect(pipelines).toEqual(LIST.map((p) => p.entity_id));
  expect(toasts.items.some((t) => t.text === "Marked 7 runs as read")).toBe(true);
});

// -- a run started here, and one in flight, move on the rail (the TeamCity run fix, 2026-11-25) --

test("a run queued here says queued on the rail at once; its row is read again when its read-back or a drawn run's refresh lands", async () => {
  const events = echoEvents(settle);
  /** The rail's own `item:refreshed` handler, which the test calls straight while the clock is faked. */
  const heard: ((event: { payload: unknown }) => void)[] = [];
  let list = LIST;
  let reads = 0;
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve(list),
    // Payout_Lint's runs are read only for its rail row: the page on the right is Payout_IntegrationTests'.
    runs: (id) => (id === "teamcity:buildType:Payout_Lint" && reads++, Promise.resolve(list.find((p) => p.entity_id === id)?.recent ?? [])),
    listen: (event, handler) => {
      if (event === "item:refreshed") heard.push(handler);
      return events.listen(event, handler);
    },
  };
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: [], job: null, ports } });
  await settle();
  const lint = () => rows().find((row) => row.querySelector(".tf-pi-nm")?.textContent === "Payout_Lint")!;
  expect(lint().querySelector(".tf-rg")?.getAttribute("aria-label")).toBe("Passed");
  // *Run* on Payout_Lint (its page, or a run page): the queue's answer draws it queued on the rail.
  echoWrite("teamcity:buildType:Payout_Lint", QUEUED_RUN, "", { write: { id: 51 }, base: "teamcity:build:904", many: true });
  flushSync();
  expect(lint().querySelector(".tf-rg")?.getAttribute("aria-label")).toBe("Queued");
  expect(lint().querySelector(".tf-pi-when")?.textContent).toBe("queued");
  // Its read-back names the write: its row's runs again, at once (not the whole list).
  const queued = run("teamcity:build:905", "ok", { started: null, finished: null, duration_seconds: null, trigger: "person", state: { text: "queued", tone: "info" } });
  list = LIST.map((p) => (p.name === "Payout_Lint" ? { ...p, recent: [queued, ...p.recent] } : p));
  await events.refreshed("teamcity:build:905", 51);
  expect(reads).toBe(1);
  expect(lint().querySelector(".tf-rg")?.getAttribute("aria-label")).toBe("Queued");
  // The follow reads it running: within 5 s of the last read, the list waits, then reads once.
  list = LIST.map((p) => (p.name === "Payout_Lint" ? { ...p, recent: [{ ...queued, started: "2026-08-22T10:06:00Z", state: { text: "running", tone: "info" } }, ...p.recent] } : p));
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"], now: Date.now() });
  try {
    heard.forEach((handler) => handler({ payload: { entity_id: "teamcity:build:905", write_id: null, ended: "refreshed" } }));
    heard.forEach((handler) => handler({ payload: { entity_id: "teamcity:build:905", write_id: null, ended: "refreshed" } }));
    vi.advanceTimersByTime(4_000);
    expect(reads, "within 5 s of the last read, nothing yet").toBe(1);
    vi.advanceTimersByTime(1_000);
  } finally {
    vi.useRealTimers();
  }
  await settle();
  expect(reads, "the two refreshes, one read").toBe(2);
  expect(lint().querySelector(".tf-rg")?.getAttribute("aria-label")).toBe("Running");
  // A refresh of a run the rail does not draw reads nothing.
  await events.refreshed("teamcity:build:1", null);
  expect(reads).toBe(2);
  forgetEchoes();
});

test("a run in flight under a newer one still moves its row: its end reads the row again and Running goes (#2571)", async () => {
  const events = echoEvents(settle);
  const stuck = run("teamcity:build:900", "ok", { started: "2026-08-22T09:00:00Z", finished: null, duration_seconds: null, updated_at: "2026-08-22T09:00:00Z", state: { text: "running", tone: "info" } });
  const newer = run("teamcity:build:904", "ok");
  let runs = [newer, stuck];
  let reads = 0;
  const ports: BuildsPorts = {
    pipelines: () => Promise.resolve([pipeline("Payout_Lint", "Payout_Lint", runs)]),
    runs: () => (reads++, Promise.resolve(runs)),
    listen: events.listen,
  };
  // On the pipeline's page, so its fold is open; the page reads its own runs too.
  page = mount(TfBuilds, { target, props: { ready: true, head: "builds", rest: ["teamcity:buildType:Payout_Lint"], job: null, ports } });
  await settle();
  const lint = () => rows().find((row) => row.querySelector(".tf-pi-nm")?.textContent === "Payout_Lint")!;
  expect(lint().querySelector(".tf-rg")?.getAttribute("aria-label"), "the glyph is the newer pass").toBe("Passed");
  const before = reads;
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!.click();
  flushSync();
  expect(names(), "the run in flight puts it under Running").toEqual(["Payout_Lint"]);
  // The older run ends: its refresh names it, though the rail draws only the newer one's glyph.
  runs = [newer, { ...stuck, finished: "2026-08-22T10:20:00Z", updated_at: "2026-08-22T10:20:00Z", state: { text: "FAILURE", tone: "err" } }];
  await events.refreshed("teamcity:build:900", null);
  await settle();
  expect(reads, "its row's runs, read again").toBeGreaterThan(before);
  expect(names(), "nothing in flight: Running keeps none").toEqual([]);
  // Its failure is no longer the latest, yet the right-click counts it before the list is read again.
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!.click();
  flushSync();
  expect(thingAt(lint().querySelector(".tf-pi-nm"))?.thing).toMatchObject({ kind: "pipeline", unread: ["teamcity:build:900"] });
  forgetEchoes();
});

// ---------------------------------------------------------------------------
// *Recent* (2026-11-25): the newest runs of every pipeline, no folds
// ---------------------------------------------------------------------------


/** A folder's estate: two pipelines under *Payments › Payout*, one under *Ops*. */
const FOLDERED: RailPipeline[] = [
  { ...LIST[0]!, path: ["Payments", "Payout"], group: "Payout" },
  { ...LIST[1]!, path: ["Payments", "Payout"], group: "Payout" },
  { ...LIST[2]!, path: ["Ops"], group: "Ops" },
];

/** The table's reads, by the filter each asked. */
const ASKING = (asked: { pipelines: string[] }[]) => ({
  ...QUIET_RECENT,
  recent: (filter: { pipelines: string[] }) => {
    asked.push(filter);
    return Promise.resolve({ runs: [{ pipeline_id: LIST[0]!.entity_id, run: LIST[0]!.recent[0]! }], pipelines: [{ ...LIST[0]!, recent: undefined }], next: null } as never);
  },
});

/** What stands at the top of the page's right side, in order: the same at every scope (#2570). */
const topOfMain = () =>
  [...target.querySelector(".tf-bmain")!.querySelectorAll(":scope .tf-crumbs, :scope .tf-fhead, :scope .tf-runbar")].map((el) =>
    el.classList.contains("tf-crumbs") ? "crumbs" : el.classList.contains("tf-fhead") ? "header" : "bar",
  );

test("All pipelines is the rail's first row, above its tools, at every scope, and the pill when selected (#2570)", async () => {
  const asked: { pipelines: string[] }[] = [];
  draw(["recent"], "builds", [], { recent: ASKING(asked) } as Partial<BuildsPorts>);
  await settle();
  const rail = target.querySelector("nav.tf-pl")!;
  expect(rail, "the rail is drawn beside the runs").not.toBeNull();
  expect(rail.firstElementChild?.classList.contains("tf-pl-home")).toBe(true);
  const home = rail.querySelector<HTMLAnchorElement>(".tf-pl-home a")!;
  expect(home.querySelector(".tf-pi-nm")?.textContent).toBe("All pipelines");
  expect(home.getAttribute("href")).toBe("#/builds/recent");
  expect(home.getAttribute("aria-current")).toBe("true");
  expect(target.querySelector("[aria-label='Builds view']"), "no Pipelines / Recent switch").toBeNull();
  expect(asked).toEqual([{ sources: [], results: [], mine: false, branches: null, pipelines: [] }]);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("All pipelines");
  expect(target.querySelector(".tf-crumbs")?.textContent).toContain("All pipelines");
  expect(topOfMain()).toEqual(["crumbs", "header", "bar"]);
  unmount(page!);
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  expect(target.querySelector(".tf-pl-home a")?.getAttribute("aria-current"), "a pipeline's page: not the pill").toBe("false");
  expect(topOfMain().slice(0, 2), "a pipeline's crumbs and header stand where they do").toEqual(["crumbs", "header"]);
});

test("All pipelines draws before the pipeline list is read, which follows its first page", async () => {
  const drawn: { pipelines: string[] }[] = [];
  let listed = 0;
  draw(["recent"], "builds", [], { recent: ASKING(drawn), pipelines: () => ((listed += 1), Promise.resolve(LIST)) } as Partial<BuildsPorts>);
  flushSync();
  expect(listed).toBe(0);
  await settle();
  expect(drawn).toHaveLength(1);
  expect(listed).toBe(1);
});

test("a bare #/builds opens All pipelines the first time in a session, and where you were after that (#2570)", async () => {
  location.hash = "#/builds";
  draw([]);
  await settle();
  expect(location.hash, "the section's top page keeps its address").toBe("#/builds");
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("All pipelines");
  expect(target.querySelector(".tf-pl-home a")?.getAttribute("aria-current")).toBe("true");
  unmount(page!);
  draw(["teamcity:buildType:Payout_Lint"]);
  await settle();
  unmount(page!);
  location.hash = "#/builds";
  draw([]);
  await settle();
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Lint");
  unmount(page!);
  draw(["folder", "teamcity", "Payments", "Payout"], "builds", [], { pipelines: () => Promise.resolve(FOLDERED) });
  await settle();
  unmount(page!);
  location.hash = "#/builds";
  draw([], "builds", [], { pipelines: () => Promise.resolve(FOLDERED) });
  await settle();
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("Payout");
});

test("a folder's address lists the runs of its pipelines alone; its fold is the pill, and the header and crumbs name it (#2570)", async () => {
  const asked: { pipelines: string[] }[] = [];
  draw(["folder", "teamcity", "Payments", "Payout"], "builds", [], { pipelines: () => Promise.resolve(FOLDERED), recent: ASKING(asked) } as Partial<BuildsPorts>);
  await settle();
  expect(asked.map((filter) => filter.pipelines)).toEqual([[LIST[0]!.entity_id, LIST[1]!.entity_id].sort()]);
  const fold = target.querySelector<HTMLElement>('.tf-tfold[aria-current="true"]')!;
  expect(fold.querySelector(".tf-fold-nm")?.textContent).toBe("Payments › Payout");
  expect(target.querySelectorAll('[aria-current="true"]'), "one selection").toHaveLength(1);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("Payout");
  expect(target.querySelector(".tf-crumbs")?.textContent).toMatch(/Builds.*Payments.*Payout/);
  expect(topOfMain()).toEqual(["crumbs", "header", "bar"]);
  // The chips narrow the rail, never the table (the mockup's rule 2).
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Running")!.click();
  await settle();
  expect(asked.at(-1)?.pipelines).toEqual([LIST[0]!.entity_id, LIST[1]!.entity_id].sort());
});

test("a press on a fold's name selects that folder and opens it; Enter on a fold selects it (#2570)", async () => {
  location.hash = "#/builds/recent";
  draw(["recent"], "builds", [], { pipelines: () => Promise.resolve(FOLDERED) });
  await settle();
  const ops = [...target.querySelectorAll<HTMLElement>(".tf-tfold")].find((f) => f.querySelector(".tf-fold-nm")?.textContent === "Ops")!;
  expect((ops.parentElement as HTMLDetailsElement).open).toBe(false);
  ops.querySelector<HTMLElement>(".tf-fold-nm")!.click();
  flushSync();
  expect(location.hash).toBe("#/builds/folder/teamcity/Ops");
  expect((ops.parentElement as HTMLDetailsElement).open, "selected, it opens").toBe(true);
  const payout = [...target.querySelectorAll<HTMLElement>(".tf-tfold")].find((f) => f.querySelector(".tf-fold-nm")?.textContent === "Payments › Payout")!;
  payout.focus();
  payout.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  flushSync();
  expect(location.hash).toBe("#/builds/folder/teamcity/Payments/Payout");
});

test("a repository's folder turns the rail to By repository and lists the pipelines its runs built (#2570)", async () => {
  const asked: { pipelines: string[] }[] = [];
  const built = LIST.map((p, i) => railOf({ ...p, recent: runsOf(p.entity_id).map((r) => ({ ...r, commit_id: i === 2 ? null : `git:c${i}` })) }));
  draw(["folder", "~repo", "git:payout"], "builds", [], {
    pipelines: () => Promise.resolve(built),
    commitRepositories: () =>
      Promise.resolve([
        { commit_id: "git:c0", repo_id: "git:payout", repo_title: "tidewater/payout-service" },
        { commit_id: "git:c1", repo_id: "git:ledger", repo_title: "tidewater/ledger-api" },
      ]),
    recent: ASKING(asked),
  } as Partial<BuildsPorts>);
  await settle();
  expect(target.querySelector('.tf-seg[aria-label="Group by"] [aria-pressed="true"]')?.textContent).toBe("By repository");
  expect(asked.map((filter) => filter.pipelines)).toEqual([[LIST[0]!.entity_id]]);
  expect(target.querySelector(".tf-bmain h1")?.textContent).toBe("tidewater/payout-service");
  expect(target.querySelector('.tf-tfold[aria-current="true"] .tf-fold-nm')?.textContent).toBe("tidewater/payout-service");
});

test("a folder knobas does not hold says so, under the header and crumbs", async () => {
  draw(["folder", "teamcity", "Gone"]);
  await settle();
  expect(target.querySelector(".tf-bmain")?.textContent).toContain("knobas does not hold this folder");
});

test("Alt+click on a Builds folder recursively expands and collapses every nested folder under it", async () => {
  const nested: RailPipeline[] = [
    { ...pipeline("a", "Api", [run("teamcity:build:1", "ok")]), path: ["Core", "Payments", "V1"] },
    { ...pipeline("b", "Worker", [run("teamcity:build:2", "ok")]), path: ["Core", "Payments", "V2"] },
    { ...pipeline("c", "Edge", [run("teamcity:build:3", "ok")]), path: ["Core", "Edge"] },
  ];
  // Api selected, so its folds open as the tree draws.
  draw(["teamcity:buildType:a"], "builds", [], { pipelines: () => Promise.resolve(nested) });
  await settle();
  const foldSummary = (name: string) =>
    [...target.querySelectorAll<HTMLElement>(".tf-pl-rows summary.tf-tfold")].find((s) => s.querySelector(".tf-fold-nm")?.textContent === name)!;

  // Collapse Core recursively first, then Alt+click to expand Core + Payments + V1 + V2 + Edge at once.
  foldSummary("Core").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect((foldSummary("Core").parentElement as HTMLDetailsElement).open).toBe(false);
  expect(names()).toEqual([]);

  foldSummary("Core").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect((foldSummary("Core").parentElement as HTMLDetailsElement).open).toBe(true);
  expect((foldSummary("Payments").parentElement as HTMLDetailsElement).open).toBe(true);
  expect((foldSummary("V1").parentElement as HTMLDetailsElement).open).toBe(true);
  expect(names()).toEqual(["Edge", "Api", "Worker"]);

  // Alt+click Core again collapses the subtree; re-opening Core alone leaves Payments closed.
  foldSummary("Core").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(names()).toEqual([]);
  const coreDetails = foldSummary("Core").parentElement as HTMLDetailsElement;
  coreDetails.open = true;
  coreDetails.dispatchEvent(new Event("toggle"));
  flushSync();
  expect((foldSummary("Payments").parentElement as HTMLDetailsElement).open).toBe(false);
});

// -- Right-click on the rail: Mark N runs as read (#2527, #2543) --------------

test("a right-click on a rail pipeline or folder carries its unread failed runs over the whole history, read as the menu opens (#2543)", async () => {
  marks(["teamcity:build:5", "teamcity:build:40"]);
  const nested: RailPipeline[] = [
    {
      ...pipeline("a", "Api", [run("teamcity:build:1", "err"), run("teamcity:build:9", "ok"), run("teamcity:build:5", "err")]),
      path: ["Core", "Payments"],
      // What the backend named as the list was read: one drawn, two past the twelve, one of them read since.
      unread: ["teamcity:build:1", "teamcity:build:41", "teamcity:build:40"],
    },
    {
      ...pipeline("b", "Worker", [run("teamcity:build:2", "err"), run("teamcity:build:8", "ok", { finished: null, state: { text: "running", tone: "info" } })]),
      path: ["Core", "Payments"],
      unread: ["teamcity:build:2", "teamcity:build:30"],
    },
    { ...pipeline("c", "Edge", [run("teamcity:build:3", "ok")]), path: ["Core", "Edge"] },
  ];
  draw([], "builds", [], { pipelines: () => Promise.resolve(nested) });
  await settle();
  const fold = (name: string) =>
    [...target.querySelectorAll<HTMLElement>(".tf-pl-rows summary.tf-tfold")].find((s) => s.querySelector(".tf-fold-nm")?.textContent === name)!;

  expect(thingAt(nameOf("Api").querySelector(".tf-pi-nm"))?.thing).toEqual({
    kind: "pipeline",
    id: "teamcity:buildType:a",
    hash: "#/builds/teamcity%3AbuildType%3Aa",
    unread: ["teamcity:build:1", "teamcity:build:41"],
  });
  expect(thingAt(fold("Core").querySelector(".tf-fold-nm"))?.thing).toEqual({
    kind: "pipeline-folder",
    name: "Core",
    unread: ["teamcity:build:1", "teamcity:build:41", "teamcity:build:2", "teamcity:build:30"],
    pipelines: ["teamcity:buildType:c", "teamcity:buildType:a", "teamcity:buildType:b"],
  });
  expect(thingAt(fold("Edge"))?.thing).toEqual({ kind: "pipeline-folder", name: "Edge", unread: [], pipelines: ["teamcity:buildType:c"] });

  // Fresh across marks: a run past the twelve read, or one put back, moves the count with no read of the list.
  void runsRead.markRead(["teamcity:build:2", "teamcity:build:41"]);
  flushSync();
  expect(thingAt(fold("Core"))?.thing).toMatchObject({ unread: ["teamcity:build:1", "teamcity:build:30"] });
  expect(thingAt(nameOf("Worker"))?.thing).toMatchObject({ kind: "pipeline", unread: ["teamcity:build:30"] });
  void runsRead.markUnread(["teamcity:build:40"]);
  flushSync();
  expect(thingAt(nameOf("Api"))?.thing).toMatchObject({ unread: ["teamcity:build:1", "teamcity:build:40"] });
});

test("a failure the list did not name, marked unread, reads the list again so the rail's count holds it (#2543)", async () => {
  marks(["teamcity:build:77"]);
  let reads = 0;
  const listed = (unread: string[]): RailPipeline[] => [{ ...pipeline("a", "Api", [run("teamcity:build:1", "ok")]), unread }];
  draw(["teamcity:buildType:a"], "builds", [], {
    pipelines: () => {
      reads += 1;
      return Promise.resolve(listed(reads === 1 ? [] : ["teamcity:build:77"]));
    },
  });
  await settle();
  expect(reads).toBe(1);
  expect(thingAt(nameOf("Api"))?.thing).toMatchObject({ unread: [] });

  // Read when the list was read, so the list did not name it: an older page's *Mark as unread* puts it back. The list is
  // read once that write answered: a read sent beside it could miss the mark.
  const unmark = runsRead.ports.unmark;
  let answer = () => {};
  runsRead.ports = { ...runsRead.ports, unmark: (runs) => new Promise((resolve) => (answer = () => resolve(runs))) };
  void runsRead.markUnread(["teamcity:build:77"]);
  await settle();
  expect(reads, "not while the write is out").toBe(1);
  answer();
  runsRead.ports = { ...runsRead.ports, unmark };
  await settle();
  expect(reads, "the list read again").toBe(2);
  expect(thingAt(nameOf("Api"))?.thing).toMatchObject({ unread: ["teamcity:build:77"] });

  // One the list names already reads nothing more.
  void runsRead.markRead(["teamcity:build:77"]);
  await settle();
  void runsRead.markUnread(["teamcity:build:77"]);
  await settle();
  expect(reads).toBe(2);
});

test("a run opened from a scope's table holds the rail's rows unseen until its pipeline is scrolled into view, so they never move (#2570)", async () => {
  let head = $state("builds");
  let rest = $state<string[]>(["recent"]);
  page = mount(TfBuilds, {
    target,
    props: {
      ready: true,
      get head() {
        return head;
      },
      get rest() {
        return rest;
      },
      job: null,
      ports: { pipelines: () => Promise.resolve(LIST), runs: (id: string) => Promise.resolve(runsOf(id)), recent: QUIET_RECENT },
    },
  });
  await settle();
  const rail = () => target.querySelector<HTMLElement>(".tf-pl-rows")!;
  expect(rail().classList.contains("tf-unseen"), "drawn at All pipelines").toBe(false);
  head = "build";
  rest = ["teamcity:buildType:Payout_Lint", "teamcity:build:904"];
  flushSync();
  expect(rail().classList.contains("tf-unseen"), "held unseen in the run page's first frame").toBe(true);
  await settle();
  expect(rail().classList.contains("tf-unseen"), "shown once scrolled").toBe(false);
  expect(target.querySelector('.tf-pi[aria-current="true"] .tf-pi-nm')?.textContent).toBe("Payout_Lint");
});
