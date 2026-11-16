/**
 * Between a key and its answer (2026-11-16,
 * `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`): the list keeps what it drew
 * until the answer to the box's text lands, a wait is said only once it has lasted, and a key that acts on a row
 * waits for the answer, so what a row of keys opens does not depend on how fast it was typed.
 *
 * Driven through the box with a search whose every answer the test lets through by hand, over a board that holds a
 * row: the row Enter would open if it did not wait.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import Launcher from "./Launcher.svelte";
import { createScope } from "./scope.svelte";
import { DEBOUNCE_MS, WAIT_SAID_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

function hit(raw: string, n: number): SearchHit {
  return {
    entity_id: `jira:${raw.toUpperCase()}-${n}`,
    kind: "ticket",
    source_id: "jira",
    updated_at: null,
    synced_at: "2026-08-25T11:56:00Z",
    path: null,
    state: null,
    title: `${raw} ticket ${n}`,
    rank: 1,
    snippet: [],
    excerpts: [],
  };
}

/** `rows` tickets named after what was typed. */
function response(raw: string, rows: number): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: rows === 0 ? [] : [{ kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: rows, hits: Array.from({ length: rows }, (_, n) => hit(raw, n)) }],
    total: rows,
    took_ms: 3,
    coverage: [],
  };
}

/** A board with one row: what the empty box draws, and what must never open for words. */
const HOME = {
  recent: [{ entity_id: "jira:OLD-1", kind: "ticket", source_id: "jira", title: "A recent ticket", updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: null }],
  favourites: [],
  sources: [],
  pending_writes: 0,
  notes_shown: true,
} as unknown as LauncherHome;

/** The address the launcher navigates to for a ticket. */
const ticket = (id: string) => `#/ticket/${encodeURIComponent(id)}`;

interface Asked {
  raw: string;
  answer: (rows?: number) => void;
  fail: () => void;
}

/** Mounts the launcher open, under *Titles*; each search waits in `asked` until the test answers it. */
function open(props: Record<string, unknown> = {}, launcherHome: () => Promise<LauncherHome> = async () => HOME) {
  const asked: Asked[] = [];
  const onnavigate = vi.fn();
  const onclose = vi.fn();
  const kept = new Map<string, string>([["knobas.launcher.scope", "titles"]]);
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      places: [],
      onnavigate,
      onclose,
      ontimer: undefined,
      scope: createScope(() => ({
        getItem: (key: string) => kept.get(key) ?? null,
        setItem: (key: string, value: string) => void kept.set(key, value),
        removeItem: (key: string) => void kept.delete(key),
      })),
      ports: {
        search: (query: SearchQuery) =>
          new Promise<SearchResponse>((resolve, reject) => {
            asked.push({
              raw: query.raw,
              answer: (rows = 3) => resolve(response(query.raw, rows)),
              fail: () => reject({ code: "internal", message: "index is rebuilding" }),
            });
          }),
        launcherHome,
      },
      ...props,
    },
  });
  flushSync();
  return { asked, onnavigate, onclose };
}

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
  flushSync();
}

const input = () => target.querySelector("input")!;

/** A key into the box, with no turn after it: its answer is owed. */
function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function press(key: string) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  input().dispatchEvent(event);
  flushSync();
  return event;
}

const list = () => target.querySelector<HTMLElement>('[role="listbox"][aria-label="Results"]')!;
const split = () => target.querySelector<HTMLElement>(".tf-ln-split")!;
const rows = () => [...list().querySelectorAll('[role="option"]')].map((row) => row.textContent?.replace(/\s+/g, " ").trim() ?? "");
const text = () => list().textContent ?? "";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  vi.useRealTimers();
  target.remove();
});

test("the first key keeps the board until its answer lands: no line stands in for it, and the list is the one it was", async () => {
  const { asked } = open();
  await settle();
  expect(text()).toContain("A recent ticket");
  const drawn = list();

  type("p");
  expect(asked.map((query) => query.raw), "asked at once").toEqual(["p"]);
  expect(text(), "the board stands").toContain("A recent ticket");
  expect(text()).not.toContain("Searching");
  expect(text()).not.toContain("Nothing matches");
  await settle();
  expect(text(), "for as long as the answer takes").toContain("A recent ticket");

  asked[0]!.answer();
  await settle();
  expect(text()).not.toContain("A recent ticket");
  expect(rows().some((row) => row.includes("p ticket 0"))).toBe(true);
  expect(list(), "the same list, redrawn in place").toBe(drawn);

  // A word that grows: the rows of the letter before stand through the pause between keys and through the read.
  type("pa");
  expect(asked.length, "the next key waits for the keys to pause").toBe(1);
  expect(rows().some((row) => row.includes("p ticket 0"))).toBe(true);
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
  expect(asked.map((query) => query.raw)).toEqual(["p", "pa"]);
  expect(rows().some((row) => row.includes("p ticket 0"))).toBe(true);
  expect(text()).not.toContain("Searching");
  asked[1]!.answer();
  await settle();
  expect(rows().some((row) => row.includes("pa ticket 0"))).toBe(true);
  expect(rows().some((row) => row.includes("p ticket 0"))).toBe(false);
});

test("rows kept past the wait's threshold are dimmed, with no line over them, and are themselves again when the answer lands", async () => {
  const { asked } = open();
  await settle();
  type("p");
  vi.advanceTimersByTime(WAIT_SAID_MS - 1);
  flushSync();
  expect(split().classList.contains("tf-wait-dim"), "a wait under the threshold is not said").toBe(false);
  vi.advanceTimersByTime(2);
  flushSync();
  expect(split().classList.contains("tf-wait-dim")).toBe(true);
  expect(text(), "the rows it kept, and no line in their place").toContain("A recent ticket");
  expect(text()).not.toContain("Searching");

  asked[0]!.answer();
  await settle();
  expect(split().classList.contains("tf-wait-dim")).toBe(false);
  expect(rows().some((row) => row.includes("p ticket 0"))).toBe(true);
});

