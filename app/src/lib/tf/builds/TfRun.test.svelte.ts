/**
 * The run page (#853) over the IPC mirror and the run's view: the header and
 * its writes, the Overview leading with the first failed test, the Run and
 * Artifacts panels, and the Changes and Parameters tabs.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DownloadTarget, LinkEntry, PipelineRow, Project, RunLog, PipelineSettings, RunRow, SourceView } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { noteWithdrawn } from "../../shell/write-queue.svelte";
import { forgetQueued, queuedMarks } from "../queued.svelte";
import type { RunPorts } from "./pipeline-ports";
import { QUEUED_RUN } from "./pipeline";
import TfRun from "./TfRun.svelte";
import type { WriteAsk } from "../../shell/refused-writes";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  toasts.items = [];
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
  forgetQueued();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const FEATURE = "feature/PAY-231-sepa-retry";
const PIPE = "teamcity:buildType:Payout_IntegrationTests";

function run(no: number, tone: "ok" | "err" | null, branch: string, extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: `teamcity:build:${no}`,
    number: String(no),
    state: tone === null ? { text: "running", tone: "info" } : { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch,
    started: "2026-10-16T08:01:00Z",
    finished: tone === null ? null : "2026-10-16T08:05:12Z",
    duration_seconds: tone === null ? null : 252,
    updated_at: "2026-10-16T08:05:12Z",
    web_url: `http://ci.example/build/${no}`,
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
  run(1188, null, FEATURE, { agent: "linux-agent-01", current_step: "Step 3 of 5 is running." }),
  run(1187, "err", FEATURE, {
    trigger: "change",
    revision: "a41f2c0e9b8d7c6a",
    commit_id: "gitea:commit:a41f2c",
    agent: "linux-agent-03",
  }),
  run(1186, "ok", "main", { duration_seconds: 241 }),
  run(1185, "err", FEATURE, { duration_seconds: 268 }),
];

const PIPELINE: PipelineRow = {
  entity_id: PIPE,
  source_id: "teamcity",
  source_name: "TeamCity",
  name: "Payout_IntegrationTests",
  group: "Payouts",
  path: ["Payments", "Payouts"],
  pinned: false,
  web_url: null,
  recent: RUNS,
};

const SETTINGS: PipelineSettings = {
  repositories: [{ name: "payout-service", url: null, default_branch: "refs/heads/main", entity_id: "gitea:repo:payout-service" }],
  triggers: [],
  parameters: [
    { name: "env", value: "staging", inherited: false, secret: false, editable: true, description: null },
    { name: "test.filter", value: "", inherited: false, secret: false, editable: true, description: null },
  ],
  parameter_write_op: "set_parameter",
  parameter_specs: [],
  chain: [],
  steps: [],
};

function viewOf(running: boolean): SourceView {
  return {
    actions: [],
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [
          {
            type: "facts",
            facts: [
              // The source's own facts and commit row, which the shell no
              // longer reads: who started a run, its agent, what it is doing
              // now and its commit are the run's declared fields (#961, #1004).
              { label: "Triggered by", value: "the view's own words", tone: null },
              ...(running ? [{ label: "Now", value: "the view's own step", tone: null }] : []),
              { label: "Agent", value: "the view's own agent", tone: null },
            ],
          },
          { type: "entities", title: "Configuration", rows: [{ entity_id: PIPE, title: "Payouts / X", meta: null, tone: null, chip: null, url: null }] },
          { type: "entities", title: "Revision", rows: [{ entity_id: "gitea:commit:view", title: "payout-service", meta: null, tone: null, chip: "view-sha", url: null }] },
        ],
      },
      {
        id: "changes",
        title: "Changes",
        count: 1,
        deferred: false,
        blocks: [
          { type: "entities", title: null, rows: [{ entity_id: "gitea:commit:a41f2c", title: "PAY-231 backoff jitter", meta: "mara", tone: null, chip: "a41f2c", url: null }] },
        ],
      },
      { id: "tests", title: "Tests", count: running ? 0 : 1, deferred: true, blocks: [] },
      {
        id: "artifacts",
        title: "Artifacts",
        count: 5,
        deferred: false,
        blocks: [
          {
            type: "entities",
            title: null,
            rows: ["a.xml", "b.html", "c.log", "d.log", "e.info"].map((name) => ({ entity_id: null, title: name, meta: "1 KB", tone: null, chip: null, url: `http://ci.example/a/${name}` })),
          },
        ],
      },
      {
        id: "parameters",
        title: "Parameters",
        count: 2,
        deferred: false,
        blocks: [
          {
            type: "settings",
            title: null,
            entity: "teamcity:build:1187",
            write_op: "",
            values: [
              { name: "env", value: "prod-dryrun", inherited: false, secret: false, editable: false, description: null },
              { name: "test.filter", value: "", inherited: false, secret: false, editable: false, description: null },
            ],
          },
        ],
      },
    ],
  };
}

const TESTS: SourceView = {
  actions: [],
  sections: [
    {
      id: "tests",
      title: "Tests",
      count: 1,
      deferred: false,
      blocks: [
        { type: "entities", title: null, rows: [{ entity_id: null, title: "sepa::retry::gives_up_after_max_attempts", meta: null, tone: "err", chip: "new", url: null }] },
        { type: "text", markdown: false, text: "assertion `left == right` failed\n  left: 6\n right: 5\n  at tests/sepa_retry.rs:14:5" },
      ],
    },
  ],
};

interface Drawn {
  downloads: DownloadTarget[];
  writes: WriteOpPayload[];
  opened: string[];
  views: string[];
  /** Moves the page to another tab of the same run, as the address does. */
  go: (tab?: string) => void;
  /** Moves the page to another run of the pipeline. */
  goRun: (runId: string) => void;
  /** How many times the pipeline's runs were read. */
  reads: number;
  /** What the next runs read answers. */
  next: RunRow[];
  /** Fire `sync:state` as the shell hears it, to every listener the page holds. */
  sync: (payload: unknown) => void;
  /** How many `sync:state` listeners the page holds. */
  listening: () => number;
}

