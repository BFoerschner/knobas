// The answer cache (the instant-open pass, 2026-11-12): what it keeps, what drops it, and that a kept answer is the
// answer the backend gave.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cacheEvents, disableAnswerCache, enableAnswerCache, keptAnswer, warm } from "./answer-cache";
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

  it("drops nothing on the runs' counts or the instant green counts from, and never keeps the counts (#2568)", async () => {
    // Unknown here, the top bar's every refresh and every entry to Builds dropped every kept answer, and a return to
    // Builds read the pipelines again (the open budget's `builds late` at 113 to 428 ms, against 20 to 30 ms).
    await ask("pipelines");
    await ask("run_counts");
    await ask("builds_in_front", { front: true });
    await ask("move_seen", { scope: { everything: true, pipelines: [], runs: [] } });
    await ask("put_back_seen", { seen: {} });
    await ask("builds_in_front", { front: false });
    await ask("run_counts");
    await ask("pipelines");
    expect(asked("pipelines")).toBe(1);
    expect(asked("run_counts"), "read fresh each time").toBe(2);
  });

  it("drops nothing when a source is asked for a run: the run's end drops by its own event (#2569)", async () => {
    // Recent asks each idle CI source for a run once it has drawn, which at start is every one of them: unknown here,
    // that ask dropped every kept answer and every start-up read still on its way, the pipelines' 571 KB among them.
    await ask("pipelines");
    await ask("sync_now", { sourceId: "live-teamcity" });
    await ask("pipelines");
    expect(asked("pipelines")).toBe(1);
  });

  it("drops nothing on a compare's reads of git and the source, a page at a time or called off", async () => {
    // The compare pass (2026-11-13): a diff of thousands of files reads a file's lines per card, and dropped every kept
    // answer each time; so did the check behind a predicted compare.
    await ask("docs_outline");
    for (const cmd of ["entity_range_files", "entity_range_file", "entity_source_range_files", "entity_source_range_diff", "cancel_read", "entity_range_names", "entity_range_file_pages"]) await ask(cmd, { read: "r" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
  });

  it("hands a kept answer over in the asker's own task, a copy, and asks nothing for one it does not keep", async () => {
    answers["docs_page"] = () => ({ entity_id: "confluence:2", title: "On-call handbook" });
    expect(keptAnswer("docs_page", { entityId: "confluence:2" })).toBeUndefined();
    const read = await ask("docs_page", { entityId: "confluence:2" });
    const held = keptAnswer("docs_page", { entityId: "confluence:2" });
    expect(held).toEqual(read);
    expect(held).not.toBe(read);
    expect(keptAnswer("site_show", {})).toBeUndefined();
    expect(asked("docs_page")).toBe(1);
  });

  it("drops nothing on a visit to a source's site: its view shown, pointed, hidden and closed, and its address asked of the mirror", async () => {
    // Fable's ruling (2026-11-25): each of these dropped every kept answer, so every page opened after a site read again.
    await ask("docs_page", { entityId: "confluence:2" });
    for (const cmd of ["site_open", "site_show", "resolve_url", "site_do", "site_hide", "site_answer", "site_close"]) await ask(cmd, { source: "confluence" });
    await ask("docs_page", { entityId: "confluence:2" });
    expect(asked("docs_page")).toBe(1);
  });

  it("drops nothing on the estate's reads, and keeps them over a visit", async () => {
    // The ux30 Windows pass (2026-11-23): Assets' columns and types, unknown here, dropped every kept answer, so Builds,
    // Monitors and Tickets opened after Assets read the backend again (Builds late 257 ms drawn at 4x on the PC).
    await ask("docs_outline");
    await ask("asset_types");
    await ask("asset_tree", { parentId: null });
    await ask("asset_tree", { parentId: "asset:site" });
    await ask("depends_on_this", { assetId: "asset:vm" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    await ask("visit_entity", { entityId: "asset:vm" });
    await ask("asset_tree", { parentId: null });
    await ask("depends_on_this", { assetId: "asset:vm" });
    expect(asked("asset_tree")).toBe(2);
    expect(asked("depends_on_this")).toBe(1);
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

  it("keeps a ticket's own reads over the visit its page makes on every open", async () => {
    // ux19: the visit dropped the ticket, its reach, its card's sections and its assets, so a return read them again.
    const reads: [string, Record<string, unknown>][] = [
      ["get_entity", { entityId: "jira:B" }],
      ["job_reach", { entityId: "jira:B" }],
      ["card_reads", { entityId: "jira:B", sections: [], limit: 200 }],
      ["get_asset", { assetId: "asset:1" }],
    ];
    for (const [cmd, args] of reads) await ask(cmd, args);
    await ask("visit_entity", { entityId: "jira:B" });
    for (const [cmd, args] of reads) await ask(cmd, args);
    expect(reads.map(([cmd]) => asked(cmd))).toEqual([1, 1, 1, 1]);
  });

  it("drops on a ticket's promotion only when it made the context", async () => {
    // ux19: every open of a ticket promotes it, which dropped every kept answer, the ticket's own sections among them.
    const now = () => new Date().toISOString();
    let created = "2026-01-01T00:00:00Z";
    answers["promote_context"] = () => ({ id: "ctx:1", kind: "ticket", anchor_id: "jira:B", created_at: created });
    await ask("docs_outline");
    await ask("promote_context", { entityId: "jira:B" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);

    // One made now drops everything, as a write does; promoted again it is found, and drops nothing.
    created = now();
    answers["promote_context"] = () => ({ id: "ctx:2", kind: "epic", anchor_id: "jira:E", created_at: created });
    await ask("promote_context", { entityId: "jira:E" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
    await ask("promote_context", { entityId: "jira:E" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(2);
  });

  it("keeps no read answered while a promotion that made its context was out", async () => {
    let answer = (_: unknown) => {};
    bridge.invoke = vi.fn((cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      if (cmd === "promote_context") return new Promise((resolve) => (answer = resolve));
      return Promise.resolve(answers[cmd]?.() ?? null);
    });
    const promoting = ask("promote_context", { entityId: "jira:E" });
    await ask("job_reach", { entityId: "jira:E" });
    answer({ id: "ctx:3", kind: "epic", anchor_id: "jira:E", created_at: new Date().toISOString() });
    await promoting;
    await ask("job_reach", { entityId: "jira:E" });
    expect(asked("job_reach")).toBe(2);
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

  it("drops on a refresh with no write what its source could change, everything after a write, and nothing on a failed one or at the call", async () => {
    await ask("list_sources");
    await ask("docs_outline");
    await ask("monitor_roster");
    // A page's refresh of its runs in flight drops nothing as it is asked: its end says what moved.
    await ask("refresh_entity", { entityId: "kuma:monitor:1", behind: true });
    await ask("docs_outline");
    await ask("monitor_roster");
    expect([asked("docs_outline"), asked("monitor_roster")]).toEqual([1, 1]);
    // A followed run moved (the TeamCity run fix): what a run of its source drops.
    cacheEvents.refreshed({ entity_id: "kuma:monitor:1", write_id: null, ended: "refreshed" });
    await ask("docs_outline");
    await ask("monitor_roster");
    expect([asked("docs_outline"), asked("monitor_roster")]).toEqual([1, 2]);
    // One that failed moved nothing.
    cacheEvents.refreshed({ entity_id: "conf:page:1", write_id: null, ended: "failed" });
    await ask("docs_outline");
    expect(asked("docs_outline")).toBe(1);
    // A write's read-back: every answer, as before.
    cacheEvents.refreshed({ entity_id: "kuma:monitor:1", write_id: 7, ended: "refreshed" });
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

  it("warms nothing while a pointer moves with a button down: a drag is input, so no read lands in it", async () => {
    await ask("docs_outline");
    cacheEvents.moved();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance", "requestIdleCallback", "cancelIdleCallback"] });
    try {
      const warming = warm();
      // A card held for three seconds, the pointer moving every 100 ms: no read in it.
      for (let at = 0; at < 3000; at += 100) {
        window.dispatchEvent(new PointerEvent("pointermove", { buttons: 1, pointerType: "mouse" }));
        await vi.advanceTimersByTimeAsync(100);
      }
      expect(asked("docs_outline")).toBe(1);
      // A pointer that only passes over the window (no button down) holds nothing: a second after the drag, it warms.
      window.dispatchEvent(new PointerEvent("pointermove", { buttons: 0, pointerType: "mouse" }));
      await vi.advanceTimersByTimeAsync(2200);
      await warming;
      expect(asked("docs_outline")).toBe(2);
    } finally {
      vi.useRealTimers();
    }
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
