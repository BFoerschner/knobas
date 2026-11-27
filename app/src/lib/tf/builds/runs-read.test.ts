/** The marks a person's runs carry (`runs-read.svelte.ts`, ux18, ADR-0063): one store for the window, every mark drawn at once. */
import { beforeEach, expect, test, vi } from "vitest";

import type { BubbleCounts, MarkScope, RunCounts, RunMarks, RunRow, SeenMove } from "../../ipc/entity";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import { noCounts, RunsRead, type RunsReadPorts } from "./runs-read.svelte";

/** `run_counts`' answer over `by`, each pipeline's counts. */
function countsOf(by: Record<string, Partial<BubbleCounts>>, extra: Partial<RunCounts> = {}): RunCounts {
  const pipelines = Object.entries(by).map(([pipeline, counts]) => ({ pipeline, counts: { ...noCounts(), ...counts } }));
  const total = noCounts();
  for (const { counts } of pipelines) for (const key of Object.keys(total) as (keyof BubbleCounts)[]) total[key] += counts[key];
  return { pipelines, total, visiting: false, fresh: [], ...extra };
}

function failed(id: string, extra: Partial<RunRow> = {}): Pick<RunRow, "entity_id" | "finished" | "state"> {
  return { entity_id: id, finished: "2026-08-22T10:04:00Z", state: { text: "FAILURE", tone: "err" }, ...extra };
}

function running(id: string): Pick<RunRow, "entity_id" | "started" | "finished"> {
  return { entity_id: id, started: "2026-08-22T10:00:00Z", finished: null };
}

/** Marks that read their runs, none taken while running. */
const reading = (read: string[]): RunMarks => ({ read, while_running: [] });

/** Runs by id, as `markRead` asks. */
const byId = (runs: string[]): MarkScope => ({ everything: false, pipelines: [], runs });

function store(ports: Partial<RunsReadPorts> = {}): RunsRead {
  const runs = new RunsRead();
  runs.settleMs = 0;
  runs.ports = {
    read: vi.fn(async () => reading([])),
    mark: vi.fn(async (scope: MarkScope) => reading(scope.runs)),
    unmark: vi.fn(async (ids: string[]) => ids),
    open: vi.fn(async () => true),
    counts: vi.fn(async () => countsOf({})),
    moveSeen: vi.fn(async () => null),
    putBackSeen: vi.fn(async () => true),
    inFront: vi.fn(async () => true),
    ...ports,
  };
  return runs;
}

beforeEach(() => resetToasts());

test("before the marks are read every failure is unread; read once, a marked one is not, and a second load reads nothing", async () => {
  const read = vi.fn(async () => reading(["teamcity:build:1"]));
  const runs = store({ read });
  expect(runs.read).toBeNull();
  expect(runs.unread(failed("teamcity:build:1"))).toBe(true);
  await runs.load();
  await runs.load();
  expect(read).toHaveBeenCalledTimes(1);
  expect(runs.unread(failed("teamcity:build:1"))).toBe(false);
  expect(runs.unread(failed("teamcity:build:2"))).toBe(true);
  expect(runs.unread(failed("teamcity:build:3", { state: { text: "SUCCESS", tone: "ok" } }))).toBe(false);
});

test("a mark taken while a run ran takes its blue and not its red; queued is not blue", async () => {
  const runs = store({ read: vi.fn(async () => ({ read: [], while_running: ["teamcity:build:2"] })) });
  await runs.load();
  expect(runs.inBlue(running("teamcity:build:1"))).toBe(true);
  expect(runs.inBlue(running("teamcity:build:2")), "marked while it ran").toBe(false);
  expect(runs.unread(failed("teamcity:build:2")), "it failed since: it asks").toBe(true);
  expect(runs.inBlue({ entity_id: "teamcity:build:3", started: null, finished: null }), "queued").toBe(false);
});