function draw(runId = "teamcity:build:1187", tab?: string, overrides: Partial<RunPorts> = {}, runs: RunRow[] = RUNS): Drawn {
  const props = $state({ rest: [PIPE, runId, ...(tab ? [tab] : [])] });
  const handlers = new Set<(event: { payload: unknown }) => void>();
  const drawn: Drawn = {
    downloads: [],
    writes: [],
    opened: [],
    views: [],
    go: (to) => (props.rest = [PIPE, props.rest[1]!, ...(to ? [to] : [])]),
    goRun: (to) => (props.rest = [PIPE, to]),
    reads: 0,
    next: runs,
    sync: (payload) => {
      for (const handler of [...handlers]) handler({ payload });
    },
    listening: () => handlers.size,
  };
  const ports: RunPorts = {
    older: () => Promise.resolve([]),
    settings: () => Promise.resolve(SETTINGS),
    adapters: () => Promise.resolve([]),
    sources: () => Promise.resolve([]),
    syncStatus: () => Promise.resolve([]),
    jobBranches: () => Promise.resolve([FEATURE]),
    submitWrite: (payload) => {
      drawn.writes.push(payload);
      return Promise.resolve({ id: 9 } as QueuedWrite);
    },
    openExternal: (url) => {
      drawn.opened.push(url);
      return Promise.resolve();
    },
    presets: () => Promise.resolve([]),
    savePreset: (preset) => Promise.resolve(preset),
    deletePreset: () => Promise.resolve(),
    repoBranches: () => Promise.resolve(["main", FEATURE]),
    runParameters: () => Promise.resolve([{ name: "env", value: "prod-dryrun" }]),
    log: () => Promise.resolve(null),
    tests: () => Promise.resolve(null),
    history: () => Promise.resolve(null),
    artifacts: () => Promise.resolve([]),
    preview: () => Promise.resolve(null),
    download: (_runId, target) => {
      drawn.downloads.push(target);
      return Promise.resolve(`/Users/mara/Downloads/${target.kind === "file" ? target.path : target.kind}`);
    },
    view: (id, section) => {
      drawn.views.push(section ?? "");
      if (section === "tests") return Promise.resolve(TESTS);
      return Promise.resolve(viewOf(id === "teamcity:build:1188"));
    },
    listen: (event, handler) => {
      if (event !== "sync:state") return Promise.resolve(() => {});
      const mine = (e: { payload: unknown }) => handler(e);
      handlers.add(mine);
      return Promise.resolve(() => void handlers.delete(mine));
    },
    follower: () => ({ follow: () => () => {} }),
    projects: () => Promise.resolve(PROJECTS),
    links: () => Promise.resolve([]),
    owedTickets: () => Promise.resolve([]),
    refusedWrites: () => Promise.resolve([]),
    ...overrides,
  };
  const crumbs = createRawSnippet((parts: () => { name: string }[]) => ({
    render: () => `<nav class="tf-crumbs"></nav>`,
    setup: (nav) => {
      $effect(() => {
        nav.textContent = parts()
          .map((p) => p.name)
          .join(" › ");
      });
    },
  }));
  page = mount(TfRun, {
    target,
    props: {
      pipeline: PIPELINE,
      get rest() {
        return props.rest;
      },
      job: "PAY-231",
      runs: () => {
        drawn.reads += 1;
        return Promise.resolve(drawn.next);
      },
      crumbs,
      ports,
    },
  });
  return drawn;
}

function button(name: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);
}

// -- the header (R-9.9) --------------------------------------------------------

test("the header names the pipeline and number with the result, then branch, who started it, when and how long", async () => {
  draw();
  await settle();
  expect(target.querySelector(".tf-crumbs")?.textContent).toBe("Builds › Payout_IntegrationTests › #1187");
  const h1 = target.querySelector("h1")!;
  expect(h1.textContent).toContain("Payout_IntegrationTests #1187");
  expect(h1.querySelector(".tf-chip.tf-err")?.textContent).toBe("FAILURE");
  const sub = target.querySelector(".tf-bhead .tf-sub")!.textContent!;
  expect(sub).toMatch(/^feature\/PAY-231-sepa-retry, started by VCS change a41f2c, .+, took 4 m 12 s$/);
  expect(button("Re-run")).toBeDefined();
  expect(button("Run again with changes…")).toBeDefined();
  expect(button("Stop")).toBeUndefined();
});

test("Re-run queues the run's branch and values again, and says so", async () => {
  const drawn = draw();
  await settle();
  button("Re-run")!.click();
  await settle();
  expect(drawn.writes).toEqual([
    {
      RunBuild: {
        entity: PIPE,
        branch: FEATURE,
        parameters: [{ name: "env", value: "prod-dryrun" }],
        clean_checkout: false,
        first_in_queue: false,
      },
    },
  ]);
  expect(toasts.items.at(-1)!.text).toBe("Queued: run Payout_IntegrationTests. Re-run of #1187, same settings.");
});

test("a run the source's credential cannot make draws Re-run and Run again… disabled saying why, and queues nothing (#2040)", async () => {
  const reason = "This TeamCity user cannot run builds in Payouts: it lacks the Run build permission";
  const asked: string[] = [];
  const drawn = draw("teamcity:build:1187", "params", {
    refusedWrites: (asks) => {
      asked.push(JSON.stringify(asks));
      return Promise.resolve(asks.map((ask) => ({ ...ask, reason })));
    },
  });
  await settle();
  expect(asked).toEqual([
    JSON.stringify([
      { entity: PIPE, op: "run_build" },
      { entity: PIPE, op: "run_build", field: "parameters" },
      { entity: PIPE, op: "run_build", field: "first_in_queue" },
    ]),
  ]);
  for (const words of ["Re-run", "Run again with changes…", "Run again with these…"]) {
    const refused = button(words)!;
    expect(refused.getAttribute("aria-disabled")).toBe("true");
    expect(refused.title).toBe(reason);
    refused.click();
  }
  await settle();
  expect(target.querySelector(".tf-run-dialog")).toBeNull();
  expect(drawn.writes).toEqual([]);
});

test("Run again with changes… opens the Run… dialog from this run's settings", async () => {
  draw();
  await settle();
  button("Run again with changes…")!.click();
  await settle();
  const start = target.querySelector<HTMLSelectElement>(".tf-run-dialog select")!;
  expect(start.selectedOptions[0]?.textContent).toBe("#1187 settings");
  expect(target.querySelector<HTMLSelectElement>('.tf-run-dialog [aria-label="env"]')?.value ?? "").toBe("prod-dryrun");
});

