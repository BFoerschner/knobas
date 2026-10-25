/**
 * The pipeline page (#851) over the IPC mirror: the header with the source's
 * word, the repository and the triggers; the history chart and its facts;
 * runs by day with the job's chip and the filter; older history on demand;
 * a run started here drawn queued at once; and the Parameters and Triggers
 * tabs over the live settings read.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { PipelineRow, PipelineSettings, RunRow } from "../../ipc/entity";
import type { QueuedWrite, SourceDescriptor, SourceSummary, WriteOpPayload } from "../../ipc/sources";
import { health } from "../../shell/health.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { forgetEchoes, echoWrite, echoesOn } from "../queued.svelte";
import type { PipelinePorts } from "./pipeline-ports";
import { QUEUED_RUN } from "./pipeline";
import TfPipeline from "./TfPipeline.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  health.replace([]);
  if (page) unmount(page);
  page = null;
  target.remove();
  forgetEchoes();
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const today = (h: number, m = 0) => {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, m).toISOString();
};
const daysAgo = (n: number, h: number) => {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - n, h).toISOString();
};

function run(no: number, tone: "ok" | "err", started: string, extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: `teamcity:build:${no}`,
    number: String(no),
    state: { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch: "main",
    started,
    finished: started,
    duration_seconds: 240,
    updated_at: started,
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

const RUNS: RunRow[] = [
  run(1187, "err", today(0, 5), {
    branch: "feature/PAY-231-sepa-retry",
    duration_seconds: 252,
    trigger: "change",
    revision: "a41f2c0e9b8d7c6a",
    tests: { failed: 1, passed: 41, ignored: 0 },
  }),
  run(1186, "ok", daysAgo(1, 18), {
    duration_seconds: 241,
    trigger: "person",
    started_by: "mara",
    started_by_me: true,
    tests: { failed: 0, passed: 42, ignored: 2 },
  }),
  run(1185, "err", daysAgo(1, 17), { branch: "feature/PAY-231-sepa-retry", duration_seconds: 268 }),
  ...Array.from({ length: 31 }, (_, i) => run(1184 - i, "ok", daysAgo(3, 12), { duration_seconds: 200 + i })),
];

const PIPELINE: PipelineRow = {
  entity_id: "teamcity:buildType:Payout_IntegrationTests",
  source_id: "teamcity",
  source_name: "TeamCity",
  name: "Payout_IntegrationTests",
  group: "Payouts",
  path: ["Payments", "Payouts"],
  pinned: false,
  web_url: "http://ci.example/buildConfiguration/Payout_IntegrationTests",
  recent: RUNS.slice(0, 12),
};

const SETTINGS: PipelineSettings = {
  repositories: [
    { name: "payout-service", url: "http://gitea.example/tidewater/payout-service.git", default_branch: "refs/heads/main", entity_id: "gitea:repo:tidewater/payout-service" },
  ],
  triggers: [
    { name: "VCS", facts: [{ label: "Branch filter", value: "+:refs/heads/*", tone: null }], disabled: false },
    { name: "Schedule", facts: [{ label: "When", value: "daily at 02:00", tone: null }], disabled: false },
  ],
  parameters: [
    { name: "env", value: "staging", inherited: false, secret: false, editable: true, description: null },
    { name: "deploy.token", value: "", inherited: true, secret: true, editable: true, description: null },
  ],
  parameter_write_op: "set_parameter",
  parameter_specs: [
    {
      name: "env",
      label: null,
      hint: null,
      kind: {
        type: "select",
        options: [
          { value: "staging", label: null },
          { value: "prod-dryrun", label: null },
        ],
      },
    },
    { name: "deploy.token", label: null, hint: null, kind: { type: "text" } },
  ],
  chain: [],
  steps: [],
};

const DESCRIPTOR = {
  adapter_kind: "teamcity",
  entity_kinds: [{ id: "build_config", label: "Build configuration" }],
  payload_paths: [{ kind: "build_config", frame: "pipeline" }],
} as unknown as SourceDescriptor;

const SOURCE = {
  id: "teamcity",
  adapter_kind: "teamcity",
  health: { source_id: "teamcity", state: "ok", checked_at: null, detail: null, secret_expires_at: null },
} as unknown as SourceSummary;

interface Drawn {
  pinned: number;
  writes: WriteOpPayload[];
  olderAsked: string[];
  /** How many times the runs were read. */
  reads: number;
  /** Fire `sync:state` as the shell hears it; `null` once the page stopped listening. */
  sync: ((payload: unknown) => void) | null;
  /** What the next runs read answers. */
  next: RunRow[] | Error | Promise<RunRow[]>;
  /** Open another pipeline in the same page. */
  goPipeline: (pipeline: PipelineRow) => void;
  /** Move the page to another address under the pipeline, as a tab click does. */
  go: (rest: string[]) => void;
}

