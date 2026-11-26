// A run in flight moves on where it is drawn (the TeamCity run fix, 2026-11-25): which runs a page refreshes, and how
// often.
import { afterEach, expect, test, vi } from "vitest";

import type { RunRow } from "../../ipc/entity";
import { forgetEchoes } from "../queued.svelte";
import { InFlightReads, IN_FLIGHT_AGAIN_MS, inFlight, phase, runsInFlight } from "./runs-in-flight";
import { queuedRunRows, withQueued } from "./pipeline";

afterEach(() => {
  vi.useRealTimers();
  forgetEchoes();
});

function run(no: number, started: string | null, finished: string | null): RunRow {
  return {
    entity_id: `tc:build:${no}`,
    number: String(no),
    state: null,
    branch: "main",
    started,
    finished,
    duration_seconds: null,
    updated_at: started,
    web_url: null,
    trigger: "person",
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
  };
}

test("the runs in flight are the newest five queued or running ones, never a row queued here and not read back", () => {
  const echo = queuedRunRows([{ write: 3, kind: "run", text: "", base: null, state: "queued", surface: null, source: null }], null)[0]!;
  const rows = [echo, ...[9, 8, 7, 6, 5, 4].map((no) => run(no, no % 2 ? "2026-11-25T10:00:00Z" : null, null)), run(3, "a", "b")];
  expect(inFlight(echo)).toBe(false);
  expect(runsInFlight(rows)).toEqual(["tc:build:9", "tc:build:8", "tc:build:7", "tc:build:6", "tc:build:5"]);
  expect(runsInFlight(null)).toEqual([]);
  expect([phase(null), phase(run(1, null, null)), phase(run(1, "a", null)), phase(run(1, "a", "b"))]).toEqual(["none", "queued", "running", "finished"]);
});

test("a page refreshes its runs in flight at most once in 5 s, and none when it has none", () => {
  vi.useFakeTimers({ toFake: ["Date"], now: 1_000_000 });
  const asked: string[] = [];
  const reads = new InFlightReads(async (id) => void asked.push(id));
  reads.read([]);
  reads.read(["tc:build:9"]);
  reads.read(["tc:build:9"]);
  vi.setSystemTime(1_000_000 + IN_FLIGHT_AGAIN_MS - 1);
  reads.read(["tc:build:9"]);
  vi.setSystemTime(1_000_000 + IN_FLIGHT_AGAIN_MS);
  reads.read(["tc:build:9", "tc:build:8"]);
  reads.reset();
  reads.read(["tc:build:7"]);
  return Promise.resolve().then(() => expect(asked).toEqual(["tc:build:9", "tc:build:9", "tc:build:8", "tc:build:7"]));
});

test("a rail row draws a run queued here on top until its own read holds it", () => {
  const pipeline = { recent: [run(904, "a", "b")] };
  const echo = { write: 51, kind: "run", text: "", base: "tc:build:904", state: "queued" as const, surface: null, source: null };
  expect(withQueued(pipeline, [])).toBe(pipeline);
  const drawn = withQueued(pipeline, [echo]);
  expect(drawn.recent).toHaveLength(2);
  expect([drawn.recent[0]!.started, drawn.recent[0]!.finished]).toEqual([null, null]);
  // The rail's read holds a newer run started by a person: the row is the read's.
  const held = { recent: [run(905, null, null), run(904, "a", "b")] };
  expect(withQueued(held, [echo])).toBe(held);
});

test("a page's read again on a run's refresh gets what the refresh wrote, not the window's kept answer", async () => {
  const { enableAnswerCache, disableAnswerCache } = await import("../../ipc/answer-cache");
  const { invoke } = await import("../../ipc/invoke");
  const { hearRuns } = await import("./runs-in-flight");
  let state = "running";
  const internals = {};
  Object.defineProperty(internals, "invoke", { value: async (cmd: string) => (cmd === "pipeline_runs" ? [{ entity_id: "tc:build:5", state }] : null) });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
  enableAnswerCache();
  try {
    const read = () => invoke("pipeline_runs", { pipelineId: "tc:buildType:P" }) as Promise<{ state: string }[]>;
    expect((await read())[0]!.state).toBe("running");
    state = "finished";
    // The page's listener heard the event before the cache's own listener did (the real app, 2026-11-25).
    let raise: (payload: unknown) => void = () => {};
    const listen = async (_event: string, handler: (event: { payload: unknown }) => void) => ((raise = (payload) => handler({ payload })), () => {});
    let reread: Promise<{ state: string }[]> | null = null;
    hearRuns(listen, () => true, () => void (reread = read()));
    await Promise.resolve();
    raise({ entity_id: "tc:build:5", write_id: null, ended: "refreshed" });
    expect((await reread!)[0]!.state).toBe("finished");
  } finally {
    disableAnswerCache();
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
});