test("one mark is drawn before its write answers, and the toggle takes it back", async () => {
  let answer: (marks: RunMarks) => void = () => {};
  const mark = vi.fn(() => new Promise<RunMarks>((resolve) => (answer = resolve)));
  const unmark = vi.fn(async (ids: string[]) => ids);
  const runs = store({ mark, unmark });
  const run = failed("teamcity:build:5");
  const pending = runs.toggle(run);
  expect(runs.unread(run), "drawn at once").toBe(false);
  answer(reading(["teamcity:build:5"]));
  await pending;
  expect(mark).toHaveBeenCalledWith(byId(["teamcity:build:5"]));
  await runs.toggle(run);
  expect(runs.unread(run)).toBe(true);
  expect(unmark).toHaveBeenCalledWith(["teamcity:build:5"]);
  expect(toasts.items, "one run's mark says nothing").toHaveLength(0);
});

test("a mark the backend wrote while the run still ran is held as one: the run is out of the blue, not read", async () => {
  const runs = store({ mark: vi.fn(async (scope: MarkScope) => ({ read: [], while_running: scope.runs })) });
  await runs.load();
  await runs.markRead(["teamcity:build:6"]);
  expect(runs.read?.has("teamcity:build:6")).toBe(false);
  expect(runs.whileRunning.has("teamcity:build:6")).toBe(true);
  await runs.markUnread(["teamcity:build:6"]);
  expect(runs.whileRunning.has("teamcity:build:6"), "Mark as unread takes it back too").toBe(false);
  expect(runs.ports.unmark).toHaveBeenCalledWith(["teamcity:build:6"]);
});

test("a mark the backend did not write over keeps the mark taken while the run ran", async () => {
  // Marked while it ran and still running: the backend writes nothing over it (`MARK_READ`'s `where not reads!`).
  const runs = store({ read: vi.fn(async () => ({ read: [], while_running: ["teamcity:build:9"] })), mark: vi.fn(async () => reading([])) });
  await runs.load();
  await runs.markRead(["teamcity:build:9"]);
  expect(runs.read?.has("teamcity:build:9"), "not read: it fails unread").toBe(false);
  expect(runs.whileRunning.has("teamcity:build:9")).toBe(true);
  await runs.markAll({ runs: ["teamcity:build:9"], pipelines: [] });
  expect(runs.read?.has("teamcity:build:9")).toBe(false);
  expect(runs.whileRunning.has("teamcity:build:9")).toBe(true);
});

test("a run that cannot be unread takes no mark", async () => {
  const runs = store();
  await runs.toggle(failed("teamcity:build:6", { finished: null }));
  await runs.toggle(failed("teamcity:build:7", { state: { text: "SUCCESS", tone: "ok" } }));
  expect(runs.ports.mark).not.toHaveBeenCalled();
  expect(runs.ports.unmark).not.toHaveBeenCalled();
});

test("a mark whose write fails is put back and said", async () => {
  const runs = store({ mark: vi.fn(async () => Promise.reject({ code: "internal", message: "down", source_id: null })) });
  await runs.markRead(["teamcity:build:8"]);
  expect(runs.unread(failed("teamcity:build:8"))).toBe(true);
  expect(toasts.items.map((t) => t.text)).toEqual(["Could not mark 1 run as read: down"]);
});

test("opening a run reads it at once, writes once a session, and tells the shell when it wrote", async () => {
  let answer: (wrote: boolean) => void = () => {};
  const open = vi.fn(() => new Promise<boolean>((resolve) => (answer = resolve)));
  const runs = store({ open, read: vi.fn(async () => ({ read: [], while_running: ["teamcity:build:20"] })) });
  const afterOpen = vi.fn();
  runs.afterOpen = afterOpen;
  await runs.load();
  const pending = runs.opened("teamcity:build:20");
  expect(runs.unread(failed("teamcity:build:20")), "drawn read before the backend answers").toBe(false);
  expect(runs.whileRunning.has("teamcity:build:20"), "an open reads it, whatever a mark while it ran said").toBe(false);
  answer(true);
  await pending;
  expect(afterOpen).toHaveBeenCalledTimes(1);
  await runs.opened("teamcity:build:20");
  expect(open, "shown again, nothing written").toHaveBeenCalledTimes(1);
  expect(toasts.items, "an open says nothing").toHaveLength(0);
});