function draw(
  rest: string[] = [PIPELINE.entity_id],
  job: string | null = "PAY-231",
  overrides: Partial<PipelinePorts> = {},
  older: RunRow[][] = [[run(1150, "ok", daysAgo(9, 9))], []],
): Drawn {
  const drawn: Drawn = { pinned: 0, writes: [], olderAsked: [], reads: 0, sync: null, next: RUNS, go: () => {}, goPipeline: () => {} };
  const ports: PipelinePorts = {
    older: (_id, before) => {
      drawn.olderAsked.push(before);
      return Promise.resolve(older.shift() ?? []);
    },
    settings: () => Promise.resolve(SETTINGS),
    adapters: () => Promise.resolve([DESCRIPTOR]),
    sources: () => Promise.resolve([SOURCE]),
    syncStatus: () => Promise.resolve([]),
    jobBranches: () => Promise.resolve(["feature/PAY-231-sepa-retry"]),
    submitWrite: (payload) => {
      drawn.writes.push(payload);
      return Promise.resolve({ id: 41 } as QueuedWrite);
    },
    openExternal: () => Promise.resolve(),
    presets: () => Promise.resolve([]),
    savePreset: (preset) => Promise.resolve(preset),
    deletePreset: () => Promise.resolve(),
    repoBranches: () => Promise.resolve(["main", "feature/PAY-231-sepa-retry", "release/1.9"]),
    runParameters: () => Promise.resolve([]),
    listen: (event, handler) => {
      if (event !== "sync:state") return Promise.resolve(() => {});
      const fire = (payload: unknown) => handler({ payload });
      drawn.sync = fire;
      return Promise.resolve(() => {
        if (drawn.sync === fire) drawn.sync = null;
      });
    },
    follower: () => ({ follow: () => () => {} }),
    refusedWrites: () => Promise.resolve([]),
    ...overrides,
  };
  const runs = () => {
    drawn.reads += 1;
    const next = drawn.next;
    return next instanceof Error ? Promise.reject(next) : Promise.resolve(next);
  };
  const props = $state({ pipeline: PIPELINE, rest, job, runs, onpin: () => (drawn.pinned += 1), ports });
  drawn.go = (next) => {
    props.rest = next;
    flushSync();
  };
  drawn.goPipeline = (next) => {
    props.pipeline = next;
    props.rest = [next.entity_id];
    flushSync();
  };
  page = mount(TfPipeline, { target, props });
  return drawn;
}

const button = (words: string) =>
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.replace(/\s+/g, " ").trim() === words);

test("a chained pipeline's header names its step and the chain (R-9.13)", async () => {
  const chain = [
    { pipeline: "teamcity:buildType:Payout_Build", name: "Payout_Build" },
    { pipeline: PIPELINE.entity_id, name: "Payout_IntegrationTests" },
    { pipeline: "teamcity:buildType:Payout_Deploy_Staging", name: "Payout_Deploy_Staging" },
  ];
  draw([PIPELINE.entity_id], "PAY-231", { settings: () => Promise.resolve({ ...SETTINGS, chain }) });
  await settle();
  const sub = target.querySelector(".tf-bhead .tf-sub")!;
  expect(sub.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "TeamCity build configuration in project Payments › Payouts, builds payout-service. Triggered by VCS and Schedule. " +
      "Step 2 of the chain Payout_Build › Payout_IntegrationTests › Payout_Deploy_Staging.",
  );
});

test("the header says what it is in the source's word, the repository it builds and what triggers it", async () => {
  draw();
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Payout_IntegrationTests");
  const sub = target.querySelector(".tf-bhead .tf-sub")!;
  expect(sub.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "TeamCity build configuration in project Payments › Payouts, builds payout-service. Triggered by VCS and Schedule.",
  );
  expect(sub.querySelector("a")?.getAttribute("href")).toBe("#/repo/gitea%3Arepo%3Atidewater%2Fpayout-service");
  expect(button("Run…")).toBeDefined();
  expect(button("Run on feature/PAY-231-sepa-retry")).toBeDefined();
  expect(button("Open in TeamCity")).toBeDefined();
  expect(target.querySelector(".tf-stale")).toBeNull();
});

test("Pin writes through the list's pins", async () => {
  const drawn = draw();
  await settle();
  button("Pin")!.click();
  expect(drawn.pinned).toBe(1);
});

test("with no job branch it built, Run on names the default branch; a failing sign-in draws the warn line and its fix", async () => {
  draw(undefined, null, {
    sources: () =>
      Promise.resolve([{ ...SOURCE, health: { ...SOURCE.health, state: "unauthorized", checked_at: new Date(Date.now() - 38 * 60_000).toISOString() } }]),
  });
  await settle();
  expect(button("Run on main")).toBeDefined();
  const stale = target.querySelector(".tf-stale")!;
  expect(stale.textContent).toContain("TeamCity data is 38 min old: sign-in failing. Runs you start wait in the queue until it works again.");
  expect(stale.querySelector("a")?.getAttribute("href")).toBe("#/settings/source/teamcity");
});

test("the chart draws the last thirty runs as links to each run, with Passed, Usual duration and Last passing", async () => {
  draw();
  await settle();
  const bars = target.querySelectorAll(".tf-bars a");
  expect(bars).toHaveLength(30);
  // The newest is the right-most, and says what it was.
  const newest = bars[29]!;
  expect(newest.getAttribute("href")).toBe("#/build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3A1187");
  expect(newest.getAttribute("aria-label")).toBe("Run 1187, FAILURE");
  expect(newest.querySelector("title")?.textContent).toContain("#1187 FAILURE, feature/PAY-231-sepa-retry · 4 m 12 s");
  const facts = target.querySelector(".tf-facts2")!.textContent!.replace(/\s+/g, " ");
  expect(facts).toContain("Passed32 of the last 34");
  expect(facts).toContain("Last passing#1186");
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toContain("Runs 34");
});

test("runs come by day, a run on the job's branch carries the job's chip, and the result chips filter", async () => {
  draw();
  await settle();
  const heads = [...target.querySelectorAll(".tf-dayh")].map((h) => h.textContent);
  expect(heads.slice(0, 2)).toEqual(["Today", "Yesterday"]);
  const first = target.querySelector(".tf-runs tbody tr")!;
  expect(first.textContent).toContain("#1187");
  expect(first.querySelector(".tf-jobchip-sm")?.textContent).toBe("PAY-231");
  expect(first.querySelector("a.tf-btn")?.getAttribute("href")).toMatch(/\/log$/);
  expect(first.textContent).toContain("Run again…");

  button("Failed")!.click();
  flushSync();
  const numbers = [...target.querySelectorAll(".tf-runs tbody tr .tf-run-no b")].map((b) => b.textContent);
  expect(numbers).toEqual(["#1187", "#1185"]);
  expect(target.querySelector(".tf-fcount")?.textContent).toContain("2");
});

