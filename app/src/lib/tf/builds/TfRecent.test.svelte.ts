/**
 * Builds' *Recent* (2026-11-25): the newest runs of every pipeline with no folds, each row naming its pipeline and
 * folders; the filter's scope, results and source asked of the backend and its words narrowing what is held; new runs
 * held behind *Show N new runs* under the pointer; older pages; the way back from a run; the minute's ask of each source.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The typed words rank through the app's ranking (#2535): here a letters-in-order stand-in.
vi.mock("../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));

import type { RecentCursor, RecentFilter, RecentPipeline, RecentRun, RecentRuns, RunRow } from "../../ipc/entity";
import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
import { forgetFilters } from "../filter/filter-state.svelte";
import { runsRead } from "./runs-read.svelte";
import TfRecent, { forgetRecent, type RecentPorts } from "./TfRecent.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

/** Frames asked for, run by `settle` (or by a test, to say when the page paints): jsdom's own come on a 16 ms timer. */
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
  forgetRecent();
  forgetFilters();
  runsRead.forget();
  vi.unstubAllGlobals();
});

/** The page's tasks run and its DOM flushed, the frames it asked for left unpainted. */
async function turns() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

/** The page's tasks run, its frames painted, and what they started run too. */
async function settle() {
  await turns();
  frames.splice(0).forEach((run) => run(performance.now()));
  await turns();
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
  /** The scope Builds hands it (#2570): its name and its pipelines, `[]` for every one, `null` while still read. */
  scope: (title: string, pipelines: readonly string[] | null) => void;
}

function draw(
  overrides: Partial<RecentPorts> = {},
  props: { job?: string | null; first?: RecentRuns; title?: string; pipelines?: readonly string[] | null; ondrawn?: () => void } = {},
): Drawn {
  const drawn: Drawn = { asked: [], next: props.first ?? PAGE, started: [], revision: () => {}, scope: () => {} };
  let revision = $state(0);
  let title = $state(props.title ?? "All pipelines");
  let pipelines = $state.raw<readonly string[] | null>(props.pipelines === undefined ? [] : props.pipelines);
  drawn.revision = (n) => (revision = n);
  drawn.scope = (name, ids) => {
    title = name;
    pipelines = ids;
  };
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
      get syncRevision() {
        return revision;
      },
      get title() {
        return title;
      },
      get pipelines() {
        return pipelines;
      },
      ...(props.ondrawn ? { ondrawn: props.ondrawn } : {}),
    },
  });
  return drawn;
}

const rows = () => [...target.querySelectorAll<HTMLElement>("tr[data-run]")];
const numbers = () => rows().map((row) => row.querySelector(".tf-run-pno")?.textContent);