test("an open of a run already read writes nothing, and one whose write fails is put back in silence", async () => {
  const open = vi.fn(async () => Promise.reject({ code: "internal", message: "down", source_id: null }));
  const runs = store({ open, read: vi.fn(async () => ({ read: ["teamcity:build:21"], while_running: ["teamcity:build:22"] })) });
  const afterOpen = vi.fn();
  runs.afterOpen = afterOpen;
  await runs.load();
  await runs.opened("teamcity:build:21");
  expect(open).not.toHaveBeenCalled();
  await runs.opened("teamcity:build:22");
  expect(runs.unread(failed("teamcity:build:22")), "put back").toBe(true);
  expect(runs.whileRunning.has("teamcity:build:22"), "with the mark it had").toBe(true);
  expect(afterOpen).not.toHaveBeenCalled();
  expect(toasts.items).toHaveLength(0);
});

test("Mark all as read draws the drawn runs at once, adds what the backend wrote, and Undo takes back exactly that", async () => {
  const mark = vi.fn(async () => ({ read: ["teamcity:build:1", "teamcity:build:40", "teamcity:build:41"], while_running: ["teamcity:build:50"] }));
  const unmark = vi.fn(async (ids: string[]) => ids);
  const runs = store({ read: vi.fn(async () => reading(["teamcity:build:9"])), mark, unmark });
  await runs.load();
  const wrote = await runs.markAll({ runs: ["teamcity:build:1", "teamcity:build:9"], pipelines: ["teamcity:buildType:A"] });
  expect(mark).toHaveBeenCalledWith({ everything: false, pipelines: ["teamcity:buildType:A"], runs: ["teamcity:build:1"] });
  expect(wrote).toHaveLength(4);
  expect(runs.unread(failed("teamcity:build:41")), "an older page's run the backend marked").toBe(false);
  expect(runs.inBlue(running("teamcity:build:50")), "a running run, out of the blue").toBe(false);
  const toast = toasts.items.find((t) => t.text === "Marked 4 runs as read")!;
  toast.action!.run();
  await vi.waitFor(() => expect(unmark).toHaveBeenCalledWith(["teamcity:build:1", "teamcity:build:40", "teamcity:build:41", "teamcity:build:50"]));
  expect(runs.unread(failed("teamcity:build:1"))).toBe(true);
  expect(runs.inBlue(running("teamcity:build:50"))).toBe(true);
  expect(runs.unread(failed("teamcity:build:9")), "a run read before the press stays read").toBe(false);
});

test("Mark all as read of everything sends no pipeline, and draws every page held at once", async () => {
  const runs = store({ mark: vi.fn(async () => reading(["teamcity:build:70", "teamcity:build:80", "teamcity:build:90"])) });
  await runs.load();
  runs.hold("teamcity:buildType:A", ["teamcity:build:70"]);
  runs.hold("recent", ["teamcity:build:80"]);
  const pending = runs.markAll({ runs: [], pipelines: [], everything: true });
  expect(runs.unread(failed("teamcity:build:80")), "drawn before the backend answers").toBe(false);
  await pending;
  expect(runs.ports.mark).toHaveBeenCalledWith({ everything: true, pipelines: [], runs: ["teamcity:build:70", "teamcity:build:80"] });
  expect(runs.unread(failed("teamcity:build:90")), "one no page held, by the backend").toBe(false);
});

test("Mark all as read that finds nothing says so, with no Undo", async () => {
  const runs = store({ mark: vi.fn(async () => reading([])) });
  await runs.markAll({ runs: [], pipelines: ["teamcity:buildType:A"] });
  const toast = toasts.items.find((t) => t.text === "No unread runs")!;
  expect(toast.action).toBeUndefined();
});

test("Mark all as read whose write fails puts the drawn runs back", async () => {
  const runs = store({ mark: vi.fn(async () => Promise.reject({ code: "internal", message: "down", source_id: null })) });
  await runs.markAll({ runs: ["teamcity:build:1"], pipelines: [] });
  expect(runs.unread(failed("teamcity:build:1"))).toBe(true);
  expect(toasts.items.map((t) => t.text)).toEqual(["Could not mark the runs as read: down"]);
});