test("each run says how it was started; Started by me keeps the reader's; the words reach the commit", async () => {
  draw();
  await settle();
  const rows = () => [...target.querySelectorAll(".tf-runs tbody tr")];
  expect(rows()[0]!.querySelector(".tf-run-trig")?.textContent).toBe("VCS change a41f2c");
  expect(rows()[1]!.querySelector(".tf-run-trig")?.textContent).toBe("you");
  const input = target.querySelector<HTMLInputElement>(".tf-runbar input")!;
  expect(input.placeholder).toBe("Filter runs by number, branch, commit or person");
  const scopes = [...target.querySelectorAll(".tf-runbar button")].map((b) => b.textContent?.trim());
  expect(scopes.indexOf("My job’s branches")).toBeLessThan(scopes.indexOf("Started by me"));
  expect(scopes.indexOf("Started by me")).toBeLessThan(scopes.indexOf("All"));

  button("Started by me")!.click();
  flushSync();
  expect(rows().map((r) => r.querySelector(".tf-run-no b")?.textContent)).toEqual(["#1186"]);

  button("All")!.click();
  input.value = "a41f2c";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(rows().map((r) => r.querySelector(".tf-run-no b")?.textContent)).toEqual(["#1187"]);
});

test("a run's test summary sits between how it started and its duration, the failed count in err tone; a run without tests draws none", async () => {
  draw();
  await settle();
  const rows = [...target.querySelectorAll(".tf-runs tbody tr")];
  const cell = (row: Element) => row.querySelector(".tf-run-tests");
  expect(cell(rows[0]!)?.previousElementSibling?.classList.contains("tf-run-trig")).toBe(true);
  expect(cell(rows[0]!)?.nextElementSibling?.classList.contains("tf-run-dur")).toBe(true);
  expect(cell(rows[0]!)?.textContent).toBe("1 failed, 41 passed");
  expect(cell(rows[0]!)?.querySelector(".tf-tests-failed")?.textContent).toBe("1 failed");
  expect(cell(rows[1]!)?.textContent).toBe("42 passed, 2 ignored");
  expect(cell(rows[1]!)?.querySelector(".tf-tests-failed")).toBeNull();
  expect(cell(rows[2]!)?.textContent).toBe("");
});

test("older runs are asked of the source from the oldest run held, appended, until it answers none", async () => {
  const drawn = draw();
  await settle();
  button("Older runs")!.click();
  await settle();
  expect(drawn.olderAsked).toEqual(["teamcity:build:1154"]);
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toContain("Runs 35");
  button("Older runs")!.click();
  await settle();
  expect(drawn.olderAsked).toEqual(["teamcity:build:1154", "teamcity:build:1150"]);
  expect(target.querySelector(".tf-older")?.textContent).toContain("The start of this pipeline’s history.");
});

test("a failed older read says so and offers to try again", async () => {
  draw(undefined, null, { older: () => Promise.reject({ code: "unreachable", message: "connection refused" }) });
  await settle();
  button("Older runs")!.click();
  await settle();
  expect(target.querySelector(".tf-older")?.textContent).toContain("Older runs could not be read: connection refused");
  expect(button("Try again")).toBeDefined();
});

test("Run on queues one run on that branch and draws it queued at once, never as started", async () => {
  const drawn = draw();
  await settle();
  button("Run on feature/PAY-231-sepa-retry")!.click();
  await settle();
  expect(drawn.writes).toEqual([
    { RunBuild: { entity: PIPELINE.entity_id, branch: "feature/PAY-231-sepa-retry", parameters: [] } },
  ]);
  const first = target.querySelector(".tf-runs tbody tr")!;
  expect(first.querySelector(".tf-run-no")?.textContent).toContain("queued");
  expect(first.textContent).toContain("waiting");
  expect(first.textContent).toContain("feature/PAY-231-sepa-retry");
});

test("leaving Runs once its runs are drawn neither throws nor stops the page (#1446)", async () => {
  const drawn = draw();
  await settle();
  expect(target.querySelector(".tf-older")).not.toBeNull();
  drawn.go([PIPELINE.entity_id, "params"]);
  await settle();
  expect(target.querySelector("table[aria-label='Parameters']")).not.toBeNull();
  drawn.go([PIPELINE.entity_id]);
  await settle();
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toContain("Runs");
});

// -- a sync finishing reads again (#1450) ---------------------------------------

/** A `sync:state` payload for `source`. */
const synced = (source: string, running = false) => ({
  source_id: source,
  running,
  run_id: running ? 7 : null,
  started_at: null,
  last_finished_at: new Date().toISOString(),
  last_outcome: running ? null : "ok",
  next_run_at: null,
  backoff_until: null,
});

/** The first run as the mirror held it in flight, then finished. */
const RUNNING: RunRow[] = [run(1188, "ok", today(0, 30), { finished: null, duration_seconds: null, state: { text: "running", tone: "info" } }), ...RUNS];
const FINISHED: RunRow[] = [run(1188, "ok", today(0, 30), { duration_seconds: 300 }), ...RUNS];

const firstRow = () => target.querySelector(".tf-runs tbody tr")!;

test("a sync of the pipeline's source finishing reads the runs again: a running run draws its result", async () => {
  const drawn = draw();
  drawn.next = RUNNING;
  await settle();
  expect(firstRow().textContent).toContain("running");
  expect(drawn.reads).toBe(1);

  drawn.next = FINISHED;
  drawn.sync!(synced("teamcity", true));
  await settle();
  expect(drawn.reads, "a sync still running reads nothing").toBe(1);

  drawn.sync!(synced("teamcity"));
  await settle();
  expect(drawn.reads).toBe(2);
  expect(firstRow().textContent).toContain("#1188");
  expect(firstRow().textContent).toContain("5 m");
  expect(firstRow().textContent).not.toContain("running");
});

test("a sync of another source reads nothing", async () => {
  const drawn = draw();
  await settle();
  drawn.sync!(synced("gitea"));
  await settle();
  expect(drawn.reads).toBe(1);
});