test("Run again with changes… from a credential that may not override sends this run's branch without its parameters, saying why (#2055)", async () => {
  const reason = "This TeamCity user cannot override parameters for one build in Payouts: it lacks the Customize build parameters permission";
  const drawn = draw("teamcity:build:1187", "overview", {
    refusedWrites: (asks) => Promise.resolve(asks.filter((ask) => ask.field === "parameters").map((ask) => ({ ...ask, reason }))),
  });
  await settle();
  expect(button("Run again with changes…")!.getAttribute("aria-disabled")).toBeNull();
  button("Run again with changes…")!.click();
  await settle();
  expect(target.querySelector(".tf-run-dialog .tf-run-refused")?.textContent).toBe(reason);
  const env = target.querySelector<HTMLSelectElement>('.tf-run-dialog [aria-label="env"]')!;
  expect(env.disabled).toBe(true);
  expect(env.value).not.toBe("prod-dryrun");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-run-dialog button")].find((b) => b.textContent?.trim() === "Run")!.click();
  await settle();
  expect(drawn.writes.map((write) => ("RunBuild" in write ? write.RunBuild.parameters : null))).toEqual([[]]);
});

test("Re-run of a run that carried overrides, from a credential that may not send them, is drawn disabled saying why and queues nothing (#2055)", async () => {
  const reason = "This TeamCity user cannot override parameters for one build in Payouts: it lacks the Customize build parameters permission";
  const drawn = draw("teamcity:build:1187", "overview", {
    refusedWrites: (asks) => Promise.resolve(asks.filter((ask) => ask.field === "parameters").map((ask) => ({ ...ask, reason }))),
  });
  await settle();
  const rerun = button("Re-run")!;
  expect(rerun.getAttribute("aria-disabled")).toBe("true");
  expect(rerun.title).toBe(reason);
  rerun.click();
  await settle();
  expect(drawn.writes).toEqual([]);
});

test("Open in the source opens the run's own page", async () => {
  const drawn = draw();
  await settle();
  button("Open in TeamCity")!.click();
  expect(drawn.opened).toEqual(["http://ci.example/build/1187"]);
});

test("a running run has Stop, disabled because it needs a new write, and says what it is doing", async () => {
  draw("teamcity:build:1188");
  await settle();
  const stop = button("Stop")!;
  expect(stop.getAttribute("aria-disabled")).toBe("true");
  expect(stop.title).toBe("Needs a new write");
  expect(button("Re-run")).toBeUndefined();
  expect(target.querySelector(".tf-callout.tf-info")?.textContent).toBe("Step 3 of 5 is running.");
  expect(target.querySelector(".tf-problem")).toBeNull();
});

// -- Overview (R-9.11) ---------------------------------------------------------

test("a failed run's Overview leads with its first failed test, where it is, and how the runs before it went", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.views).toEqual(["", "tests"]);
  const problem = target.querySelector(".tf-problem")!;
  expect(problem.querySelector("h3")?.textContent).toBe("1 test failed: sepa::retry::gives_up_after_max_attempts");
  expect(problem.querySelector("pre")?.textContent).toContain("at tests/sepa_retry.rs:14:5");
  const links = [...problem.querySelectorAll("a")].map((a) => [a.textContent, a.getAttribute("href")]);
  expect(links).toEqual([
    ["Show it in the log", "#/build/teamcity%3AbuildType%3APayout_IntegrationTests/teamcity%3Abuild%3A1187/log"],
    ["Open the test file", "#/repo/gitea%3Arepo%3Apayout-service/files/tests/sepa_retry.rs"],
  ]);
  expect(problem.querySelector(".tf-dim")?.textContent).toBe("Also failed in #1185 on this branch. #1186 on main passed.");
});

test("the Run panel lists branch, commit, who started it, agent, start and took against the run before", async () => {
  draw();
  await settle();
  const panel = target.querySelector('[aria-label="Run"].tf-panel')!;
  const facts = Object.fromEntries(
    [...panel.querySelectorAll("dt")].map((dt) => [dt.textContent, (dt.nextElementSibling as HTMLElement).textContent?.trim()]),
  );
  expect(facts["Branch"]).toBe(FEATURE);
  expect(facts["Commit"]).toBe("a41f2c");
  expect(facts["Started by"]).toBe("VCS change a41f2c");
  expect(facts["Agent"]).toBe("linux-agent-03");
  expect(facts["Took"]).toBe("4 m 12 s, −16 s against #1185");
  expect(panel.querySelector("dd a")?.getAttribute("href")).toBe("#/repo/gitea%3Arepo%3Apayout-service/commits/gitea%3Acommit%3Aa41f2c");
});

// #1154: a pipeline of two mirrored repositories whose run built the second.
const TWO_REPOS: PipelineSettings = {
  ...SETTINGS,
  repositories: [
    { name: "payout-service", url: null, default_branch: "refs/heads/main", entity_id: "gitea:tidewater/payout-service" },
    { name: "ledger", url: null, default_branch: "refs/heads/main", entity_id: "gitea:tidewater/ledger" },
  ],
};
const LEDGER_COMMIT = "gitea:tidewater/ledger@a41f2c0e9b8d7c6a";
const TWO_REPO_RUNS: RunRow[] = RUNS.map((r) => (r.entity_id === "teamcity:build:1187" ? { ...r, commit_id: LEDGER_COMMIT } : r));

