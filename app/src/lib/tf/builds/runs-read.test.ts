/** The marks a person's runs carry (`runs-read.svelte.ts`, ux18, ADR-0063): one store for the window, every mark drawn at once. */
import { beforeEach, expect, test, vi } from "vitest";

import type { MarkScope, RunMarks, RunRow } from "../../ipc/entity";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import { RunsRead, type RunsReadPorts } from "./runs-read.svelte";

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
  runs.ports = {
    read: vi.fn(async () => reading([])),
    mark: vi.fn(async (scope: MarkScope) => reading(scope.runs)),
    unmark: vi.fn(async (ids: string[]) => ids),
    open: vi.fn(async () => true),
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