test("a read again keeps the entered filter and the tab; a failed one keeps the runs and says so", async () => {
  const drawn = draw();
  drawn.next = RUNNING;
  await settle();
  const input = target.querySelector<HTMLInputElement>(".tf-runbar input")!;
  input.value = "1188";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  drawn.next = FINISHED;
  drawn.sync!(synced("teamcity"));
  await settle();
  expect(target.querySelector<HTMLInputElement>(".tf-runbar input")!.value).toBe("1188");
  const rows = [...target.querySelectorAll(".tf-runs tbody tr")];
  expect(rows).toHaveLength(1);
  expect(rows[0]!.textContent).not.toContain("running");

  drawn.go([PIPELINE.entity_id, "params"]);
  drawn.sync!(synced("teamcity"));
  await settle();
  expect(drawn.reads).toBe(3);
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toBe("Parameters");

  drawn.go([PIPELINE.entity_id]);
  drawn.next = Object.assign(new Error("connection refused"), { code: "unreachable" });
  drawn.sync!(synced("teamcity"));
  await settle();
  expect(target.textContent).toContain("Runs could not be read");
  expect(target.querySelectorAll(".tf-runs tbody tr")).toHaveLength(1);
  expect(target.querySelectorAll(".tf-bars a"), "the chart stays").toHaveLength(30);
});

test("of two reads again, the later one's answer stands even when the earlier one answers last", async () => {
  const drawn = draw();
  await settle();
  let answerFirst: (rows: RunRow[]) => void = () => {};
  drawn.next = new Promise<RunRow[]>((resolve) => (answerFirst = resolve));
  drawn.sync!(synced("teamcity"));
  drawn.next = FINISHED;
  drawn.sync!(synced("teamcity"));
  await settle();
  answerFirst(RUNNING);
  await settle();
  expect(firstRow().textContent).toContain("#1188");
  expect(firstRow().textContent).not.toContain("running");
});

test("the page stops listening when it goes", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.sync).not.toBeNull();
  unmount(page!);
  page = null;
  expect(drawn.sync).toBeNull();
});

// -- runs queued on the run page (#1449) ------------------------------------------

const runNos = () => [...target.querySelectorAll(".tf-runs tbody tr .tf-run-no")].map((td) => td.querySelector("b")?.textContent);

test("a run queued on the run page is one queued row at the top; another pipeline draws none for it", async () => {
  echoWrite(PIPELINE.entity_id, QUEUED_RUN, "feature/PAY-231-sepa-retry", { write: { id: 77 }, base: "teamcity:build:1187", many: true });
  const drawn = draw();
  await settle();
  expect(runNos().slice(0, 2)).toEqual(["queued", "#1187"]);
  expect(firstRow().textContent).toContain("feature/PAY-231-sepa-retry");
  expect(firstRow().textContent).toContain("waiting");
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toContain("Runs 35");

  drawn.goPipeline({ ...PIPELINE, entity_id: "teamcity:buildType:Payout_Build", name: "Payout_Build" });
  await settle();
  expect(runNos()).not.toContain("queued");
});

test("the queued row goes once a read holds the run, and is never drawn beside it", async () => {
  echoWrite(PIPELINE.entity_id, QUEUED_RUN, "", { write: { id: 77 }, base: "teamcity:build:1187", many: true });
  const drawn = draw();
  await settle();
  expect(runNos().slice(0, 2)).toEqual(["queued", "#1187"]);

  // A sync that has not brought it yet keeps it queued.
  drawn.sync!(synced("teamcity"));
  await settle();
  expect(runNos().slice(0, 2)).toEqual(["queued", "#1187"]);

  drawn.next = [run(1188, "ok", today(0, 30), { finished: null, duration_seconds: null, trigger: "person", started_by_me: true }), ...RUNS];
  drawn.sync!(synced("teamcity"));
  await settle();
  expect(runNos().slice(0, 2)).toEqual(["#1188", "#1187"]);
  expect(echoesOn(PIPELINE.entity_id, QUEUED_RUN)).toEqual([]);
  expect(target.querySelector(".tf-tabs a[aria-current='page']")?.textContent).toContain("Runs 35");
});

test("Run on keeps its queued row through a visit elsewhere, and the queue refusing it takes it away", async () => {
  const heard: ((row: QueuedWrite | undefined) => void)[] = [];
  const drawn = draw(undefined, "PAY-231", { follower: () => ({ follow: (_id, hear) => (heard.push(hear), () => {}) }) });
  await settle();
  button("Run on feature/PAY-231-sepa-retry")!.click();
  await settle();
  expect(runNos()[0]).toBe("queued");
  drawn.goPipeline({ ...PIPELINE, entity_id: "teamcity:buildType:Payout_Build", name: "Payout_Build" });
  await settle();
  drawn.goPipeline(PIPELINE);
  await settle();
  expect(runNos()[0], "the read holds no new run yet").toBe("queued");

  heard[0]!({ id: 41, state: "refused", detail: "no such branch" } as QueuedWrite);
  flushSync();
  expect(runNos()[0]).toBe("#1187");
  expect(toasts.items.at(-1)?.text).toBe("Run of Payout_IntegrationTests was refused: no such branch");
});

// -- the run rows against prototype-6 (#1452) ------------------------------------

