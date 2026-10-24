/**
 * **The `?fake-ipc` backend starts each test from its seed** (#2004).
 *
 * The fake keeps what it was told in module state, as a server would, and a
 * test file loads it once. `test-setup.ts` runs `resetFake()` after every test
 * (through `test-resets.ts`), so the tests below are ordered on purpose: the
 * first writes, the second reads what a freshly loaded fake answers. The scan
 * below fails on a value a handler can change that no module registers.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";

import { demoHandlers } from "./fake-tauri";
import { FIXTURE_NOTE_ID } from "./fake/notes";

type Calls = Record<string, (args: Record<string, unknown>) => unknown>;

/** What a handful of reads answer, one per module the first test writes to. */
function reads(h: Calls) {
  return {
    note: noteOrNull(h),
    shortcut: h["capture_shortcut"]!({}),
    parentLinks: h["parent_links"]!({}),
    layers: (h["dependencies"]!({}) as { layers: { id: string }[] }).layers.map((layer) => layer.id),
    sets: (h["link_sets"]!({}) as { id: string }[]).map((set) => set.id),
    views: (h["ticket_views"]!({}) as { id: string }[]).map((view) => view.id),
    acked: (h["open_alerts"]!({}) as { acked_at: string | null }[]).map((alert) => alert.acked_at !== null),
  };
}

function noteOrNull(h: Calls): unknown {
  try {
    return h["get_note"]!({ noteId: FIXTURE_NOTE_ID });
  } catch {
    return null;
  }
}

let seed: ReturnType<typeof reads> | undefined;

test("a test writes to the fake", () => {
  const h = demoHandlers() as Calls;
  seed = reads(h);
  const alert = (h["open_alerts"]!({}) as { monitor_id: string; acked_at: string | null }[]).find((row) => row.acked_at === null)!;

  expect(h["delete_note"]!({ noteId: FIXTURE_NOTE_ID })).toBe(true);
  h["set_capture_shortcut"]!({ accelerator: "Alt+Shift+N" });
  h["set_parent_links"]!({ enabled: !seed.parentLinks });
  h["create_dep_layer"]!({ name: "Scratch" });
  h["delete_link_set"]!({ id: seed.sets[0] });
  h["delete_ticket_view"]!({ id: "blocked" });
  h["ack_alert"]!({ monitorId: alert.monitor_id });

  const after = reads(h);
  for (const key of Object.keys(seed) as (keyof typeof seed)[]) {
    expect(after[key], `${key} did not change`).not.toEqual(seed[key]);
  }
});

test("the next test reads the fake's seed", () => {
  expect(seed, "runs after the test that writes").toBeDefined();
  expect(reads(demoHandlers() as Calls)).toEqual(seed);
});

// ---------------------------------------------------------------------------
// The scan: every value a handler can change is registered.
// ---------------------------------------------------------------------------

const FAKE = join(import.meta.dirname, "fake");

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? sources(path) : entry.name.endsWith(".ts") ? [path] : [];
  });
}

/** A column-0 `let` that does not start at a number: an id counter counts on across tests (#1762). */
const LET = /^(?:export )?let (\w+)\b[^=\n]*(?:= (?!\d)|;$)/gm;
const CONST = /^(export )?const (\w+)\b/gm;

/** A `const` some handler changes (an indented call that writes to it, or an index assigned) in `texts`. */
function written(name: string, texts: string[]): boolean {
  const write = new RegExp(
    String.raw`^[ \t]+.*(?<![\w.])\(?${name}(?: as [^)]+\))?(?:\.(?:push|pop|shift|unshift|splice|set|add|delete|clear|sort|reverse|fill)\(|\[[^\]]+\] = )`,
    "m",
  );
  return texts.some((text) => write.test(text));
}

/** The names a module hands to `keepSeed` or mentions in an `onFakeReset`. */
function registered(text: string): Set<string> {
  const held = [...text.matchAll(/^keepSeed\(([^)]*)\);$/gm), ...text.matchAll(/^onFakeReset\(\(\) => \{([\s\S]*?)^\}\);$/gm)];
  return new Set(held.flatMap((match) => match[1]!.match(/\b\w+\b/g) ?? []));
}

/** Each module's changeable values that no registration names; an exported `const` may be changed by any module. */
function unkept(files: Record<string, string>): string[] {
  const all = Object.values(files);
  return Object.entries(files).flatMap(([path, text]) => {
    const lets = [...text.matchAll(LET)].map((match) => match[1]!);
    const consts = [...text.matchAll(CONST)]
      .filter(([, exported, name]) => written(name!, exported ? all : [text]))
      .map(([, , name]) => name!);
    const kept = registered(text);
    return [...lets, ...consts].filter((name) => !kept.has(name)).map((name) => `${path}#${name}`);
  });
}

test("every value under fake/ a handler can change goes back to its seed", () => {
  const files = Object.fromEntries(sources(FAKE).map((path) => [relative(FAKE, path), readFileSync(path, "utf8")]));
  expect(Object.keys(files)).toContain("notes.ts");
  expect(unkept(files), "register it with keepSeed or onFakeReset (dev/fake-seed.ts)").toEqual([]);
});

test("the scan sees what it is meant to", () => {
  const files = {
    "a.ts": [
      "let picked: string | null = null;",
      "let open = false;",
      "let seq = 0;",
      "const TABLE = { up: 1 };",
      "const SEEDED: string[] = [];",
      "SEEDED.push('at load');",
      "const ADDED = new Set<string>();",
      "const EDITED: Record<string, string> = {};",
      "export const SHARED: string[] = [];",
      "const local: string[] = [];",
      "function handlers() {",
      "  ADDED.add('x');",
      "  EDITED['k'] = 'v';",
      "  open = true;",
      "}",
      "keepSeed(EDITED);",
      "onFakeReset(() => {",
      "  open = false;",
      "});",
    ].join("\n"),
    "b.ts": ["function handlers() {", "  (SHARED as unknown[]).push(1);", "  local.push(1);", "}"].join("\n"),
  };
  expect(unkept(files)).toEqual(["a.ts#picked", "a.ts#ADDED", "a.ts#SHARED"]);
});