test("a run whose commit sits in the pipeline's second repository links it under that repository", async () => {
  draw("teamcity:build:1187", undefined, { settings: () => Promise.resolve(TWO_REPOS) }, TWO_REPO_RUNS);
  await settle();
  const panel = target.querySelector('[aria-label="Run"].tf-panel')!;
  expect(panel.querySelector("dd a")?.getAttribute("href")).toBe(
    `#/repo/gitea%3Atidewater%2Fledger/commits/${encodeURIComponent(LEDGER_COMMIT)}`,
  );
  const open = [...target.querySelectorAll("a")].find((a) => a.textContent === "Open the test file");
  expect(open?.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fledger/files/tests/sepa_retry.rs");
});

test("Changes without a commit id of their own follow the run's repository; a commit naming no repository falls back to the first", async () => {
  draw("teamcity:build:1187", "changes", { settings: () => Promise.resolve(TWO_REPOS) }, TWO_REPO_RUNS);
  await settle();
  expect(target.querySelector('[aria-label="Changes"] a')?.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fledger/commits/gitea%3Acommit%3Aa41f2c",
  );
  if (page) unmount(page);
  target.replaceChildren();
  draw("teamcity:build:1187", undefined, { settings: () => Promise.resolve(TWO_REPOS) });
  await settle();
  expect(target.querySelector('[aria-label="Run"].tf-panel dd a')?.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3Acommit%3Aa41f2c",
  );
});

test("the Artifacts panel lists the first four with Download through knobas, and links to all of them", async () => {
  const drawn = draw();
  await settle();
  const panel = target.querySelector('[aria-label="Artifacts"].tf-panel')!;
  expect([...panel.querySelectorAll(".tf-r .tf-t")].map((t) => t.textContent)).toEqual(["a.xml", "b.html", "c.log", "d.log"]);
  expect(panel.querySelector(".tf-open")?.textContent).toBe("All 5");
  panel.querySelector<HTMLButtonElement>(".tf-r button")!.click();
  await settle();
  // D8 (#856): through knobas into the downloads folder, not the source's page.
  expect(drawn.opened).toEqual([]);
  expect(drawn.downloads).toEqual([{ kind: "file", path: "a.xml" }]);
  expect(toasts.items.at(-1)?.text).toBe("Saved a.xml to ~/Downloads");
  expect(target.querySelector('.tf-tabs [href$="/tests"]')?.textContent).toBe("Tests 1 failed");
});

test("a run older than the latest page is looked for in the older pages", async () => {
  const old = run(1100, "ok", "main");
  const asked: string[] = [];
  draw("teamcity:build:1100", undefined, {
    older: (_pipe, before) => {
      asked.push(before);
      return Promise.resolve(asked.length === 1 ? [old] : []);
    },
  });
  await settle();
  expect(asked).toEqual(["teamcity:build:1185"]);
  expect(target.querySelector("h1")?.textContent).toContain("#1100");
});

test("an address naming no run of the pipeline says so and leads back to it", async () => {
  draw("teamcity:build:1", undefined, {}, []);
  await settle();
  expect(target.querySelector(".tf-empty")?.textContent).toContain("No run of Payout_IntegrationTests at this address.");
});

// -- Changes and Parameters (R-9.11) ------------------------------------------

test("Changes lists the commits in the run, each opening its commit", async () => {
  draw("teamcity:build:1187", "changes");
  await settle();
  const cells = [...target.querySelectorAll('[aria-label="Changes"] td')].map((td) => td.textContent?.trim());
  expect(cells).toEqual(["a41f2c", "PAY-231 backoff jitter", "mara"]);
  expect(target.querySelector('[aria-label="Changes"] a')?.getAttribute("href")).toBe(
    "#/repo/gitea%3Arepo%3Apayout-service/commits/gitea%3Acommit%3Aa41f2c",
  );
});

test("Parameters pairs each value in this run with its default, then the branch, and runs again from them", async () => {
  draw("teamcity:build:1187", "params");
  await settle();
  const rows = [...target.querySelectorAll('[aria-label="Parameters"] tbody tr')].map((tr) =>
    [...tr.querySelectorAll("td")].map((td) => td.textContent?.trim()),
  );
  expect(rows).toEqual([
    ["env", "prod-dryrun", "staging"],
    ["test.filter", "(empty)", "(empty)"],
    ["branch", FEATURE, "main"],
  ]);
  button("Run again with these…")!.click();
  await settle();
  expect(target.querySelector(".tf-run-dialog")).not.toBeNull();
});

test("the Log, Tests and Artifacts tabs read the run live; the log is read once, for the steps bar and the Log tab", async () => {
  const asked: string[] = [];
  for (const tab of ["log", "tests", "artifacts"]) {
    draw("teamcity:build:1187", tab, {
      log: (id) => (asked.push(`log ${id}`), Promise.resolve(null)),
      tests: (id) => (asked.push(`tests ${id}`), Promise.resolve(null)),
      artifacts: (id, path) => (asked.push(`artifacts ${id} ${JSON.stringify(path)}`), Promise.resolve([])),
    });
    await settle();
    expect(target.querySelector('.tf-tabs [aria-current="page"]')?.textContent?.startsWith(tab === "log" ? "Log" : tab === "tests" ? "Tests" : "Artifacts")).toBe(true);
    unmount(page!);
    page = null;
  }
  expect(asked).toEqual([
    "log teamcity:build:1187",
    "log teamcity:build:1187",
    "tests teamcity:build:1187",
    "log teamcity:build:1187",
    'artifacts teamcity:build:1187 ""',
  ]);
});

// -- the steps bar (#1003, R-9.10) ---------------------------------------------

const STEPPED: RunLog = {
  total: 9,
  steps: [
    { name: "Checkout", first: 1, last: 2, seconds: 12, tone: null },
    { name: "cargo build --tests", first: 3, last: 5, seconds: 130, tone: "warn" },
    { name: "cargo test --test sepa_retry", first: 6, last: 9, seconds: 82, tone: "err" },
  ],
  lines: Array.from({ length: 9 }, (_, i) => ({
    number: i + 1,
    time: "10:06:00",
    text: i === 8 ? "Process exited with code 101" : `line ${i + 1}`,
    tone: i === 8 ? ("err" as const) : null,
    step: i < 2 ? 0 : i < 5 ? 1 : 2,
  })),
};

function segments(): SVGAElement[] {
  return [...target.querySelectorAll<SVGAElement>('[aria-label="Steps"] a')];
}

test("under the header, the steps bar draws one segment per step of the run's log, toned by result, the duration as its tip", async () => {
  draw("teamcity:build:1187", undefined, { log: () => Promise.resolve(STEPPED) });
  await settle();
  const drawn = segments();
  expect(drawn.map((a) => a.querySelector("text")?.textContent)).toEqual(["Checkout", "cargo build --tests", "cargo test --test sepa_retry"]);
  expect(drawn.map((a) => a.querySelector("title")?.textContent)).toEqual([
    "Checkout, 12 s",
    "cargo build --tests, 2 m 10 s",
    "cargo test --test sepa_retry, 1 m 22 s",
  ]);
  expect(drawn.map((a) => a.querySelector("rect")?.getAttribute("class"))).toEqual(["tf-sbseg tf-ok", "tf-sbseg tf-ok", "tf-sbseg tf-err"]);
  // Width by duration: 130 s of 224 s.
  expect(parseFloat(drawn[1]!.querySelector("svg")!.getAttribute("width")!)).toBeCloseTo(58.04, 1);
  expect(drawn.every((a) => a.getAttribute("href") === `#/build/${encodeURIComponent(PIPE)}/${encodeURIComponent("teamcity:build:1187")}/log`)).toBe(true);
});

test("a run in flight draws its last step running", async () => {
  const live: RunLog = { ...STEPPED, steps: STEPPED.steps.map((s, i) => (i === 2 ? { ...s, tone: null, seconds: null } : s)) };
  draw("teamcity:build:1188", undefined, { log: () => Promise.resolve(live) });
  await settle();
  const last = segments().at(-1)!;
  expect(last.querySelector("rect")?.getAttribute("class")).toBe("tf-sbseg tf-run");
  expect(last.querySelector("title")?.textContent).toBe("cargo test --test sepa_retry, running");
});

test("a run in flight draws the pipeline's steps its log has not reached in grey, not started, and one opens the Log tab with no fold", async () => {
  const scrolled: string[] = [];
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this.getAttribute("data-s") ?? "");
  };
  const live: RunLog = { ...STEPPED, steps: STEPPED.steps.map((s, i) => (i === 2 ? { ...s, tone: null, seconds: null } : s)) };
  const steps = [...STEPPED.steps.map((s) => s.name), "Publish test report"];
  const drawn = draw("teamcity:build:1188", undefined, {
    log: () => Promise.resolve(live),
    settings: () => Promise.resolve({ ...SETTINGS, steps }),
  });
  await settle();
  const wait = segments().at(-1)!;
  expect(segments()).toHaveLength(4);
  expect(wait.querySelector("text")?.textContent).toBe("Publish test report");
  expect(wait.querySelector("title")?.textContent).toBe("Publish test report, not started");
  expect(wait.getAttribute("aria-label")).toBe("Publish test report, not started");
  expect(wait.querySelector("rect")?.getAttribute("class")).toBe("tf-sbseg tf-wait");
  expect(wait.getAttribute("href")).toBe(`#/build/${encodeURIComponent(PIPE)}/${encodeURIComponent("teamcity:build:1188")}/log`);
  wait.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  drawn.go("log");
  await settle();
  expect(target.querySelectorAll("details[data-s]")).toHaveLength(3);
  expect(scrolled).toEqual([]);
});