test("a click anywhere on a run row opens the run; its Log link and Run again… do their own", async () => {
  location.hash = "#/builds";
  draw();
  await settle();
  const row = firstRow() as HTMLTableRowElement;
  row.querySelector<HTMLElement>(".tf-run-br")!.click();
  expect(location.hash).toBe("#/build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3A1187");

  location.hash = "#/builds";
  [...row.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run again…")!.click();
  await settle();
  expect(location.hash).toBe("#/builds");
  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
});

test("a click on a queued row opens nothing: there is no run yet", async () => {
  location.hash = "#/builds";
  echoWrite(PIPELINE.entity_id, QUEUED_RUN, "", { write: { id: 77 }, base: "teamcity:build:1187", many: true });
  draw();
  await settle();
  (firstRow() as HTMLElement).querySelector<HTMLElement>(".tf-run-br")!.click();
  expect(location.hash).toBe("#/builds");
  expect(firstRow().classList.contains("tf-row")).toBe(false);
});

test("the run number stays the keyboard's link to the run", async () => {
  draw();
  await settle();
  const link = firstRow().querySelector<HTMLAnchorElement>(".tf-run-no a")!;
  expect(link.getAttribute("href")).toBe("#/build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3A1187");
  expect(link.tabIndex).toBe(0);
});

test("every day's table lays its columns out the same, so they line up down the page", async () => {
  draw();
  await settle();
  const tables = [...target.querySelectorAll<HTMLTableElement>(".tf-runs table")];
  expect(tables.length).toBeGreaterThanOrEqual(3);
  const layout = (table: HTMLTableElement) => [table.className, ...[...table.querySelectorAll("colgroup col")].map((col) => col.className)];
  expect(layout(tables[0]!)).toEqual([
    "tf-tbl tf-runtbl",
    "tf-run-no",
    "tf-run-br",
    "tf-run-trig",
    "tf-run-tests",
    "tf-run-dur",
    "tf-run-at",
    "tf-run-acts",
  ]);
  for (const table of tables) expect(layout(table)).toEqual(layout(tables[0]!));
});

/** A select whose default it does not offer, a checkbox, a secret and a plain spec (#1447). */
const KINDS: PipelineSettings = {
  ...SETTINGS,
  parameters: [
    { name: "env", value: "qa", inherited: false, secret: false, editable: true, description: null },
    { name: "dry.run", value: "false", inherited: true, secret: false, editable: true, description: null },
    { name: "deploy.token", value: "", inherited: true, secret: true, editable: true, description: null },
    { name: "test.filter", value: "", inherited: true, secret: false, editable: true, description: null },
  ],
  parameter_specs: [
    SETTINGS.parameter_specs[0]!,
    { name: "dry.run", label: null, hint: null, kind: { type: "checkbox", checked: "true", unchecked: "false" } },
    { name: "test.filter", label: null, hint: null, kind: { type: "text" } },
  ],
};

const paramRows = () => [...target.querySelectorAll("table[aria-label='Parameters'] tbody tr")];

test("Parameters: each default with the kind its spec gives; a secret is never drawn", async () => {
  draw([PIPELINE.entity_id, "params"], "PAY-231", { settings: () => Promise.resolve(KINDS) });
  await settle();
  const rows = paramRows();
  expect(rows.map((r) => r.querySelector("td")?.textContent)).toEqual(["env", "dry.run", "deploy.token", "test.filter"]);
  const kindCell = (r: Element) => r.querySelectorAll("td")[2]!;
  expect(rows.map((r) => kindCell(r).firstChild?.textContent?.trim())).toEqual([
    "one of staging, prod-dryrun",
    "one of false, true",
    "password",
    "text",
  ]);
  // *inherited* is a chip of its own, never a word in the kind's list (#1564); a secret draws none.
  expect(rows.map((r) => kindCell(r).querySelector(".tf-chip")?.textContent ?? null)).toEqual([null, "inherited", null, "inherited"]);
  expect(rows[2]!.textContent).toContain("(hidden)");
});

test("Change… on a fixed list draws a select of it, the unlisted default first; the pick queues against the old default", async () => {
  const drawn = draw([PIPELINE.entity_id, "params"], "PAY-231", { settings: () => Promise.resolve(KINDS) });
  await settle();
  const rows = paramRows();
  rows[0]!.querySelector<HTMLButtonElement>("button")!.click();
  flushSync();
  expect(target.querySelector("input[aria-label='New default for env']")).toBeNull();
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='New default for env']")!;
  expect([...select.options].map((o) => o.value)).toEqual(["qa", "staging", "prod-dryrun"]);
  expect(select.value).toBe("qa");
  select.value = "prod-dryrun";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  button("Save")!.click();
  await settle();
  expect(drawn.writes).toEqual([{ SetParameter: { entity: PIPELINE.entity_id, name: "env", value: "prod-dryrun", base: "qa" } }]);
  expect(rows[0]!.textContent).toContain("prod-dryrun");
  expect(rows[0]!.textContent).toContain("queued");
  expect(toasts.items.at(-1)?.text).toBe(`Queued: set env on ${PIPELINE.name}.`);
  expect(toasts.items.at(-1)?.action?.label).toBe("Show queue");
});

test("Change… on a text parameter keeps the text input; a secret's is a password", async () => {
  const drawn = draw([PIPELINE.entity_id, "params"], "PAY-231", { settings: () => Promise.resolve(KINDS) });
  await settle();
  paramRows()[3]!.querySelector<HTMLButtonElement>("button")!.click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input[aria-label='New default for test.filter']")!;
  expect(input.type).toBe("text");
  input.value = "sepa::retry";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  button("Save")!.click();
  await settle();
  expect(drawn.writes).toEqual([{ SetParameter: { entity: PIPELINE.entity_id, name: "test.filter", value: "sepa::retry", base: "" } }]);
  paramRows()[2]!.querySelector<HTMLButtonElement>("button")!.click();
  flushSync();
  expect(target.querySelector<HTMLInputElement>("input[aria-label='New default for deploy.token']")!.type).toBe("password");
});

test("Change… while the source's sign-in fails says the new default waits there", async () => {
  health.patch({ source_id: "teamcity", state: "unauthorized", checked_at: null, detail: null, secret_expires_at: null });
  draw([PIPELINE.entity_id, "params"]);
  await settle();
  target.querySelector<HTMLButtonElement>("table[aria-label='Parameters'] tbody tr button")!.click();
  flushSync();
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='New default for env']")!;
  select.value = "prod-dryrun";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  button("Save")!.click();
  await settle();
  expect(toasts.items.at(-1)?.text).toBe(
    `Queued: set env on ${PIPELINE.name}. TeamCity sign-in is failing, so it waits there.`,
  );
});

