/**
 * An answer draws what the list shows first (the one-letter pass, 2026-11-15): the frame an answer lands in draws its
 * first {@link FIRST_FRAME_ROWS} rows and the frame after the rest, below them, so the first key into an empty box
 * does not build forty rows before anything is seen. Nothing drawn moves or goes, a kind left for the next frame
 * draws its heading with its rows, and a list that already drew whole draws the next answer whole at once.
 *
 * Driven through the box with fake ports. The frames are the test's: a frame comes when {@link nextFrame} says so,
 * so what the first one drew is read however long the machine took to get there.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import Launcher from "./Launcher.svelte";
import { FIRST_FRAME_ROWS } from "./Results.svelte";
import { createScope } from "./scope.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

const KINDS = [
  ["ticket", "Tickets"],
  ["repo", "Repositories"],
  ["pr", "Pull requests"],
  ["page", "Pages"],
] as const;

function hit(kind: string, n: number, raw: string): SearchHit {
  return {
    entity_id: `src:${raw}-${kind}-${n}`,
    kind,
    source_id: "src",
    updated_at: null,
    synced_at: "2026-08-25T11:56:00Z",
    path: null,
    state: null,
    title: `${raw} ${kind} ${n}`,
    rank: 1,
    snippet: [],
    excerpts: [],
  };
}

/** `each` rows of every kind, named after what was typed. */
function response(raw: string, each: number): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: KINDS.map(([kind, plural]) => ({
      kind,
      label: kind,
      plural,
      monogram: kind.slice(0, 2).toUpperCase(),
      total: each,
      hits: Array.from({ length: each }, (_, n) => hit(kind, n, raw)),
    })),
    total: each * KINDS.length,
    took_ms: 3,
    coverage: [],
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

function storage(value: string) {
  const kept = new Map<string, string>([["knobas.launcher.scope", value]]);
  return {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, v: string) => void kept.set(key, v),
    removeItem: (key: string) => void kept.delete(key),
  };
}

/** Mounts the launcher open under `mode`, each search answered with `each(raw)` rows a kind. */
function open(mode: "everything" | "titles", each: (raw: string) => number) {
  const kept = storage(mode);
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: () => {},
      onclose: () => {},
      ontimer: undefined,
      scope: createScope(() => kept),
      ports: {
        search: async (query: SearchQuery) => response(query.raw, each(query.raw)),
        launcherHome: async () => HOME,
        entityView: async () => {
          throw new Error("no view is read");
        },
      },
    },
  });
}

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

/** The callbacks waiting for a frame, by the handle each was given. */
const frames = new Map<number, FrameRequestCallback>();
let handles = 0;

/** The frame after: what an answer draws below its first rows. */
async function nextFrame() {
  const waiting = [...frames.values()];
  frames.clear();
  for (const callback of waiting) callback(performance.now());
  await settle();
}

/**
 * Types `text` into the box once the box has settled, so the key is asked at once, and waits for its answer and no
 * frame longer.
 */
async function type(text: string) {
  await new Promise((resolve) => setTimeout(resolve, DEBOUNCE_MS + 30));
  const box = target.querySelector("input")!;
  box.value = text;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
}

const rows = () => [...target.querySelectorAll<HTMLElement>(".tf-ln-list [role=option]")];
const titles = () => rows().map((row) => row.textContent?.replace(/\s+/g, " ").trim() ?? "");
const headings = () => [...target.querySelectorAll(".tf-ln-list .tf-ln-h")].map((h) => h.textContent?.trim());

beforeEach(() => {
  frames.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    handles += 1;
    frames.set(handles, callback);
    return handles;
  });
  vi.stubGlobal("cancelAnimationFrame", (handle: number) => void frames.delete(handle));
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  vi.unstubAllGlobals();
});

for (const mode of ["everything", "titles"] as const) {
  test(`the first answer draws its first rows in the frame it lands and the rest the frame after, under ${mode}`, async () => {
    open(mode, () => 10);
    await settle();
    await type("p");

    // Two kinds whole and four rows of the third: the fourth kind's heading waits with its rows.
    expect(rows()).toHaveLength(FIRST_FRAME_ROWS);
    expect(headings()).toEqual(["Tickets", "Repositories", "Pull requests"]);
    const first = rows();
    const firstTitles = titles();

    await nextFrame();
    expect(rows()).toHaveLength(40);
    expect(headings()).toEqual(["Tickets", "Repositories", "Pull requests", "Pages"]);
    // Every row of the first frame is the element it was, where it was: the rest came in below.
    expect(first.every((row) => row.isConnected)).toBe(true);
    expect(rows().slice(0, FIRST_FRAME_ROWS)).toEqual(first);
    expect(titles().slice(0, FIRST_FRAME_ROWS)).toEqual(firstTitles);

    // The next answer replaces a list drawn whole, so it draws whole at once: no row goes for a frame.
    await type("pa");
    expect(rows()).toHaveLength(40);
    expect(titles()[39]).toContain("pa page 9");
  });
}

test("an answer the first frame holds whole draws at once, and a longer one after it draws its first rows first", async () => {
  open("everything", (raw) => (raw === "p" ? 5 : 10));
  await settle();
  await type("p");
  expect(rows()).toHaveLength(20);
  expect(headings()).toEqual(["Tickets", "Repositories", "Pull requests", "Pages"]);
  await nextFrame();
  expect(rows()).toHaveLength(20);

  // Twenty rows drew, fewer than a first frame's: the longer answer grows the list, first rows first.
  await type("pa");
  expect(rows()).toHaveLength(FIRST_FRAME_ROWS);
  await nextFrame();
  expect(rows()).toHaveLength(40);
});