test("a run whose source keeps no log, or whose log has no steps, draws no steps bar", async () => {
  draw("teamcity:build:1187", undefined, { log: () => Promise.resolve(null) });
  await settle();
  expect(target.querySelector("h1")).not.toBeNull();
  expect(target.querySelector('[aria-label="Steps"]')).toBeNull();
  unmount(page!);
  page = null;
  draw("teamcity:build:1187", undefined, { log: () => Promise.resolve({ ...STEPPED, steps: [] }) });
  await settle();
  expect(target.querySelector('[aria-label="Steps"]')).toBeNull();
});

test("a click on a step opens the Log tab at that step, open and scrolled to", async () => {
  const scrolled: string[] = [];
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this.getAttribute("data-s") ?? "");
  };
  const drawn = draw("teamcity:build:1187", undefined, { log: () => Promise.resolve(STEPPED) });
  await settle();
  segments()[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  drawn.go("log");
  await settle();
  const fold = target.querySelector<HTMLDetailsElement>('details[data-s="1"]')!;
  expect(fold.open).toBe(true);
  expect(scrolled).toEqual(["1"]);
  // The failed step opens by itself; the first stays shut.
  expect(target.querySelector<HTMLDetailsElement>('details[data-s="0"]')!.open).toBe(false);
});

test("a run in flight is read again when the Log tab opens, and the bar follows the newer read", async () => {
  const answers: RunLog[] = [
    { ...STEPPED, steps: STEPPED.steps.slice(0, 2).map((s) => ({ ...s, tone: null })) },
    { ...STEPPED, steps: STEPPED.steps.map((s) => ({ ...s, tone: null })) },
  ];
  let reads = 0;
  const drawn = draw("teamcity:build:1188", undefined, { log: () => Promise.resolve(answers[Math.min(reads++, 1)]!) });
  await settle();
  expect(segments()).toHaveLength(2);
  drawn.go("log");
  await settle();
  expect(reads).toBe(2);
  expect(segments()).toHaveLength(3);
});

// -- a sync finishing reads again (#1450) ---------------------------------------

/** A `sync:state` payload for `source`. */
const synced = (source: string, running = false) => ({
  source_id: source,
  running,
  run_id: running ? 7 : null,
  started_at: null,
  last_finished_at: "2026-10-16T08:10:00Z",
  last_outcome: running ? null : "ok",
  next_run_at: null,
  backoff_until: null,
});

/** The running run 1188 as the next sync left it: passed. */
const LANDED: RunRow[] = RUNS.map((r) =>
  r.entity_id === "teamcity:build:1188" ? run(1188, "ok", FEATURE, { duration_seconds: 300, agent: "linux-agent-01" }) : r,
);

test("a sync of the run's source finishing reads the run again: a running run draws its result", async () => {
  const views: string[] = [];
  const drawn = draw("teamcity:build:1188", undefined, {
    view: (_id, section) => (views.push(section ?? ""), Promise.resolve(viewOf(views.length === 1))),
  });
  await settle();
  expect(button("Stop")).toBeDefined();
  expect(target.querySelector(".tf-callout.tf-info")?.textContent).toBe("Step 3 of 5 is running.");

  drawn.next = LANDED;
  drawn.sync(synced("teamcity", true));
  await settle();
  expect(drawn.reads, "a sync still running reads nothing").toBe(1);

  drawn.sync(synced("teamcity"));
  await settle();
  expect(drawn.reads).toBe(2);
  expect(views, "the view again, and its failed test now that it finished").toEqual(["", "", "tests"]);
  expect(target.querySelector("h1 .tf-chip")?.textContent).toBe("SUCCESS");
  expect(target.querySelector(".tf-bhead .tf-sub")!.textContent).toContain("took 5 m");
  expect(button("Stop")).toBeUndefined();
  expect(button("Re-run")).toBeDefined();
  expect(target.querySelector(".tf-callout.tf-info")).toBeNull();
});