test("Triggers: each by name with what it fires on, read only", async () => {
  draw([PIPELINE.entity_id, "triggers"]);
  await settle();
  const rows = [...target.querySelectorAll("table[aria-label='Triggers'] tr")].map((r) => [
    r.querySelector("b")?.textContent,
    r.querySelector(".tf-sub")?.textContent,
  ]);
  expect(rows).toEqual([
    ["VCS trigger", "branch filter +:refs/heads/*"],
    ["Schedule trigger", "when daily at 02:00"],
  ]);
  expect(target.textContent).toContain("Read only. Edit triggers in TeamCity.");
});

test("settings that cannot be read say so on their tab, and the runs still draw", async () => {
  draw([PIPELINE.entity_id, "triggers"], null, { settings: () => Promise.reject({ code: "unreachable", message: "connection refused" }) });
  await settle();
  expect(target.textContent).toContain("Triggers could not be read from TeamCity: connection refused");
  expect(target.querySelectorAll(".tf-bars a")).toHaveLength(30);
});

// -- the Run… dialog (#852) -------------------------------------------------------

const dialog = () => target.querySelector<HTMLElement>('[role="dialog"]');
const field = <T extends HTMLElement>(label: string) => {
  const at = [...dialog()!.querySelectorAll("label")].find((l) => l.textContent?.trim() === label);
  return dialog()!.querySelector<T>(`#${CSS.escape(at!.htmlFor)}`)!;
};
function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}
function choose(select: HTMLSelectElement, value: string) {
  select.value = value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}
const summary = () => dialog()!.querySelector(".tf-runsum")!.textContent;

test("Run… opens the dialog on the job's branch with each parameter typed by its spec; Run queues only what changed, drawn queued at once", async () => {
  const drawn = draw();
  await settle();
  button("Run…")!.click();
  await settle();
  expect(dialog()!.querySelector("h2")?.textContent).toBe("Run Payout_IntegrationTests");
  expect(field<HTMLSelectElement>("Start from").value).toBe("");
  expect(field<HTMLInputElement>("Branch").value).toBe("feature/PAY-231-sepa-retry");
  const env = field<HTMLSelectElement>("env");
  expect([...env.options].map((o) => o.value)).toEqual(["staging", "prod-dryrun"]);
  expect(field<HTMLInputElement>("deploy.token").type).toBe("password");
  expect(summary()).toBe("On feature/PAY-231-sepa-retry, defaults");
  expect(dialog()!.querySelector(".tf-chg")?.textContent).toBe("");

  choose(env, "prod-dryrun");
  expect(dialog()!.querySelector(".tf-chg")?.textContent).toBe("changed");
  expect(summary()).toBe("On feature/PAY-231-sepa-retry, with env=prod-dryrun");
  const clean = [...dialog()!.querySelectorAll<HTMLLabelElement>("label.tf-ck")].find((l) => l.textContent?.includes("Clean checkout"))!;
  clean.querySelector("input")!.click();
  flushSync();

  button("Run")!.click();
  await settle();
  expect(dialog()).toBeNull();
  expect(drawn.writes).toEqual([
    {
      RunBuild: {
        entity: PIPELINE.entity_id,
        branch: "feature/PAY-231-sepa-retry",
        parameters: [{ name: "env", value: "prod-dryrun" }],
        clean_checkout: true,
        first_in_queue: false,
      },
    },
  ]);
  const first = target.querySelector(".tf-runs tbody tr")!;
  expect(first.querySelector(".tf-run-no")?.textContent).toContain("queued");
});

test("the branch combobox: type-ahead with the default and the job's branch marked, arrows and Enter pick; the default runs as no branch", async () => {
  const drawn = draw();
  await settle();
  button("Run…")!.click();
  await settle();
  const branch = field<HTMLInputElement>("Branch");
  branch.dispatchEvent(new FocusEvent("focus"));
  type(branch, "");
  const options = () => [...dialog()!.querySelectorAll('[role="option"]')].map((o) => o.textContent?.replace(/\s+/g, " ").trim());
  expect(options()).toEqual(["maindefault", "feature/PAY-231-sepa-retryyour job’s branch", "release/1.9"]);
  type(branch, "rel");
  expect(options()).toEqual(["release/1.9"]);
  type(branch, "");
  branch.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  branch.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  branch.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  flushSync();
  expect(branch.value).toBe("main");
  expect(summary()).toBe("On main, defaults");
  button("Run")!.click();
  await settle();
  expect(drawn.writes).toEqual([
    { RunBuild: { entity: PIPELINE.entity_id, branch: null, parameters: [], clean_checkout: false, first_in_queue: false } },
  ]);
});