test("an opening on words draws nothing until its answer lands, and says Searching once the wait has lasted", async () => {
  const { asked } = open({ query: "#t pay" });
  await settle();
  expect(asked.map((query) => query.raw)).toEqual(["#t pay"]);
  expect(rows(), "no row of the board, which the box is not on").toEqual([]);
  expect(text()).not.toContain("A recent ticket");
  expect(text(), "and no line for a wait this short").not.toContain("Searching");

  vi.advanceTimersByTime(WAIT_SAID_MS + 1);
  flushSync();
  expect(text()).toContain("Searching…");
  expect(split().classList.contains("tf-wait-dim"), "nothing was drawn, so nothing is dimmed").toBe(false);

  asked[0]!.answer();
  await settle();
  expect(text()).not.toContain("Searching");
  expect(rows().some((row) => row.includes("#t pay ticket 0"))).toBe(true);
});

test("the board's first read is waited on in silence, and named once the wait has lasted", async () => {
  open({}, () => new Promise<never>(() => {}));
  await settle();
  expect(text()).not.toContain("Reading the mirror");
  vi.advanceTimersByTime(WAIT_SAID_MS + 1);
  flushSync();
  expect(text()).toContain("Reading the mirror…");
});

test("Enter pressed before the answer lands opens the answer's first row, never the row the board had there", async () => {
  const { asked, onnavigate, onclose } = open();
  await settle();
  type("p");
  expect(press("Enter").defaultPrevented).toBe(true);
  await settle();
  expect(onnavigate, "the board's row is drawn and selected, and is not what was asked for").not.toHaveBeenCalled();
  expect(onclose).not.toHaveBeenCalled();

  asked[0]!.answer();
  await settle();
  expect(onnavigate).toHaveBeenCalledTimes(1);
  expect(onnavigate).toHaveBeenCalledWith(ticket("jira:P-0"));
  expect(onclose).toHaveBeenCalledTimes(1);
});

test("the arrows wait with it: ↓ ↓ Enter before the answer open its third row", async () => {
  const { asked, onnavigate } = open();
  await settle();
  type("p");
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(onnavigate).not.toHaveBeenCalled();
  asked[0]!.answer(5);
  await settle();
  expect(onnavigate).toHaveBeenCalledWith(ticket("jira:P-2"));
});

test("a waiting Enter asks at once what waited for the keys to pause", async () => {
  const { asked, onnavigate } = open();
  await settle();
  type("p");
  asked[0]!.answer();
  await settle();
  // Typed within the pause of the key before: its query waits.
  type("pa");
  expect(asked.length).toBe(1);
  press("Enter");
  expect(asked.map((query) => query.raw), "Enter says the typing is done").toEqual(["p", "pa"]);
  expect(onnavigate, "and the rows of the letter before are not what it opens").not.toHaveBeenCalled();
  asked[1]!.answer();
  await settle();
  expect(onnavigate).toHaveBeenCalledWith(ticket("jira:PA-0"));
});

test("typing on lets a waiting Enter go: nothing opens for a text the box no longer holds", async () => {
  const { asked, onnavigate, onclose } = open();
  await settle();
  type("p");
  press("Enter");
  type("pa");
  asked[0]!.answer();
  await settle();
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
  asked.at(-1)!.answer();
  await settle();
  expect(asked.at(-1)!.raw).toBe("pa");
  expect(onnavigate).not.toHaveBeenCalled();
  expect(onclose).not.toHaveBeenCalled();
  expect(rows().some((row) => row.includes("pa ticket 0"))).toBe(true);
});

test("Esc lets a waiting Enter go, and takes the words", async () => {
  const { asked, onnavigate, onclose } = open();
  await settle();
  type("p");
  press("Enter");
  press("Escape");
  expect(input().value).toBe("");
  asked[0]!.answer();
  await settle();
  expect(onnavigate).not.toHaveBeenCalled();
  expect(onclose).not.toHaveBeenCalled();
  expect(text(), "the box is empty: the board").toContain("A recent ticket");
});

test("a search that fails opens nothing for the keys that waited, and one that finds nothing has no row to open", async () => {
  const { asked, onnavigate, onclose } = open();
  await settle();
  type("p");
  press("Enter");
  asked[0]!.fail();
  await settle();
  expect(onnavigate).not.toHaveBeenCalled();
  expect(target.textContent).toContain("index is rebuilding");

  type("zz");
  vi.advanceTimersByTime(DEBOUNCE_MS);
  press("Enter");
  asked.at(-1)!.answer(0);
  await settle();
  expect(onnavigate).not.toHaveBeenCalled();
  expect(onclose).not.toHaveBeenCalled();
  expect(text()).toContain("Nothing matches");
});

test("a click opens the row it was aimed at, at once, whatever the box is waiting for", async () => {
  const { onnavigate } = open();
  await settle();
  type("p");
  const row = [...list().querySelectorAll<HTMLElement>('[role="option"]')].find((option) => option.textContent?.includes("A recent ticket"))!;
  row.click();
  flushSync();
  expect(onnavigate).toHaveBeenCalledWith(ticket("jira:OLD-1"));
});

test("with the answer on screen Enter opens at once, as it did", async () => {
  const { asked, onnavigate } = open();
  await settle();
  type("p");
  asked[0]!.answer();
  await settle();
  press("ArrowDown");
  press("Enter");
  expect(onnavigate).toHaveBeenCalledWith(ticket("jira:P-1"));
});