test("a sync of another source reads nothing", async () => {
  const drawn = draw();
  await settle();
  drawn.sync(synced("gitea"));
  await settle();
  expect(drawn.reads).toBe(1);
  expect(drawn.views).toEqual(["", "tests"]);
});

test("the open tab reads again too, on the tab the reader is on", async () => {
  const asked: string[] = [];
  const reads = {
    log: (id: string) => (asked.push(`log ${id}`), Promise.resolve(null)),
    tests: (id: string) => (asked.push(`tests ${id}`), Promise.resolve(null)),
    artifacts: (id: string, path: string) => (asked.push(`artifacts ${id} ${JSON.stringify(path)}`), Promise.resolve([])),
  };
  for (const tab of ["log", "tests", "artifacts"]) {
    asked.length = 0;
    const drawn = draw("teamcity:build:1187", tab, reads);
    await settle();
    drawn.sync(synced("teamcity"));
    await settle();
    expect(target.querySelector('.tf-tabs [aria-current="page"]')?.getAttribute("href")).toMatch(new RegExp(`/${tab}$`));
    const own = asked.filter((line) => line.startsWith(tab));
    expect(own, `${tab} is read again`).toHaveLength(2);
    unmount(page!);
    page = null;
  }
});

test("the page stops listening when the run changes or the page goes", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.listening()).toBe(1);
  drawn.goRun("teamcity:build:1186");
  await settle();
  expect(drawn.listening(), "the last run's listener is gone").toBe(1);
  unmount(page!);
  page = null;
  expect(drawn.listening()).toBe(0);
});

test("the Log tab draws the log read again, and the steps bar follows it", async () => {
  const answers: RunLog[] = [{ total: 0, steps: [], lines: [] }, STEPPED];
  const drawn = draw("teamcity:build:1187", "log", { log: () => Promise.resolve(answers.shift() ?? STEPPED) });
  await settle();
  expect(target.querySelector(".tf-runbody")?.textContent).toContain("This run has no log.");
  expect(segments()).toHaveLength(0);
  drawn.sync(synced("teamcity"));
  await settle();
  expect(target.querySelector(".tf-runbody")?.textContent).toContain("Process exited with code 101");
  expect(segments()).toHaveLength(3);
});

// -- a run queued here shows on the pipeline page (#1449) -----------------------

test("Re-run leaves the run queued on its pipeline, from the newest run held, and draws no queued row here", async () => {
  draw();
  await settle();
  button("Re-run")!.click();
  await settle();
  expect(queuedMarks(PIPE, QUEUED_RUN)).toMatchObject([{ write: 9, kind: QUEUED_RUN, text: FEATURE, base: "teamcity:build:1188" }]);
  expect(target.textContent).not.toContain("queued");
});

test("Run in Run again with changes… leaves the run queued on its pipeline too, the default branch as none", async () => {
  draw();
  await settle();
  button("Run again with changes…")!.click();
  await settle();
  const branch = target.querySelector<HTMLInputElement>('.tf-run-dialog input[id^="tf-run-branch-"]')!;
  branch.value = "main";
  branch.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-run-dialog button")].find((b) => b.textContent?.trim() === "Run")!.click();
  await settle();
  expect(queuedMarks(PIPE, QUEUED_RUN)).toMatchObject([{ write: 9, kind: QUEUED_RUN, text: "", base: "teamcity:build:1188" }]);
});

// -- Create ticket (R-9.11, #1451) ---------------------------------------------

const PROJECTS: Project[] = [
  { source_id: "jira", key: "PAY", name: "Payments" },
  { source_id: "jira", key: "WEB", name: "Web" },
] as Project[];

const FILED_TITLE = "sepa::retry::gives_up_after_max_attempts fails in Payout_IntegrationTests #1187";

/** A confirmed link from a ticket to the run, as `get_entity`'s `links` reads it on the run. */
function ticketLink(key: string, title: string): LinkEntry {
  return {
    link: { id: `l-${key}`, from_id: `jira:${key}`, to_id: "teamcity:build:1187", relation: "related" },
    other: { entity_id: `jira:${key}`, kind: "ticket", title, deleted_at: null },
  } as unknown as LinkEntry;
}

/** The run's view with no test failed. */
function noFailedTests(id: string, section?: string): Promise<SourceView> {
  const view = viewOf(id === "teamcity:build:1188");
  return Promise.resolve(
    section === "tests" ? { actions: [], sections: [] } : { ...view, sections: view.sections.map((s) => (s.id === "tests" ? { ...s, count: 0 } : s)) },
  );
}

const dialogButton = (name: string) =>
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find((b) => b.textContent?.trim() === name);

test("a failed run with a failing test offers Create ticket after Open the test file", async () => {
  draw();
  await settle();
  const acts = [...target.querySelectorAll(".tf-problem .tf-acts > *")].map((el) => el.textContent?.trim());
  expect(acts).toEqual(["Show it in the log", "Open the test file", "Create ticket"]);
});

test("a green run and a failed run without a failing test draw no Create ticket", async () => {
  draw("teamcity:build:1186");
  await settle();
  expect(button("Create ticket")).toBeUndefined();
  if (page) unmount(page);
  page = null;
  draw("teamcity:build:1185", undefined, { view: noFailedTests });
  await settle();
  expect(target.querySelector("h1 .tf-chip.tf-err")).not.toBeNull();
  expect(button("Create ticket")).toBeUndefined();
});

test("Create ticket opens the dialog prefilled from the failing test, its assertion and the run's address", async () => {
  draw();
  await settle();
  button("Create ticket")!.click();
  await settle();
  const title = document.querySelector<HTMLInputElement>(".tf-dialog input[aria-label='Ticket title']")!;
  expect(title.value).toBe(FILED_TITLE);
  const body = document.querySelector<HTMLTextAreaElement>(".tf-dialog textarea[aria-label='Ticket description']")!.value;
  expect(body).toContain("1 test failed: sepa::retry::gives_up_after_max_attempts");
  expect(body).toContain("assertion `left == right` failed");
  expect(body).toContain("Payout_IntegrationTests #1187 on feature/PAY-231-sepa-retry: http://ci.example/build/1187");
  expect(body).toContain("At tests/sepa_retry.rs:14");
  const options = [...document.querySelectorAll<HTMLOptionElement>(".tf-dialog select[aria-label='Project the ticket goes in'] option")];
  expect(options.map((o) => o.textContent)).toEqual(["Choose…", "Payments", "Web"]);
  expect(dialogButton("Create ticket")!.disabled).toBe(true);
});

