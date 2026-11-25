/**
 * Builds' *Recent* (2026-11-25): the newest runs of every pipeline with no folds, each row naming its pipeline and
 * folders; the filter's scope, results and source asked of the backend and its words narrowing what is held; new runs
 * held behind *Show N new runs* under the pointer; older pages; the way back from a run; the minute's ask of each source.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RecentCursor, RecentFilter, RecentPipeline, RecentRun, RecentRuns, RunRow } from "../../ipc/entity";
import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
import { forgetFilters } from "../filter/filter-state.svelte";
import { runsRead } from "./runs-read.svelte";
import TfRecent, { forgetRecent, type RecentPorts } from "./TfRecent.svelte";

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
  forgetRecent();
  forgetFilters();
  runsRead.forget();
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const today = (h: number, m = 0) => {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, m).toISOString();
};

function run(no: number, tone: "ok" | "err", started: string | null, extra: Partial<RunRow> = {}): RunRow {
  return {
    entity_id: `tc:build:${no}`,
    number: String(no),
    state: { text: tone === "err" ? "FAILURE" : "SUCCESS", tone },
    branch: "main",
    started,
    finished: started,
    duration_seconds: 60,
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

const LINT: RecentPipeline = { entity_id: "tc:buildType:Lint", source_id: "tc", source_name: "TeamCity", name: "Payout_Lint", group: "Payout", path: ["Payments", "Payout"] };
const DEPLOY: RecentPipeline = { entity_id: "tc:buildType:Deploy", source_id: "tc", source_name: "TeamCity", name: "Deploy", group: null, path: ["Ops"] };

const at = (pipeline: RecentPipeline, row: RunRow): RecentRun => ({ pipeline_id: pipeline.entity_id, run: row });

const PAGE: RecentRuns = {
  runs: [
    at(DEPLOY, run(12, "ok", today(0, 40), { branch: "release/2026.11" })),
    at(LINT, run(11, "err", today(0, 30), { branch: "feature/PAY-231" })),
    at(LINT, run(10, "ok", today(0, 20))),
  ],
  pipelines: [LINT, DEPLOY],
  next: null,
};

interface Drawn {
  asked: { filter: RecentFilter; before: RecentCursor | null; limit: number }[];
  next: RecentRuns | Error;
  started: string[];
  revision: (n: number) => void;
}

function draw(overrides: Partial<RecentPorts> = {}, props: { job?: string | null; first?: RecentRuns } = {}): Drawn {
  const drawn: Drawn = { asked: [], next: props.first ?? PAGE, started: [], revision: () => {} };
  let revision = $state(0);
  drawn.revision = (n) => (revision = n);
  const ports: RecentPorts = {
    recent: (filter, before, limit) => {
      drawn.asked.push({ filter, before, limit });
      return drawn.next instanceof Error ? Promise.reject(drawn.next) : Promise.resolve(drawn.next);
    },
    follow: () => Promise.resolve(null),
    listen: () => Promise.resolve(() => {}),
    jobBranches: () => Promise.resolve(["feature/PAY-231"]),
    sources: () => Promise.resolve([]),
    status: () => Promise.resolve([]),
    syncNow: (id) => {
      drawn.started.push(id);
      return Promise.resolve(1);
    },
    focused: () => true,
    ...overrides,
  };
  page = mount(TfRecent, {
    target,
    props: {
      ready: true,
      job: props.job ?? null,
      ports,
      crumbs: createRawSnippet(() => ({ render: () => "<nav>Builds › Recent</nav>" })),
      switcher: createRawSnippet(() => ({ render: () => '<div class="tf-seg">Pipelines Recent</div>' })),
      get syncRevision() {
        return revision;
      },
    },
  });
  return drawn;
}

const rows = () => [...target.querySelectorAll<HTMLElement>("tr[data-run]")];
const numbers = () => rows().map((row) => row.querySelector(".tf-run-pno")?.textContent);

test("the newest runs of every pipeline, newest first, each naming its pipeline, number and folders", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.asked).toEqual([{ filter: { sources: [], results: [], mine: false, branches: null }, before: null, limit: 200 }]);
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  const first = rows()[0]!;
  expect(first.querySelector(".tf-run-pname")?.textContent).toBe("Deploy");
  expect(first.querySelector(".tf-run-place")?.textContent).toBe("Ops");
  expect(rows()[1]!.querySelector(".tf-run-place")?.textContent).toBe("Payments › Payout");
  expect(first.querySelector(".tf-run-pno")?.getAttribute("href")).toBe("#/build/tc%3AbuildType%3ADeploy/tc%3Abuild%3A12");
  // No folds: no fold's summary, no tree.
  expect(target.querySelector("details, .tf-tfold")).toBeNull();
  expect(target.querySelector("h1")?.textContent).toBe("Recent runs 3");
  // The unread failure keeps its dot.
  expect(rows()[1]!.querySelector(".tf-unread")).not.toBeNull();
  expect(rows()[0]!.querySelector(".tf-unread")).toBeNull();
});

test("a pipeline's place names its source first where two sources list runs", async () => {
  const other: RecentPipeline = { ...DEPLOY, entity_id: "gh:wf:Deploy", source_id: "gh", source_name: "Forgejo" };
  draw({}, { first: { runs: [at(other, run(13, "ok", today(0, 50))), ...PAGE.runs], pipelines: [LINT, DEPLOY, other], next: null } });
  await settle();
  expect(rows()[0]!.querySelector(".tf-run-place")?.textContent).toBe("Forgejo › Ops");
  expect(rows()[1]!.querySelector(".tf-run-place")?.textContent).toBe("TeamCity › Ops");
  // *Source* is offered with two sources.
  expect(target.querySelector('select[aria-label="Source"], .tf-fsel')).not.toBeNull();
});

test("a result chip asks the backend; typed words narrow what is held without a read", async () => {
  const drawn = draw();
  await settle();
  const failed = [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Failed")!;
  failed.click();
  await settle();
  expect(drawn.asked.at(-1)!.filter.results).toEqual(["Failed"]);
  failed.click();
  await settle();
  const reads = drawn.asked.length;
  const input = target.querySelector<HTMLInputElement>("input[type=search], input")!;
  input.value = "deploy";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(drawn.asked.length).toBe(reads);
  expect(numbers()).toEqual(["#12"]);
});

test("Mine and This job are asked of the backend", async () => {
  const drawn = draw({}, { job: "PAY-231" });
  await settle();
  const press = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.startsWith(label))!.click();
  press("Mine");
  await settle();
  expect(drawn.asked.at(-1)!.filter).toMatchObject({ mine: true, branches: null });
  press("This job");
  await settle();
  expect(drawn.asked.at(-1)!.filter).toMatchObject({ mine: false, branches: ["feature/PAY-231"] });
});

test("a run a sync brings waits behind Show 1 new run while the pointer rests on the list, then takes the top", async () => {
  const drawn = draw();
  await settle();
  const list = target.querySelector<HTMLElement>(".tf-recent")!;
  list.dispatchEvent(new PointerEvent("pointerenter"));
  drawn.next = { ...PAGE, runs: [at(LINT, run(13, "ok", today(0, 50))), ...PAGE.runs] };
  drawn.revision(1);
  await settle();
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  expect(target.querySelector(".tf-newruns button")?.textContent).toBe("Show 1 new run");
  list.dispatchEvent(new PointerEvent("pointerleave"));
  await settle();
  expect(numbers()).toEqual(["#13", "#12", "#11", "#10"]);
  expect(target.querySelector(".tf-newruns")).toBeNull();
  expect(rows()[0]!.classList.contains("tf-run-new")).toBe(true);
});

test("an older page follows from the last page's cursor", async () => {
  const drawn = draw({}, { first: { ...PAGE, next: { made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" } } });
  await settle();
  drawn.next = { runs: [at(LINT, run(9, "ok", today(0, 10)))], pipelines: [LINT], next: null };
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Older runs")!.click();
  await settle();
  expect(drawn.asked.at(-1)!.before).toEqual({ made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" });
  expect(numbers()).toEqual(["#12", "#11", "#10", "#9"]);
  expect(target.querySelector(".tf-older")?.textContent).toContain("Every run knobas holds.");
});

test("back from a run, it draws what it held at once and focuses the run that was opened", async () => {
  draw();
  await settle();
  rows()[1]!.querySelector<HTMLElement>(".tf-run-pname")!.click();
  expect(location.hash).toBe("#/build/tc%3AbuildType%3ALint/tc%3Abuild%3A11");
  unmount(page!);
  page = null;
  draw({ recent: () => new Promise<never>(() => {}) });
  flushSync();
  // Drawn before any answer.
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  await settle();
  expect(document.activeElement).toBe(rows()[1]!.querySelector(".tf-run-pno"));
});

test("Up and Down step the runs by their numbers", async () => {
  draw();
  await settle();
  const first = rows()[0]!.querySelector<HTMLElement>(".tf-run-pno")!;
  first.focus();
  first.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  expect(document.activeElement).toBe(rows()[1]!.querySelector(".tf-run-pno"));
  (document.activeElement as HTMLElement).dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  expect(document.activeElement).toBe(first);
});

test("each CI source is asked for its run once its last ended a minute ago; never one by hand, running or off", async () => {
  const ago = (s: number) => new Date(Date.now() - s * 1000).toISOString();
  const source = (id: string, extra: Partial<SourceSummary> = {}) => ({ id, enabled: true, sync_by_hand: false, ...extra }) as SourceSummary;
  const state = (id: string, extra: Partial<SourceSyncStatus> = {}): SourceSyncStatus => ({
    source_id: id,
    running: false,
    run_id: null,
    started_at: null,
    last_finished_at: ago(120),
    last_outcome: null,
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
    ...extra,
  });
  const pipes = ["tc", "fresh", "hand", "busy", "off", "elsewhere"].map((id) => ({ ...LINT, entity_id: `${id}:p`, source_id: id }));
  const drawn = draw(
    {
      sources: () => Promise.resolve([source("tc"), source("fresh"), source("hand", { sync_by_hand: true }), source("busy"), source("off", { enabled: false }), source("docs")]),
      status: () => Promise.resolve([state("tc"), state("fresh", { last_finished_at: ago(20) }), state("hand"), state("busy", { running: true }), state("off"), state("docs")]),
    },
    { first: { runs: pipes.map((p, n) => at(p, run(n, "ok", today(0, n)))), pipelines: pipes, next: null } },
  );
  await settle();
  expect(drawn.started).toEqual(["tc"]);
});

test("nothing is asked while the window is in the background", async () => {
  const drawn = draw({
    sources: () => Promise.resolve([{ id: "tc", enabled: true, sync_by_hand: false } as SourceSummary]),
    status: () => Promise.resolve([{ source_id: "tc", running: false, last_finished_at: null, backoff_until: null } as SourceSyncStatus]),
    focused: () => false,
  });
  await settle();
  expect(drawn.started).toEqual([]);
});

test("a failed read says so", async () => {
  draw({}, { first: new Error("the database is not up") as unknown as RecentRuns });
  await settle();
  expect(target.textContent).toContain("Runs could not be read");
});
