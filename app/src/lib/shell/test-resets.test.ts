/**
 * **A module-level store with no reset is red.**
 *
 * `test-setup.ts` resets the stores in `test-resets.ts` after every test. This
 * lists the sources the way `residue.test` lists components: any module-level
 * `$state`, and any exported singleton made by a `create…()` factory, has to
 * be a key of `RESETS` or be named in `NOT_YET_RESET`.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test, vi } from "vitest";

import { NOT_YET_RESET, RESETS, UNSCANNED } from "./test-resets";

const LIB = join(import.meta.dirname, "..");

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return /(?<!\.test)\.svelte\.ts$|^view-read\.ts$/.test(entry.name) && !entry.name.includes(".test.") ? [path] : [];
  });
}

/** Column-0 declarations: module-level `$state`, or an exported `create…()` singleton. */
const DECLARATION = /^(?:export )?(?:const|let) (\w+)\b[^=\n]*= (?:\$state\b|create\w+\()/gm;

export function moduleStores(text: string): string[] {
  return [...text.matchAll(DECLARATION)].map((match) => match[1]!);
}

function found(): string[] {
  return sources(LIB).flatMap((path) => moduleStores(readFileSync(path, "utf8")).map((name) => `${relative(LIB, path)}#${name}`));
}

test("every module-level store has a reset or a named debt", () => {
  const known = new Set([...Object.keys(RESETS), ...NOT_YET_RESET]);
  expect(found().filter((id) => !known.has(id)), "add it to RESETS in test-resets.ts").toEqual([]);
});

test("neither list names a store that is gone, and no store is in both", () => {
  const present = new Set([...found(), ...UNSCANNED]);
  expect(Object.keys(RESETS).filter((id) => !present.has(id)), "RESETS names a store that no longer exists").toEqual([]);
  expect(NOT_YET_RESET.filter((id) => !present.has(id)), "NOT_YET_RESET names a store that is reset or gone").toEqual([]);
  expect(NOT_YET_RESET.filter((id) => id in RESETS)).toEqual([]);
  expect(UNSCANNED.filter((id) => !(id in RESETS)), "an unscanned store needs its reset").toEqual([]);
});

test("no store is left on the debt list", () => {
  expect(NOT_YET_RESET).toEqual([]);
});

test("the scan sees what it is meant to", () => {
  expect(moduleStores("export const a = $state({});\nlet b = $state<X>(null);\nexport const c = createC();\n  const inner = $state(1);\nconst d = new Set();")).toEqual(["a", "b", "c"]);
});

// ---------------------------------------------------------------------------
// The resets do what the list says.
// ---------------------------------------------------------------------------

import { createHealth, health } from "./health.svelte";
import { createKindRegistry, kindRegistry } from "./kind-registry.svelte";
import { lifecycle } from "./lifecycle.svelte";
import { resetStores } from "./test-resets";
import { push, toasts } from "./toasts.svelte";
import { noteWithdrawn, onWithdrawn, writeQueuePanel, openWriteQueue } from "./write-queue.svelte";

test("resetStores puts every listed store back to a fresh window", async () => {
  push({ text: "left over" });
  openWriteQueue();
  const gone = vi.fn();
  onWithdrawn(7, gone);
  health.patch({ source_id: "gitea", state: "unauthorized" } as Parameters<typeof health.patch>[0]);

  await resetStores();

  expect(toasts.items).toEqual([]);
  expect(writeQueuePanel.open).toBe(false);
  noteWithdrawn(7);
  expect(gone, "a withdrawal subscriber outlived the reset").not.toHaveBeenCalled();
  expect(health.all).toEqual([]);
  expect(health.phase).toBe("loading");
  expect(kindRegistry.frameKinds("ci", "run")).toBeNull();
  expect(lifecycle.status).toBeNull();
  expect(lifecycle.db.state).toBe("starting");
  expect(lifecycle.ready).toBe(false);
});

test("health.reset drops a seeded store back to loading", async () => {
  const h = createHealth({ credentialHealth: async () => [], listen: async () => () => {} });
  const stop = h.start();
  await h.reseed();
  expect(h.phase).toBe("ready");
  h.reset();
  expect(h.phase).toBe("loading");
  stop();
});

test("kindRegistry.reset makes the next load ask again", async () => {
  const listAdapters = vi.fn(async () => []);
  const registry = createKindRegistry({ listAdapters });
  await registry.load();
  await registry.load();
  expect(listAdapters).toHaveBeenCalledTimes(1);
  registry.reset();
  await registry.load();
  expect(listAdapters).toHaveBeenCalledTimes(2);
});