test("Run again… starts from that run's settings; a preset fills the form; Save as preset stores the overrides under the name", async () => {
  const saved: unknown[] = [];
  draw(undefined, "PAY-231", {
    runParameters: (runId) =>
      Promise.resolve(
        runId === "teamcity:build:1186"
          ? [
              { name: "env", value: "prod-dryrun" },
              { name: "deploy.token", value: "", secret: true },
            ]
          : [],
      ),
    presets: () =>
      Promise.resolve([
        {
          pipeline_id: PIPELINE.entity_id,
          name: "Only staging",
          branch: "release/1.9",
          parameters: [{ name: "env", value: "staging" }],
          clean_checkout: false,
          first_in_queue: true,
        },
        {
          pipeline_id: PIPELINE.entity_id,
          name: "Traced",
          branch: null,
          parameters: [{ name: "env.TRACE", value: "1" }],
          clean_checkout: false,
          first_in_queue: false,
        },
      ]),
    savePreset: (preset) => {
      saved.push(preset);
      return Promise.resolve(preset);
    },
  });
  await settle();
  const again = [...target.querySelectorAll(".tf-runs tbody tr")].find((row) => row.textContent?.includes("#1186"))!;
  [...again.querySelectorAll("button")].find((b) => b.textContent === "Run again…")!.click();
  await settle();
  expect(dialog()!.querySelector("h2")?.textContent).toBe("Run Payout_IntegrationTests again, from #1186");
  const start = field<HTMLSelectElement>("Start from");
  expect([...start.options].map((o) => o.textContent)).toEqual([
    "Defaults",
    "#1186 settings",
    "Preset: Only staging",
    "Preset: Traced",
  ]);
  expect(start.value).toBe("from");
  expect(field<HTMLInputElement>("Branch").value).toBe("main");
  expect(field<HTMLSelectElement>("env").value).toBe("prod-dryrun");
  expect(field<HTMLInputElement>("deploy.token").value).toBe("");

  choose(start, "preset:Only staging");
  expect(field<HTMLInputElement>("Branch").value).toBe("release/1.9");
  expect(field<HTMLSelectElement>("env").value).toBe("staging");

  // A preset saved on the default branch moves the branch back to it, and an
  // override of no declared parameter comes back as an added row (#852 review).
  choose(start, "preset:Traced");
  expect(field<HTMLInputElement>("Branch").value).toBe("main");
  expect(dialog()!.querySelector<HTMLInputElement>('[aria-label="Name of added parameter 1"]')?.value).toBe("env.TRACE");
  expect(summary()).toBe("On main, with env.TRACE=1");

  choose(start, "preset:Only staging");
  choose(start, "");
  expect(summary()).toBe("On release/1.9, defaults");

  choose(field<HTMLSelectElement>("env"), "prod-dryrun");
  const save = [...dialog()!.querySelectorAll<HTMLLabelElement>("label.tf-ck")].find((l) => l.textContent?.includes("Save these settings"))!;
  save.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  type(save.querySelector<HTMLInputElement>('input[type="text"]')!, "Prod dry run");
  button("Run")!.click();
  await settle();
  expect(saved).toEqual([
    {
      pipeline_id: PIPELINE.entity_id,
      name: "Prod dry run",
      branch: "release/1.9",
      parameters: [{ name: "env", value: "prod-dryrun" }],
      clean_checkout: false,
      first_in_queue: true,
    },
  ]);
});

test("Remove preset shows for a picked preset only, removes it by pipeline and name, and Start from forgets it while the form keeps it", async () => {
  const removed: unknown[] = [];
  const preset = (name: string) => ({
    pipeline_id: PIPELINE.entity_id,
    name,
    branch: null,
    parameters: [],
    clean_checkout: false,
    first_in_queue: false,
  });
  draw(undefined, null, {
    presets: () => Promise.resolve([preset("Only staging"), { ...preset("Traced"), parameters: [{ name: "env", value: "prod-dryrun" }] }]),
    deletePreset: (pipelineId, name) => {
      removed.push([pipelineId, name]);
      return Promise.resolve();
    },
  });
  await settle();
  button("Run…")!.click();
  await settle();
  expect(button("Remove preset")).toBeUndefined();
  const start = field<HTMLSelectElement>("Start from");
  choose(start, "preset:Traced");
  expect(button("Remove preset")).toBeDefined();
  button("Remove preset")!.click();
  await settle();
  expect(removed).toEqual([[PIPELINE.entity_id, "Traced"]]);
  expect([...start.options].map((o) => o.textContent)).toEqual(["Defaults", "Preset: Only staging"]);
  expect(start.value).toBe("");
  expect(button("Remove preset")).toBeUndefined();
  // The form keeps the removed preset's settings, so saving them brings it back.
  expect(summary()).toBe("On main, with env=prod-dryrun");
});

test("Remove preset refused: an already-gone preset leaves quietly, any other refusal says it failed and keeps the preset", async () => {
  toasts.items.length = 0;
  const preset = (name: string) => ({
    pipeline_id: PIPELINE.entity_id,
    name,
    branch: null,
    parameters: [],
    clean_checkout: false,
    first_in_queue: false,
  });
  let held = [preset("Gone"), preset("Kept")];
  draw(undefined, null, {
    presets: () => Promise.resolve(held),
    deletePreset: (_pipelineId, name) => {
      if (name === "Gone") {
        held = [preset("Kept")];
        return Promise.reject({ code: "not_found", message: "the pipeline has no preset named \"Gone\"", source_id: null });
      }
      return Promise.reject({ code: "not_ready", message: "the database is starting", source_id: null });
    },
  });
  await settle();
  button("Run…")!.click();
  await settle();
  const start = field<HTMLSelectElement>("Start from");
  choose(start, "preset:Gone");
  button("Remove preset")!.click();
  await settle();
  expect([...start.options].map((o) => o.textContent)).toEqual(["Defaults", "Preset: Kept"]);
  expect(start.value).toBe("");
  expect(toasts.items).toEqual([]);

  choose(start, "preset:Kept");
  button("Remove preset")!.click();
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Could not remove the preset: the database is starting"]);
  expect([...start.options].map((o) => o.textContent)).toEqual(["Defaults", "Preset: Kept"]);
  expect(start.value).toBe("preset:Kept");
});