test("the newest runs of every pipeline, newest first, each naming its pipeline, number and folders", async () => {
  const drawn = draw();
  await settle();
  expect(drawn.asked).toEqual([{ filter: { sources: [], results: [], mine: false, branches: null, pipelines: [] }, before: null, limit: 60 }]);
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  const first = rows()[0]!;
  expect(first.querySelector(".tf-run-pname")?.textContent).toBe("Deploy");
  expect(first.querySelector(".tf-run-place")?.textContent).toBe("Ops");
  expect(rows()[1]!.querySelector(".tf-run-place")?.textContent).toBe("Payments › Payout");
  expect(first.querySelector(".tf-run-pno")?.getAttribute("href")).toBe("#/build/tc%3AbuildType%3ADeploy/tc%3Abuild%3A12");
  // No folds: no fold's summary, no tree.
  expect(target.querySelector("details, .tf-tfold")).toBeNull();
  // The header names the scope and holds no count (#2570); no switch heads the bar.
  expect(target.querySelector("h1")?.textContent).toBe("All pipelines");
  expect(target.querySelector("[aria-label='Builds view']")).toBeNull();
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

test("typed words have the app rank the runs by their pipeline's name and place, newest first, its letters marked (#2535)", async () => {
  draw();
  await settle();
  const input = target.querySelector<HTMLInputElement>("input[type=search], input")!;
  // No substring holds "pylnt"; its letters stand in order in "Payout_Lint".
  input.value = "pylnt";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(numbers()).toEqual(["#11", "#10"]);
  const names = [...target.querySelectorAll(".tf-run-pname")];
  for (const name of names) expect([...name.querySelectorAll("mark")].map((m) => m.textContent).join("").toLowerCase()).toBe("pylnt");
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

test("the first read asks for the rows that fill the window and draws them; the rest of the page follows below them, the drawn rows kept in place (#2569)", async () => {
  const cursor = { made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" };
  let answer: (page: RecentRuns) => void = () => {};
  const asked: { before: RecentCursor | null; limit: number }[] = [];
  draw(
    {
      recent: (_filter, before, limit) => {
        asked.push({ before, limit });
        return asked.length === 1 ? Promise.resolve({ ...PAGE, next: cursor }) : new Promise((resolve) => (answer = resolve));
      },
    },
  );
  await settle();
  expect(asked[0]).toEqual({ before: null, limit: 60 });
  const first = rows();
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  // The rest of the 200, from the first answer's cursor.
  expect(asked[1]).toEqual({ before: cursor, limit: 140 });
  answer({ runs: [at(LINT, run(9, "ok", today(0, 10))), at(DEPLOY, run(8, "ok", today(0, 5)))], pipelines: [LINT, DEPLOY], next: null });
  await settle();
  expect(numbers()).toEqual(["#12", "#11", "#10", "#9", "#8"]);
  // The rows drawn first are the same rows, in the same places.
  expect(rows().slice(0, 3)).toEqual(first);
  first.forEach((row, i) => expect(rows()[i]).toBe(row));
  expect(asked).toHaveLength(2);
});

test("the rest of the first page is asked once the first rows have had their frame, so they paint before it is laid (#2569)", async () => {
  {
    const cursor = { made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" };
    const asked: number[] = [];
    draw({
      recent: (_filter, _before, limit) => {
        asked.push(limit);
        return Promise.resolve({ ...PAGE, next: asked.length === 1 ? cursor : null });
      },
    });
    await turns();
    expect(numbers()).toEqual(["#12", "#11", "#10"]);
    // Drawn, not yet painted: the rest waits for the frame.
    expect(asked).toEqual([60]);
    await settle();
    expect(asked).toEqual([60, 140]);
  }
});

test("a head read again while the rest of the first page is out leaves older pages readable, not stuck on Reading (#2569)", async () => {
  const cursor = { made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" };
  let answer: (page: RecentRuns) => void = () => {};
  const asked: { before: RecentCursor | null; limit: number }[] = [];
  const drawn = draw({
    recent: (_filter, before, limit) => {
      asked.push({ before, limit });
      return asked.length === 2 ? new Promise((resolve) => (answer = resolve)) : Promise.resolve({ ...PAGE, next: cursor });
    },
  });
  await settle();
  expect(asked.map((one) => one.limit)).toEqual([60, 140]);
  // A sync ends while the rest is out: the head is read again, and the rest's answer belongs to the read before it.
  drawn.revision(1);
  await settle();
  expect(asked.at(-1)).toEqual({ before: null, limit: 200 });
  answer({ runs: [at(LINT, run(9, "ok", today(0, 10)))], pipelines: [LINT], next: null });
  await settle();
  expect(target.querySelector(".tf-older")?.textContent).not.toContain("Reading older runs");
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Older runs")!.click();
  await settle();
  expect(asked.at(-1)).toEqual({ before: cursor, limit: 200 });
});

test("an older page follows from the last page's cursor", async () => {
  const rest = { made: "2026-08-22 10:20:00+00", entity_id: "tc:build:10" };
  const drawn = draw({}, { first: { ...PAGE, next: rest } });
  await settle();
  // The first read's rest came back as the same page: nothing new, and its cursor is where *Older runs* reads from.
  expect(drawn.asked.map((one) => one.limit)).toEqual([60, 140]);
  drawn.next = { runs: [at(LINT, run(9, "ok", today(0, 10)))], pipelines: [LINT], next: null };
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Older runs")!.click();
  await settle();
  expect(drawn.asked.at(-1)!.before).toEqual(rest);
  expect(drawn.asked.at(-1)!.limit).toBe(200);
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

test("a CI source's run that changed something reads the head again; one that changed nothing, or another source's, does not", async () => {
  const heard: Record<string, (event: { payload: unknown }) => void> = {};
  const drawn = draw({
    listen: (event, handler) => {
      heard[event] = handler;
      return Promise.resolve(() => {});
    },
  });
  await settle();
  const reads = drawn.asked.length;
  const ended = (source_id: string, last_changed: boolean) =>
    heard["sync:state"]!({ payload: { source_id, running: false, last_changed, run_id: 2, started_at: null, last_finished_at: null, last_outcome: null, next_run_at: null, backoff_until: null } });
  ended("tc", false);
  ended("jira", true);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(drawn.asked.length).toBe(reads);
  drawn.next = { ...PAGE, runs: [at(LINT, run(13, "ok", today(0, 50))), ...PAGE.runs] };
  ended("tc", true);
  await new Promise((resolve) => setTimeout(resolve, 20));
  await settle();
  expect(drawn.asked.length).toBe(reads + 1);
  expect(numbers()[0]).toBe("#13");
});

test("Mark all as read with nothing narrowing asks for every run, so a press before the pipeline list answers marks every failure (#2555)", async () => {
  const scopes: unknown[] = [];
  runsRead.ports = {
    // The counts and the instants as the bridge answers them (#2568): not what this case reads.
    ...runsRead.ports,
    read: async () => ({ read: [], while_running: [] }),
    mark: async (scope) => (scopes.push(scope), { read: ["tc:build:11", "tc:build:3"], while_running: [] }),
    unmark: async (runs) => runs,
    open: async () => false,
  };
  await runsRead.load();
  draw();
  await settle();
  expect(rows()[1]!.querySelector(".tf-unread")).not.toBeNull();
  target.querySelector<HTMLButtonElement>("button[aria-label='Mark all as read']")!.click();
  flushSync();
  expect(rows()[1]!.querySelector(".tf-unread"), "the failure drawn read at once").toBeNull();
  await settle();
  expect(scopes).toEqual([{ everything: true, pipelines: [], runs: ["tc:build:11"] }]);
  expect(runsRead.read?.has("tc:build:3"), "and what the backend wrote past the page").toBe(true);
});

test("a folder's scope waits for its pipelines, asks for them alone, and its header names the folder (#2570)", async () => {
  const drawn = draw({}, { title: "Payout", pipelines: null });
  await settle();
  expect(drawn.asked, "nothing read while the folder's pipelines are read").toEqual([]);
  expect(target.querySelector("h1")?.textContent).toBe("Payout");
  expect(target.querySelector(".tf-runs .tf-dim")?.textContent).toBe("Reading runs…");
  drawn.next = { ...PAGE, runs: PAGE.runs.filter((one) => one.pipeline_id === LINT.entity_id), pipelines: [LINT] };
  drawn.scope("Payout", [LINT.entity_id]);
  await settle();
  expect(drawn.asked[0]).toEqual({ filter: { sources: [], results: [], mine: false, branches: null, pipelines: [LINT.entity_id] }, before: null, limit: 60 });
  expect(numbers()).toEqual(["#11", "#10"]);
});

test("another scope draws its own runs from the top, never laid over the last scope's (#2570)", async () => {
  const drawn = draw();
  await settle();
  expect(numbers()).toEqual(["#12", "#11", "#10"]);
  drawn.next = { ...PAGE, runs: [at(DEPLOY, run(12, "ok", today(0, 40)))], pipelines: [DEPLOY] };
  drawn.scope("Ops", [DEPLOY.entity_id]);
  await settle();
  expect(drawn.asked.at(-1)?.filter.pipelines).toEqual([DEPLOY.entity_id]);
  expect(drawn.asked.at(-1)?.limit, "a scope with nothing held reads its first rows first").toBe(60);
  expect(numbers()).toEqual(["#12"]);
  expect(target.querySelector("h1")?.textContent).toBe("Ops");
});

test("Mark all as read at a folder with nothing narrowing marks its pipelines (#2570)", async () => {
  const scopes: unknown[] = [];
  runsRead.ports = {
    ...runsRead.ports,
    read: async () => ({ read: [], while_running: [] }),
    mark: async (scope) => (scopes.push(scope), { read: ["tc:build:11"], while_running: [] }),
    unmark: async (runs) => runs,
    open: async () => false,
  };
  await runsRead.load();
  draw({}, { title: "Payout", pipelines: [LINT.entity_id] });
  await settle();
  const button = target.querySelector<HTMLButtonElement>("button[aria-label='Mark all as read']")!;
  expect(button.title).toBe("Mark all as read: every failed run of the pipelines in Payout");
  button.click();
  await settle();
  expect(scopes).toEqual([{ everything: false, pipelines: [LINT.entity_id], runs: ["tc:build:11"] }]);
});

test("a result chip draws the runs its read answers, never the rows the last filter held (the rows held under a new filter)", async () => {
  const drawn = draw();
  await settle();
  drawn.next = { ...PAGE, runs: [at(LINT, run(11, "err", today(0, 30)))], pipelines: [LINT] };
  const failed = [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Failed")!;
  failed.click();
  await settle();
  expect(numbers()).toEqual(["#11"]);
});

test("Builds hears the first rows drew once they have painted, so the pipeline list it then reads lands in a later frame (#2570)", async () => {
  let drawnAt = 0;
  draw({}, { ondrawn: () => (drawnAt += 1) });
  await turns();
  expect(numbers(), "the first rows laid").toEqual(["#12", "#11", "#10"]);
  expect(drawnAt, "not before their frame").toBe(0);
  await settle();
  expect(drawnAt).toBe(1);
});