test("filing queues a CreateTicket in the picked project, linked to the run, and the problem block says it is queued", async () => {
  const linked: (string | undefined)[] = [];
  const drawn = draw("teamcity:build:1187", undefined, {
    submitWrite: (payload, linkTo) => {
      drawn.writes.push(payload);
      linked.push(linkTo);
      return Promise.resolve({ id: 41 } as QueuedWrite);
    },
  });
  await settle();
  button("Create ticket")!.click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>(".tf-dialog select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:WEB";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  dialogButton("Create ticket")!.click();
  await settle();
  expect(drawn.writes).toHaveLength(1);
  const create = (drawn.writes[0] as { CreateTicket: Record<string, string> }).CreateTicket;
  expect(create.entity).toBe("jira:WEB");
  expect(create.title).toBe(FILED_TITLE);
  expect(create.ticket_type).toBe("Task");
  expect(create.body).toContain("assertion `left == right` failed");
  expect(linked).toEqual(["teamcity:build:1187"]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(toasts.items.at(-1)!.text).toBe(`Queued: ticket ${FILED_TITLE}.`);
  expect(button("Create ticket")).toBeUndefined();
  expect(target.querySelector(".tf-problem .tf-acts")?.textContent).toContain("Ticket queued");
  expect(queuedMarks("teamcity:build:1187", "ticket").map((m) => m.text)).toEqual([FILED_TITLE]);
});

test("a project the source's credential cannot file in draws Create ticket disabled saying why, and queues nothing (#2052)", async () => {
  const reason = "This Jira user cannot create tickets in WEB: it lacks the Create Issues permission";
  const asked: WriteAsk[][] = [];
  const drawn = draw("teamcity:build:1187", undefined, {
    refusedWrites: (asks) => {
      asked.push(asks);
      return Promise.resolve(asks.some((ask) => ask.entity === "jira:WEB") ? [{ entity: "jira:WEB", op: "create_ticket", reason }] : []);
    },
  });
  await settle();
  button("Create ticket")!.click();
  await settle();
  expect(asked).toContainEqual([
    { entity: "jira:PAY", op: "create_ticket" },
    { entity: "jira:WEB", op: "create_ticket" },
  ]);
  const select = document.querySelector<HTMLSelectElement>(".tf-dialog select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:WEB";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  const create = dialogButton("Create ticket")!;
  expect(create.getAttribute("aria-disabled")).toBe("true");
  expect(create.title).toBe(reason);
  expect(document.querySelector(".tf-dialog")?.textContent).toContain(reason);
  create.click();
  await settle();
  expect(drawn.writes).toEqual([]);
  // Another project is the reader's to pick, and files as before.
  select.value = "jira:PAY";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(dialogButton("Create ticket")!.hasAttribute("aria-disabled")).toBe(false);
});

test("once a sync brings the ticket in, the run page draws the linked ticket in place of the queued mark", async () => {
  let links: LinkEntry[] = [];
  const drawn = draw("teamcity:build:1187", undefined, { links: () => Promise.resolve(links) });
  await settle();
  button("Create ticket")!.click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>(".tf-dialog select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:PAY";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  dialogButton("Create ticket")!.click();
  await settle();
  links = [ticketLink("PAY-342", FILED_TITLE)];
  // Jira's sync, not TeamCity's: the ticket's source brought it in.
  drawn.sync({ source_id: "jira", running: false });
  await settle();
  const ticket = [...target.querySelectorAll<HTMLAnchorElement>(".tf-problem a")].find((a) => a.textContent?.includes("PAY-342"));
  expect(ticket?.getAttribute("href")).toBe("#/job/jira%3APAY-342");
  expect(target.querySelector(".tf-problem .tf-acts")?.textContent).not.toContain("Ticket queued");
  expect(queuedMarks("teamcity:build:1187", "ticket")).toEqual([]);
  expect(button("Create ticket")).toBeUndefined();
});

test("a run a ticket is already linked to draws the ticket and no Create ticket", async () => {
  draw("teamcity:build:1187", undefined, { links: () => Promise.resolve([ticketLink("PAY-300", "Flaky SEPA retry")]) });
  await settle();
  const ticket = [...target.querySelectorAll<HTMLAnchorElement>(".tf-problem a")].find((a) => a.textContent?.includes("PAY-300"));
  expect(ticket?.getAttribute("href")).toBe("#/job/jira%3APAY-300");
  expect(ticket?.title).toBe("Flaky SEPA retry");
  expect(button("Create ticket")).toBeUndefined();
});

// -- A reload before the ticket's sync (#1953) ---------------------------------

/** A `CreateTicket` filed from the run, as `owed_tickets` names it. */
function owedWrite(id: number, state: QueuedWrite["state"]): QueuedWrite {
  return {
    id,
    state,
    payload: { CreateTicket: { entity: "jira:PAY", title: FILED_TITLE, body: "assertion failed", ticket_type: "Bug" } },
  } as QueuedWrite;
}

test("after a reload, a ticket filed from the run and still queued draws Ticket queued and no Create ticket", async () => {
  const asked: string[] = [];
  draw("teamcity:build:1187", undefined, {
    owedTickets: (runId) => {
      asked.push(runId);
      return Promise.resolve([owedWrite(41, "pending")]);
    },
  });
  await settle();
  expect(asked).toEqual(["teamcity:build:1187"]);
  expect(button("Create ticket")).toBeUndefined();
  const chip = target.querySelector<HTMLElement>(".tf-problem .tf-acts .tf-chip");
  expect(chip?.textContent).toBe("Ticket queued");
  expect(chip?.title).toBe(FILED_TITLE);
  expect(queuedMarks("teamcity:build:1187", "ticket")).toMatchObject([{ write: 41, kind: "ticket", text: FILED_TITLE, base: null }]);
});

test("after a reload, a ticket sent and not mirrored yet stays queued until the sync that brings it in draws it", async () => {
  let owed = [owedWrite(41, "sent")];
  let links: LinkEntry[] = [];
  const drawn = draw("teamcity:build:1187", undefined, {
    owedTickets: () => Promise.resolve(owed),
    links: () => Promise.resolve(links),
  });
  await settle();
  expect(button("Create ticket")).toBeUndefined();
  expect(target.querySelector(".tf-problem .tf-acts")?.textContent).toContain("Ticket queued");
  // The ticket's listen for the sync that brings it in, beside the page's own.
  const listening = drawn.listening();

  // A sync that does not bring it in: still on its way.
  drawn.sync({ source_id: "teamcity", running: false });
  await settle();
  expect(button("Create ticket")).toBeUndefined();
  expect(target.querySelector(".tf-problem .tf-acts")?.textContent).toContain("Ticket queued");

  // Jira's sync brings it in: the read links it and names it no more.
  owed = [];
  links = [ticketLink("PAY-342", FILED_TITLE)];
  drawn.sync({ source_id: "jira", running: false });
  await settle();
  const ticket = [...target.querySelectorAll<HTMLAnchorElement>(".tf-problem a")].find((a) => a.textContent?.includes("PAY-342"));
  expect(ticket?.getAttribute("href")).toBe("#/job/jira%3APAY-342");
  expect(queuedMarks("teamcity:build:1187", "ticket")).toEqual([]);
  expect(button("Create ticket")).toBeUndefined();
  expect(drawn.listening()).toBe(listening - 1);
});

test("after a reload, a ticket held on drift or refused offers Create ticket: it waits on the reader in the queue", async () => {
  draw("teamcity:build:1187", undefined, { owedTickets: () => Promise.resolve([owedWrite(41, "refused"), owedWrite(42, "held")]) });
  await settle();
  expect(button("Create ticket")).toBeDefined();
  expect(target.querySelector(".tf-problem .tf-acts")?.textContent).not.toContain("Ticket queued");
});

test("after a reload, a ticket held as unconfirmed draws Ticket queued and is followed: it may be in Jira already (#1966)", async () => {
  const hearing: ((row: QueuedWrite | undefined) => void)[] = [];
  const unconfirmed = {
    ...owedWrite(41, "held"),
    held_snapshot: { op: "create_ticket", unconfirmed: { source: "Jira", detail: "the connection closed" } },
  } as QueuedWrite;
  draw("teamcity:build:1187", undefined, {
    owedTickets: () => Promise.resolve([unconfirmed]),
    follower: () => ({
      follow: (id, hear) => {
        if (id === 41) hearing.push(hear);
        return () => {};
      },
    }),
  });
  await settle();
  expect(button("Create ticket")).toBeUndefined();
  expect(target.querySelector(".tf-problem .tf-acts .tf-chip")?.textContent).toBe("Ticket queued");
  expect(hearing).toHaveLength(1);
  // Still held as unconfirmed on the next read: still on its way.
  hearing[0]!(unconfirmed);
  await settle();
  expect(button("Create ticket")).toBeUndefined();
  // The reader discards it in the queue: the create is offered again.
  noteWithdrawn(41);
  await settle();
  expect(button("Create ticket")).toBeDefined();
});

test("after a reload, a queued ticket is followed: refused, it gives Create ticket back and Edit and resend reopens what was filed", async () => {
  const hearing: ((row: QueuedWrite | undefined) => void)[] = [];
  draw("teamcity:build:1187", undefined, {
    owedTickets: () => Promise.resolve([owedWrite(41, "pending")]),
    follower: () => ({
      follow: (id, hear) => {
        if (id === 41) hearing.push(hear);
        return () => {};
      },
    }),
  });
  await settle();
  expect(hearing).toHaveLength(1);
  hearing[0]!({ ...owedWrite(41, "refused"), detail: "Summary is required" });
  await settle();
  expect(button("Create ticket")).toBeDefined();
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Ticket was refused: Summary is required");
  toast.more!.find((a) => a.label === "Edit and resend")!.run();
  await settle();
  const title = document.querySelector<HTMLInputElement>(".tf-dialog input[aria-label='Ticket title']")!;
  expect(title.value).toBe(FILED_TITLE);
});

test("after a reload, a ticket the owed read links as it answers is drawn, and Create ticket is not offered", async () => {
  // Mirrored, not linked yet: the end of the sync has not resolved it, and the owed read does.
  let links: LinkEntry[] = [];
  draw("teamcity:build:1187", undefined, {
    // The link is drawn as the read answers, not as it is asked.
    owedTickets: () =>
      Promise.resolve().then(() => {
        links = [ticketLink("PAY-342", FILED_TITLE)];
        return [];
      }),
    links: () => Promise.resolve(links),
  });
  await settle();
  const ticket = [...target.querySelectorAll<HTMLAnchorElement>(".tf-problem a")].find((a) => a.textContent?.includes("PAY-342"));
  expect(ticket?.getAttribute("href")).toBe("#/job/jira%3APAY-342");
  expect(button("Create ticket")).toBeUndefined();
});

test("an owed read that answers before the create's own answer marks it once, and a refusal toasts once", async () => {
  let answer: (write: QueuedWrite) => void = () => {};
  let owed: QueuedWrite[] = [];
  const hearing: ((row: QueuedWrite | undefined) => void)[] = [];
  const drawn = draw("teamcity:build:1187", undefined, {
    submitWrite: () => new Promise((resolve) => (answer = resolve)),
    owedTickets: () => Promise.resolve(owed),
    follower: () => ({
      follow: (id, hear) => {
        if (id === 41) hearing.push(hear);
        return () => {};
      },
    }),
  });
  await settle();
  button("Create ticket")!.click();
  await settle();
  const select = document.querySelector<HTMLSelectElement>(".tf-dialog select[aria-label='Project the ticket goes in']")!;
  select.value = "jira:PAY";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  dialogButton("Create ticket")!.click();
  await settle();
  // The row is in the queue, and the run's sync ends before `submit_write`'s answer is back.
  owed = [owedWrite(41, "pending")];
  drawn.sync(synced("teamcity"));
  await settle();
  answer({ id: 41, state: "pending" } as QueuedWrite);
  await settle();
  expect(queuedMarks("teamcity:build:1187", "ticket").map((mark) => mark.write)).toEqual([41]);
  expect(hearing, "one follow of the create").toHaveLength(1);
  const before = toasts.items.length;
  for (const hear of hearing) hear({ ...owedWrite(41, "refused"), detail: "Summary is required" });
  await settle();
  expect(toasts.items.slice(before).map((toast) => toast.text)).toEqual(["Ticket was refused: Summary is required"]);
});