test("Add a parameter sends a free name and value; a failing sign-in warns that the run waits in the queue", async () => {
  const drawn = draw(undefined, null, {
    sources: () => Promise.resolve([{ ...SOURCE, health: { ...SOURCE.health, state: "unauthorized", checked_at: null } }]),
  });
  await settle();
  button("Run…")!.click();
  await settle();
  expect(dialog()!.querySelector(".tf-banner.tf-warn")?.textContent?.trim()).toBe(
    "TeamCity sign-in is failing. The run waits in knobas’s write queue and starts once you re-enter the password.",
  );
  button("Add a parameter")!.click();
  flushSync();
  type(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Name of added parameter 1"]')!, "env.EXTRA");
  type(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Value of added parameter 1"]')!, "1");
  expect(summary()).toBe("On main, with env.EXTRA=1");
  button("Cancel")!.click();
  await settle();
  expect(dialog()).toBeNull();
  expect(drawn.writes).toEqual([]);
});

const CANNOT_RUN = "This TeamCity user cannot run builds in Payouts: it lacks the Run build permission";
const CANNOT_CHANGE = "This TeamCity user cannot change the settings of Payouts: it lacks the Edit project permission";

test("writes the source's credential cannot make are drawn disabled saying why, and queue nothing (#2040)", async () => {
  const asked: string[] = [];
  const drawn = draw([PIPELINE.entity_id], "PAY-231", {
    refusedWrites: (asks) => {
      asked.push(JSON.stringify(asks));
      return Promise.resolve(
        asks.map((ask) => ({ ...ask, reason: ask.op === "run_build" ? CANNOT_RUN : CANNOT_CHANGE })),
      );
    },
  });
  await settle();
  expect(asked).toEqual([
    JSON.stringify([
      { entity: PIPELINE.entity_id, op: "run_build" },
      { entity: PIPELINE.entity_id, op: "set_parameter" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "parameters" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "first_in_queue" },
    ]),
  ]);
  const runNow = button("Run…")!;
  const runOn = button("Run on feature/PAY-231-sepa-retry")!;
  const again = [...firstRow()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run again…")!;
  for (const refused of [runNow, runOn, again]) {
    expect(refused.getAttribute("aria-disabled")).toBe("true");
    expect(refused.title).toBe(CANNOT_RUN);
    refused.click();
  }
  await settle();
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  expect(drawn.writes).toEqual([]);
});

test("a parameter the credential cannot change offers Change… disabled saying why (#2040)", async () => {
  draw([PIPELINE.entity_id, "params"], "PAY-231", {
    settings: () => Promise.resolve(KINDS),
    refusedWrites: (asks) =>
      Promise.resolve(asks.filter((ask) => ask.op === "set_parameter").map((ask) => ({ ...ask, reason: CANNOT_CHANGE }))),
  });
  await settle();
  const changes = paramRows().flatMap((row) =>
    [...row.querySelectorAll<HTMLButtonElement>("button")].filter((b) => b.textContent === "Change…"),
  );
  expect(changes.length).toBeGreaterThan(0);
  for (const change of changes) {
    expect(change.getAttribute("aria-disabled")).toBe("true");
    expect(change.title).toBe(CANNOT_CHANGE);
    change.click();
  }
  await settle();
  expect(target.querySelector('input[aria-label^="New default"], select[aria-label^="New default"]')).toBeNull();
  expect(button("Run…")!.getAttribute("aria-disabled")).toBeNull();
});

test("a pipeline whose settings name no parameter op is asked about its runs alone (#2040)", async () => {
  const asked: string[] = [];
  draw([PIPELINE.entity_id], "PAY-231", {
    settings: () => Promise.resolve({ ...SETTINGS, parameter_write_op: null }),
    refusedWrites: (asks) => {
      asked.push(JSON.stringify(asks));
      return Promise.resolve([]);
    },
  });
  await settle();
  expect(asked).toEqual([
    JSON.stringify([
      { entity: PIPELINE.entity_id, op: "run_build" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "parameters" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "first_in_queue" },
    ]),
  ]);
});

const CANNOT_OVERRIDE =
  "This TeamCity user cannot override parameters for one build in Payouts: it lacks the Customize build parameters permission";
const CANNOT_PUT_FIRST = "This TeamCity user cannot put a build first in the queue: it lacks the Reorder builds in queue permission";

test("Run… draws the overrides and the queue's top disabled saying why where the credential cannot send them, and sends neither (#2055)", async () => {
  const asked: string[] = [];
  const drawn = draw([PIPELINE.entity_id], "PAY-231", {
    presets: () =>
      Promise.resolve([
        {
          pipeline_id: PIPELINE.entity_id,
          name: "Loud",
          branch: null,
          parameters: [
            { name: "env", value: "prod-dryrun" },
            { name: "env.TRACE", value: "1" },
          ],
          clean_checkout: false,
          first_in_queue: true,
        },
      ]),
    refusedWrites: (asks) => {
      asked.push(JSON.stringify(asks));
      const reasons: Record<string, string> = { parameters: CANNOT_OVERRIDE, first_in_queue: CANNOT_PUT_FIRST };
      return Promise.resolve(
        asks.flatMap((ask) => (ask.field !== undefined && reasons[ask.field] ? [{ ...ask, reason: reasons[ask.field]! }] : [])),
      );
    },
  });
  await settle();
  expect(asked).toEqual([
    JSON.stringify([
      { entity: PIPELINE.entity_id, op: "run_build" },
      { entity: PIPELINE.entity_id, op: "set_parameter" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "parameters" },
      { entity: PIPELINE.entity_id, op: "run_build", field: "first_in_queue" },
    ]),
  ]);
  expect(button("Run…")!.getAttribute("aria-disabled")).toBeNull();
  button("Run…")!.click();
  await settle();

  expect(dialog()!.querySelector(".tf-run-refused")?.textContent).toBe(CANNOT_OVERRIDE);
  const env = field<HTMLSelectElement>("env");
  expect(env.disabled).toBe(true);
  expect(env.value).toBe("staging");
  const add = button("Add a parameter")!;
  expect(add.getAttribute("aria-disabled")).toBe("true");
  expect(add.title).toBe(CANNOT_OVERRIDE);
  add.click();
  flushSync();
  expect(dialog()!.querySelector('input[aria-label="Name of added parameter 1"]')).toBeNull();
  const top = [...dialog()!.querySelectorAll<HTMLLabelElement>("label.tf-ck")].find((l) => l.textContent?.includes("Put first in the queue"))!;
  expect(top.title).toBe(CANNOT_PUT_FIRST);
  expect(top.querySelector("input")!.disabled).toBe(true);
  expect(summary()).toBe("On feature/PAY-231-sepa-retry, defaults");

  // A preset that overrides and goes first fills neither in.
  choose(field<HTMLSelectElement>("Start from"), "preset:Loud");
  expect(env.value).toBe("staging");
  expect(dialog()!.querySelector('input[aria-label="Name of added parameter 1"]')).toBeNull();
  expect(top.querySelector("input")!.checked).toBe(false);
  button("Run")!.click();
  await settle();
  expect(drawn.writes).toEqual([
    {
      RunBuild: {
        entity: PIPELINE.entity_id,
        branch: null,
        parameters: [],
        clean_checkout: false,
        first_in_queue: false,
      },
    },
  ]);
});
