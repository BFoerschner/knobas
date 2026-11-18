// The answer cache (the instant-open pass, 2026-11-12): what it keeps, what drops it, and that a kept answer is the
// answer the backend gave.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cacheEvents, disableAnswerCache, enableAnswerCache, warm } from "./answer-cache";
import { invoke } from "./invoke";

type Bridge = { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
let calls: { cmd: string; args: unknown }[];
let answers: Record<string, () => unknown>;
let bridge: Bridge;

/** A read as the app makes it: through `ipc/invoke.ts`, never the bridge itself. */
function ask(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  return invoke(cmd, args);
}

/**
 * Install `window.__TAURI_INTERNALS__` the way Tauri 2.11's `scripts/core.js` does: `invoke` by `defineProperty` with
 * `value` only, so it can't be assigned. v3.2.9-ux7's cache assigned to it, which threw in the packaged app and left
 * the window on *Starting the local database*. The read-only `invoke` hands each call to `bridge`, which a test swaps.
 */
function installBridge(): void {
  const internals = {};
  Object.defineProperty(internals, "invoke", { value: (cmd: string, args?: unknown) => bridge.invoke(cmd, args) });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
}

beforeEach(() => {
  calls = [];
  answers = {
    list_entities: () => ({ rows: [{ id: "a" }, { id: "b" }] }),
    docs_outline: () => ({ spaces: [{ key: "ENG", pages: [] }] }),
    monitor_roster: () => [{ id: "m1" }],
    list_sources: () => [
      { id: "kuma", roles: [{ role: "monitoring" }] },
      { id: "conf", roles: [{ role: "wiki" }] },
    ],
  };
  bridge = {
    invoke: vi.fn(async (cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      return answers[cmd]?.() ?? null;
    }),
  };
  installBridge();
  enableAnswerCache();
});

afterEach(() => {
  disableAnswerCache();
  delete (window as unknown as { __TAURI_INTERNALS__?: Bridge }).__TAURI_INTERNALS__;
});

const asked = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

describe("the answer cache", () => {
  it("starts over Tauri's read-only invoke, and never writes to the bridge", () => {
    disableAnswerCache();
    const internals = (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__;
    const before = Object.getOwnPropertyDescriptor(internals, "invoke");
    expect(before?.writable, "the test bridge must be read-only, as Tauri's is").toBe(false);
    expect(() => enableAnswerCache()).not.toThrow();
    expect(Object.getOwnPropertyDescriptor(internals, "invoke")?.value).toBe(before?.value);
  });

  it("answers through the app's own invoke over the read-only bridge", async () => {
    await ask("docs_outline");
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
  });

  it("answers a read again from memory, with an answer the caller may change", async () => {
    const first = (await ask("list_entities", { filter: { kinds: ["pr"] } })) as { rows: unknown[] };
    first.rows.reverse();
    first.rows.push({ id: "mine" });
    const second = await ask("list_entities", { filter: { kinds: ["pr"] } });
    expect(asked("list_entities")).toBe(1);
    expect(second).toEqual({ rows: [{ id: "a" }, { id: "b" }] });
    // Other arguments are another read.
    await ask("list_entities", { filter: { kinds: ["ticket"] } });
    expect(asked("list_entities")).toBe(2);
  });

  it("shares one read between two asking at once", async () => {
    const [a, b] = await Promise.all([ask("docs_outline"), ask("docs_outline")]);
    expect(asked("docs_outline")).toBe(1);
    expect(a).toEqual(b);
    expect(a).not.toBe(b);
  });

  it("drops every answer on a write, and none on a quiet command", async () => {
    await ask("docs_outline");
    await ask("sync_status");
    await ask("record_capture_context", { context: null });
    await ask("plugin:event|listen", { event: "x" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    await ask("submit_write", { write: {} });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });

  it("drops nothing on a compare's reads of git and the source, a page at a time or called off", async () => {
    // The compare pass (2026-11-13): a diff of thousands of files reads a file's lines per card, and dropped every kept
    // answer each time; so did the check behind a predicted compare.
    await ask("docs_outline");
    for (const cmd of ["entity_range_files", "entity_range_file", "entity_source_range_files", "entity_source_range_diff", "cancel_read", "entity_range_names", "entity_range_file_pages"]) await ask(cmd, { read: "r" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
  });

  it("drops nothing on a clone's reads or a pipeline's older runs, and shares a read they came between", async () => {
    // The ux18 Windows pass (2026-11-18): Files reading a clone's folder as a repository opened dropped every kept answer
    // and the press's reads on their way, so the page asked its row and the rest a second time.
    await ask("docs_outline");
    const first = ask("list_entities", { filter: {} });
    for (const cmd of ["entity_local_tree", "entity_local_dir", "entity_local_paths", "entity_local_file", "entity_file_bytes", "entity_checkout_diff", "entity_commit_diff", "older_pipeline_runs"]) await ask(cmd, { entityId: "r" });
    const second = ask("list_entities", { filter: {} });
    await Promise.all([first, second]);
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    expect(asked("list_entities")).toBe(1);
  });

  it("keeps on a visit only what never reads visits", async () => {
    await ask("docs_outline");
    await ask("list_entities", { filter: { kinds: ["pr"] } });
    await ask("visit_entity", { entityId: "x" });
    await ask("docs_outline");
    await ask("list_entities", { filter: { kinds: ["pr"] } });
    expect([asked("docs_outline"), asked("list_entities")]).toEqual([2, 1]);
  });

  it("drops on the merge follower only when it moved a ticket, and on a heartbeat only the time reads", async () => {
    let moved = 0;
    answers["follow_merges"] = () => moved;
    await ask("docs_outline");
    await ask("day_blocks", { day: "2026-11-12" });
    await ask("follow_merges");
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    await ask("timer_heartbeat", { foreground: null });
    await ask("docs_outline");
    await ask("day_blocks", { day: "2026-11-12" });
    expect([asked("docs_outline"), asked("day_blocks")]).toEqual([1, 2]);
    moved = 2;
    await ask("follow_merges");
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });

  it("drops on a run's end what a source of its roles could change", async () => {
    await ask("list_sources");
    await ask("docs_outline");
    await ask("monitor_roster");
    await ask("list_entities", { filter: { kinds: ["page"] } });
    await ask("inbox_count");
    cacheEvents.syncState({ source_id: "kuma", running: false, last_changed: true });
    await ask("docs_outline");
    await ask("monitor_roster");
    await ask("list_entities", { filter: { kinds: ["page"] } });
    await ask("inbox_count");
    expect([asked("docs_outline"), asked("monitor_roster"), asked("list_entities"), asked("inbox_count")]).toEqual([1, 2, 1, 2]);
    // A run that changed nothing, or one still running, drops nothing.
    cacheEvents.syncState({ source_id: "conf", running: false, last_changed: false });
    cacheEvents.syncState({ source_id: "conf", running: true, last_changed: true });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    cacheEvents.syncState({ source_id: "conf", running: false, last_changed: true });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });

  it("keeps the sources' status until a run starts or ends, whatever it changed", async () => {
    await ask("sync_status");
    await ask("credential_health");
    await ask("docs_outline");
    await ask("sync_status");
    expect(asked("sync_status")).toBe(1);
    cacheEvents.syncState({ source_id: "conf", running: true, last_changed: null });
    await ask("sync_status");
    await ask("credential_health");
    await ask("docs_outline");
    expect([asked("sync_status"), asked("credential_health"), asked("docs_outline")]).toEqual([2, 1, 1]);
    cacheEvents.health();
    await ask("credential_health");
    expect(asked("credential_health")).toBe(2);
  });

  it("drops everything on a run of a source whose roles it never read", async () => {
    await ask("docs_outline");
    cacheEvents.syncState({ source_id: "stranger", running: false, last_changed: true });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });

  it("keeps no answer that was on its way when the mirror moved", async () => {
    let release: (value: unknown) => void = () => {};
    bridge.invoke = vi.fn((cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      return cmd === "docs_outline" && asked("docs_outline") === 1 ? new Promise((resolve) => (release = resolve)) : Promise.resolve(answers[cmd]?.() ?? null);
    });
    disableAnswerCache();
    enableAnswerCache();
    const pending = ask("docs_outline");
    cacheEvents.moved();
    release({ spaces: "old" });
    expect(await pending).toEqual({ spaces: "old" });
    expect(await ask("docs_outline")).toEqual({ spaces: [{ key: "ENG", pages: [] }] });
    expect(asked("docs_outline")).toBe(2);
  });

  it("warms the reads made, so the next ask is answered from memory", async () => {
    await ask("docs_outline");
    await ask("monitor_roster");
    cacheEvents.moved();
    await warm(() => Promise.resolve());
    expect([asked("docs_outline"), asked("monitor_roster")]).toEqual([2, 2]);
    await ask("docs_outline");
    await ask("monitor_roster");
    expect([asked("docs_outline"), asked("monitor_roster")]).toEqual([2, 2]);
  });

  it("passes everything through once off", async () => {
    disableAnswerCache();
    await ask("docs_outline");
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });
});

describe("a kept answer's copy", () => {
  it("is the answer, every level its own, and a value JSON never holds is cloned", async () => {
    const { copy } = await import("./answer-cache");
    const when = new Date(0);
    const answer = { rows: [{ id: 1, tags: ["a", "b"], open: true, gone: null }, { id: 2, tags: [], open: false, gone: null }], total: 2, when };
    const got = copy(answer);
    expect(got).toEqual(answer);
    expect(got).not.toBe(answer);
    expect(got.rows).not.toBe(answer.rows);
    expect(got.rows[0]).not.toBe(answer.rows[0]);
    expect(got.rows[0]!.tags).not.toBe(answer.rows[0]!.tags);
    expect(got.when).not.toBe(when);
    expect(got.when.getTime()).toBe(0);
    got.rows[0]!.tags.push("c");
    got.rows.sort((a, b) => b.id - a.id);
    expect(answer.rows.map((row) => row.id)).toEqual([1, 2]);
    expect(answer.rows[0]!.tags).toEqual(["a", "b"]);
    expect(copy("text")).toBe("text");
    expect(copy(null)).toBeNull();
  });
});
