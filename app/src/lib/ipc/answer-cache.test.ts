// The answer cache (the instant-open pass, 2026-11-12): what it keeps, what drops it, and that a kept answer is the
// answer the backend gave.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cacheEvents, disableAnswerCache, enableAnswerCache, warm } from "./answer-cache";

type Bridge = { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
let calls: { cmd: string; args: unknown }[];
let answers: Record<string, () => unknown>;
let bridge: Bridge;

function ask(cmd: string, args: unknown = {}): Promise<unknown> {
  return (window as unknown as { __TAURI_INTERNALS__: Bridge }).__TAURI_INTERNALS__.invoke(cmd, args);
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
  Object.assign(window, { __TAURI_INTERNALS__: bridge });
  enableAnswerCache();
});

afterEach(() => {
  disableAnswerCache();
  delete (window as unknown as { __TAURI_INTERNALS__?: Bridge }).__TAURI_INTERNALS__;
});

const asked = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

describe("the answer cache", () => {
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
    Object.assign(window, { __TAURI_INTERNALS__: bridge });
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
