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
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import type { PipelinePorts } from "./pipeline-ports";
import TfPipeline from "./TfPipeline.svelte";

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
    ...extra,
  };
}

const RUNS: RunRow[] = [
  run(1187, "err", today(0, 5), {
    branch: "feature/PAY-231-sepa-retry",
    duration_seconds: 252,
    trigger: "change",
    revision: "a41f2c0e9b8d7c6a",
  }),
  run(1186, "ok", daysAgo(1, 18), { duration_seconds: 241, trigger: "person", started_by: "mara", started_by_me: true }),
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
}

function draw(
  rest: string[] = [PIPELINE.entity_id],
  job: string | null = "PAY-231",
  overrides: Partial<PipelinePorts> = {},
  older: RunRow[][] = [[run(1150, "ok", daysAgo(9, 9))], []],
): Drawn {
  const drawn: Drawn = { pinned: 0, writes: [], olderAsked: [] };
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
    ...overrides,
  };
  page = mount(TfPipeline, {
    target,
    props: { pipeline: PIPELINE, rest, job, runs: () => Promise.resolve(RUNS), onpin: () => (drawn.pinned += 1), ports },
  });
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

test("Parameters: each default with its kind; Change… queues the new default against the old one; a secret is never drawn", async () => {
  const drawn = draw([PIPELINE.entity_id, "params"]);
  await settle();
  const rows = [...target.querySelectorAll("table[aria-label='Parameters'] tbody tr")];
  expect(rows.map((r) => r.querySelector("td")?.textContent)).toEqual(["env", "deploy.token"]);
  expect(rows[1]!.textContent).toContain("(hidden)");
  rows[0]!.querySelector<HTMLButtonElement>("button")!.click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input[aria-label='New default for env']")!;
  input.value = "prod-dryrun";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  button("Save")!.click();
  await settle();
  expect(drawn.writes).toEqual([
    { SetParameter: { entity: PIPELINE.entity_id, name: "env", value: "prod-dryrun", base: "staging" } },
  ]);
  expect(rows[0]!.textContent).toContain("prod-dryrun");
  expect(rows[0]!.textContent).toContain("queued");
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
