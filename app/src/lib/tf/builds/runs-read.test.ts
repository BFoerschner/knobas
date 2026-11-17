/** The runs marked read (`runs-read.svelte.ts`, ux18): one set for the window, every mark drawn at once. */
import { beforeEach, expect, test, vi } from "vitest";

import type { RunRow } from "../../ipc/entity";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import { RunsRead, type RunsReadPorts } from "./runs-read.svelte";

function failed(id: string, extra: Partial<RunRow> = {}): Pick<RunRow, "entity_id" | "finished" | "state"> {
  return { entity_id: id, finished: "2026-08-22T10:04:00Z", state: { text: "FAILURE", tone: "err" }, ...extra };
}

function store(ports: Partial<RunsReadPorts> = {}): RunsRead {
  const runs = new RunsRead();
  runs.ports = {
    read: vi.fn(async () => []),
    mark: vi.fn(async (ids: string[]) => ids),
    unmark: vi.fn(async (ids: string[]) => ids),
    ...ports,
  };
  return runs;
}

beforeEach(() => resetToasts());

test("before the marks are read every failure is unread; read once, a marked one is not, and a second load reads nothing", async () => {
  const read = vi.fn(async () => ["teamcity:build:1"]);
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

test("one mark is drawn before its write answers, and the toggle takes it back", async () => {
  let answer: (ids: string[]) => void = () => {};
  const mark = vi.fn(() => new Promise<string[]>((resolve) => (answer = resolve)));
  const unmark = vi.fn(async (ids: string[]) => ids);
  const runs = store({ mark, unmark });
  const run = failed("teamcity:build:5");
  const pending = runs.toggle(run);
  expect(runs.unread(run), "drawn at once").toBe(false);
  answer(["teamcity:build:5"]);
  await pending;
  expect(mark).toHaveBeenCalledWith(["teamcity:build:5"], []);
  await runs.toggle(run);
  expect(runs.unread(run)).toBe(true);
  expect(unmark).toHaveBeenCalledWith(["teamcity:build:5"]);
  expect(toasts.items, "one run's mark says nothing").toHaveLength(0);
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

test("Mark all as read draws the drawn runs at once, adds what the backend wrote, and Undo takes back exactly that", async () => {
  const mark = vi.fn(async () => ["teamcity:build:1", "teamcity:build:40", "teamcity:build:41"]);
  const unmark = vi.fn(async (ids: string[]) => ids);
  const runs = store({ read: vi.fn(async () => ["teamcity:build:9"]), mark, unmark });
  await runs.load();
  const wrote = await runs.markAll({ runs: ["teamcity:build:1", "teamcity:build:9"], pipelines: ["teamcity:buildType:A"] });
  expect(mark).toHaveBeenCalledWith(["teamcity:build:1"], ["teamcity:buildType:A"]);
  expect(wrote).toHaveLength(3);
  expect(runs.unread(failed("teamcity:build:41")), "an older page's run the backend marked").toBe(false);
  const toast = toasts.items.find((t) => t.text === "Marked 3 runs as read")!;
  toast.action!.run();
  await vi.waitFor(() => expect(unmark).toHaveBeenCalledWith(["teamcity:build:1", "teamcity:build:40", "teamcity:build:41"]));
  expect(runs.unread(failed("teamcity:build:1"))).toBe(true);
  expect(runs.unread(failed("teamcity:build:9")), "a run read before the press stays read").toBe(false);
});

test("Mark all as read that finds nothing says so, with no Undo", async () => {
  const runs = store({ mark: vi.fn(async () => []) });
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
  let answer: (ids: string[]) => void = () => {};
  const runs = store({ read: vi.fn(() => new Promise<string[]>((resolve) => (answer = resolve))) });
  const loading = runs.load();
  runs.note("teamcity:build:3");
  answer(["teamcity:build:4"]);
  await loading;
  expect(runs.unread(failed("teamcity:build:3"))).toBe(false);
  expect(runs.unread(failed("teamcity:build:4"))).toBe(false);
});