test("the Inbox's Done reads its run here too, and a mark made while the marks were read out is kept", async () => {
  let answer: (marks: RunMarks) => void = () => {};
  const runs = store({ read: vi.fn(() => new Promise<RunMarks>((resolve) => (answer = resolve))) });
  const loading = runs.load();
  runs.note("teamcity:build:3");
  answer(reading(["teamcity:build:4"]));
  await loading;
  expect(runs.unread(failed("teamcity:build:3"))).toBe(false);
  expect(runs.unread(failed("teamcity:build:4"))).toBe(false);
});

test("Mark all as read turns the runs an open pipeline page holds at once, and a page let go holds none", async () => {
  let answer: (marks: RunMarks) => void = () => {};
  const runs = store({ mark: vi.fn(() => new Promise<RunMarks>((resolve) => (answer = resolve))) });
  await runs.load();
  const release = runs.hold("teamcity:buildType:A", ["teamcity:build:70", "teamcity:build:71"]);
  runs.hold("teamcity:buildType:B", ["teamcity:build:80"]);
  const pending = runs.markAll({ runs: ["teamcity:build:1"], pipelines: ["teamcity:buildType:A"] });
  expect(runs.unread(failed("teamcity:build:70")), "drawn before the backend answers").toBe(false);
  expect(runs.unread(failed("teamcity:build:80")), "another pipeline's page is not in scope").toBe(true);
  expect(runs.ports.mark).toHaveBeenCalledWith({ everything: false, pipelines: ["teamcity:buildType:A"], runs: ["teamcity:build:1", "teamcity:build:70", "teamcity:build:71"] });
  answer(reading(["teamcity:build:1", "teamcity:build:70", "teamcity:build:71"]));
  await pending;
  release();
  await runs.markUnread(["teamcity:build:1", "teamcity:build:70", "teamcity:build:71"]);
  void runs.markAll({ runs: [], pipelines: ["teamcity:buildType:A"] });
  expect(runs.unread(failed("teamcity:build:70")), "the page was let go").toBe(true);
});

// -- the counts by bubble (#2568) ---------------------------------------------

const TESTS = "teamcity:buildType:Payout_Tests";
const LINT = "teamcity:buildType:Payout_Lint";

test("a folder sums its pipelines, a pipeline with no unread run counts zero, and the top bar holds no green while Builds is in front", async () => {
  const runs = store({ counts: vi.fn(async () => countsOf({ [TESTS]: { blue: 1, green: 2, red: 1 }, [LINT]: { yellow: 1, grey: 3 } })) });
  expect(runs.topBar, "nothing until read").toBeNull();
  await runs.readCounts();
  expect(runs.countsOf(TESTS, LINT)).toEqual({ blue: 1, green: 2, yellow: 1, red: 1, grey: 3 });
  expect(runs.countsOf("teamcity:buildType:Other")).toEqual(noCounts());
  expect(runs.topBar).toEqual({ blue: 1, green: 2, yellow: 1, red: 1, grey: 3 });
  await runs.inFront(true);
  expect(runs.topBar).toEqual({ blue: 1, green: 0, yellow: 0, red: 1, grey: 0 });
  expect(runs.countsOf(TESTS).green, "the rail keeps the visit's green").toBe(2);
  await runs.inFront(false);
  expect(runs.topBar?.green).toBe(2);
});

test("a run's bubble: blue unless marked, red unless read, green, yellow and grey only among the visit's", async () => {
  const runs = store({
    read: vi.fn(async () => ({ read: ["r:read"], while_running: ["r:swept"] })),
    counts: vi.fn(async () => countsOf({}, { visiting: true, fresh: ["r:pass", "r:warn"] })),
  });
  await runs.load();
  await runs.readCounts();
  const ran = { started: "2026-08-22T10:00:00Z", finished: null, state: null };
  const ended = (tone: "ok" | "warn" | "err" | "muted") => ({ started: "2026-08-22T10:00:00Z", finished: "2026-08-22T10:04:00Z", state: { text: tone, tone } });
  expect(runs.bubble({ entity_id: "r:run", ...ran })).toBe("blue");
  expect(runs.bubble({ entity_id: "r:swept", ...ran }), "marked while running").toBeNull();
  expect(runs.bubble({ entity_id: "r:queued", started: null, finished: null, state: null })).toBeNull();
  expect(runs.bubble({ entity_id: "r:fail", ...ended("err") })).toBe("red");
  expect(runs.bubble({ entity_id: "r:read", ...ended("err") })).toBeNull();
  expect(runs.bubble({ entity_id: "r:swept", ...ended("err") }), "it failed since").toBe("red");
  expect(runs.bubble({ entity_id: "r:pass", ...ended("ok") })).toBe("green");
  expect(runs.bubble({ entity_id: "r:warn", ...ended("warn") })).toBe("yellow");
  expect(runs.bubble({ entity_id: "r:old", ...ended("ok") }), "before the instant").toBeNull();
  expect(runs.bubble({ entity_id: "r:cancel", ...ended("muted") })).toBeNull();
});

test("Mark all as read at All pipelines takes every count to zero at once, and Undo brings the same green back", async () => {
  let held = countsOf({ [TESTS]: { green: 2, red: 1 }, [LINT]: { grey: 1 } });
  const moved: SeenMove = { to: "2026-11-27T12:00:00Z", everything: true, at: "2026-11-27T08:00:00Z", pipelines: [] };
  let answer: (marks: RunMarks) => void = () => {};
  const mark = vi.fn(() => new Promise<RunMarks>((resolve) => (answer = resolve)));
  const moveSeen = vi.fn(async () => ((held = countsOf({})), moved));
  const putBackSeen = vi.fn(async () => ((held = countsOf({ [TESTS]: { green: 2 }, [LINT]: { grey: 1 } })), true));
  const runs = store({ counts: vi.fn(async () => held), mark, moveSeen, putBackSeen });
  await runs.readCounts();
  const pressed = runs.markAll({ runs: [], pipelines: [], everything: true });
  expect(runs.topBar, "drawn at once").toEqual(noCounts());
  answer(reading(["teamcity:build:9"]));
  await pressed;
  await vi.waitFor(() => expect(runs.topBar).toEqual(noCounts()));
  expect(moveSeen).toHaveBeenCalledWith({ everything: true, pipelines: [], runs: [] });
  expect(toasts.items.at(-1)?.text, "the row and the instant's three").toBe("Marked 4 runs as read");

  toasts.items.at(-1)?.action?.run();
  await vi.waitFor(() => expect(runs.topBar).toEqual({ ...noCounts(), green: 2, grey: 1 }));
  expect(putBackSeen).toHaveBeenCalledWith(moved);
});

test("a folder's Mark all as read clears its pipelines' counts and no other's", async () => {
  const runs = store({ counts: vi.fn(async () => countsOf({ [TESTS]: { green: 2 }, [LINT]: { red: 1 } })) });
  await runs.readCounts();
  const pressed = runs.markAll({ runs: [], pipelines: [TESTS] });
  expect(runs.countsOf(TESTS)).toEqual(noCounts());
  expect(runs.countsOf(LINT).red).toBe(1);
  await pressed;
});

test("the backend hears a section change once the page has had its turn, the last change only; the top bar follows at once", async () => {
  vi.useFakeTimers();
  try {
    const inFront = vi.fn(async () => true);
    const runs = store({ inFront, counts: vi.fn(async () => countsOf({ [TESTS]: { green: 2 } })) });
    runs.settleMs = 600;
    await runs.readCounts();
    void runs.inFront(true);
    expect(runs.topBar?.green, "at once").toBe(0);
    void runs.inFront(false);
    const heard = runs.inFront(true);
    await vi.advanceTimersByTimeAsync(599);
    expect(inFront, "the page reads first").not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    await heard;
    expect(inFront.mock.calls).toEqual([[true]]);
  } finally {
    vi.useRealTimers();
  }
});
