/**
 * *Everything* with a preview (#2431): with Everything on the dialog grows
 * and splits, results at the left and at the right the selected row's
 * excerpts, the words marked, following the arrow keys after a short pause;
 * a click on an excerpt opens the row. The preview is the hit's own
 * `excerpts`: no port is asked for it. With Titles on the dialog is the one
 * list at its usual size.
 *
 * Driven through props and the keyboard with fake ports, as
 * `Launcher.overlay.test.svelte.ts` drives the box's modes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import Launcher from "./Launcher.svelte";
import { PREVIEW_DEBOUNCE_MS } from "./Preview.svelte";
import { createScope } from "./scope.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

function hit(entity_id: string, kind: string, title: string, excerpts: SearchHit["excerpts"]): SearchHit {
  return {
    entity_id,
    kind,
    source_id: entity_id.split(":")[0]!,
    updated_at: null,
    synced_at: "2026-08-25T11:56:00Z",
    path: null,
    state: null,
    title,
    rank: 1,
    snippet: [],
    excerpts,
  };
}

const PAGE = hit("confluence:7", "page", "Retry runbook", [
  {
    place: "Backoff policy",
    segments: [
      { text: "Wait, then ", hit: false },
      { text: "retry", hit: true },
      { text: " again.", hit: false },
    ],
  },
  { place: "Comment by Priya", segments: [{ text: "Cap the ", hit: false }, { text: "retry", hit: true }] },
]);
const PIPELINE = hit("teamcity:Deploy", "pipeline", "Deploy payouts", [
  { place: "line 382", segments: [{ text: "connection refused, ", hit: false }, { text: "retry", hit: true }] },
]);
const TITLED = hit("jira:PAY-1", "ticket", "Retry everything", []);

function response(raw: string): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [
      { kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: 1, hits: [TITLED] },
      { kind: "pipeline", label: "Pipeline", plural: "Pipelines", monogram: "PL", total: 1, hits: [PIPELINE] },
      { kind: "page", label: "Page", plural: "Pages", monogram: "PG", total: 1, hits: [PAGE] },
    ],
    total: 3,
    took_ms: 3,
    coverage: [],
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

function storage(value: string | null) {
  const kept = new Map<string, string>();
  if (value !== null) kept.set("knobas.launcher.scope", value);
  return {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, v: string) => void kept.set(key, v),
    removeItem: (key: string) => void kept.delete(key),
  };
}

/** Mounts the launcher open under `mode`; answers what each port was asked. */
function open(mode: "everything" | "titles") {
  const kept = storage(mode);
  const searched: SearchQuery[] = [];
  const onnavigate = vi.fn();
  const entityView = vi.fn();
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate,
      onclose: () => {},
      ontimer: undefined,
      scope: createScope(() => kept),
      ports: {
        search: async (query: SearchQuery) => (searched.push(query), response(query.raw)),
        launcherHome: async () => HOME,
        entityView,
      },
    },
  });
  return { searched, onnavigate, entityView };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

function input(): HTMLInputElement {
  return target.querySelector("input")!;
}

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function press(key: string, options: KeyboardEventInit = {}) {
  input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options }));
  flushSync();
}

/** Past the preview's pause. */
async function paused() {
  vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS);
  await settle();
}

function dialog(): HTMLElement {
  return target.querySelector(".tf-dialog")!;
}

function pane(): HTMLElement | null {
  return target.querySelector('[aria-label="Where it matches"]');
}

/** The pane's excerpts as `place: text`, the marked words in brackets. */
function excerpts(): string[] {
  return [...(pane()?.querySelectorAll(".tf-ln-ex") ?? [])].map((row) => {
    const place = row.querySelector(".tf-ln-ex-at")?.textContent?.trim();
    const text = [...row.querySelector(".tf-ln-ex-t")!.childNodes]
      .map((node) => (node.nodeName === "MARK" ? `[${node.textContent}]` : node.textContent))
      .join("");
    return place ? `${place}: ${text}` : text;
  });
}

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

test("with Everything on the dialog grows and shows the pane beside the results", async () => {
  open("everything");
  await settle();
  await type("retry");
  expect(dialog().classList.contains("tf-launch-deep")).toBe(true);
  expect(pane()).not.toBeNull();
  expect(target.querySelector('[aria-label="Results"]')).not.toBeNull();
});

test("with Titles on it is the single list at the usual size", async () => {
  const { searched } = open("titles");
  await settle();
  await type("retry");
  expect(dialog().classList.contains("tf-launch-deep")).toBe(false);
  expect(pane()).toBeNull();
  expect(searched.at(-1)).toMatchObject({ titles: true });
});

test("the pane waits for the selection to settle, then draws its excerpts with their places and marks", async () => {
  open("everything");
  await settle();
  await type("retry");
  // The first row is a hit by its title alone.
  await paused();
  expect(pane()?.textContent).toContain("Retry everything");
  // #2481: the same row may be a hit by a stem or a key, so no word about where.
  expect(pane()?.textContent).toContain("No passage to quote.");
  expect(pane()?.textContent).not.toContain("title holds");

  press("ArrowDown");
  // Still the row before, until the pause is over.
  expect(pane()?.textContent).toContain("Retry everything");
  vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS - 1);
  flushSync();
  expect(pane()?.textContent).toContain("Retry everything");
  await paused();
  expect(excerpts()).toEqual(["line 382: connection refused, [retry]"]);

  press("ArrowDown");
  await paused();
  expect(excerpts()).toEqual(["Backoff policy: Wait, then [retry] again.", "Comment by Priya: Cap the [retry]"]);
});

test("a click on an excerpt opens the row, and the preview made no second read", async () => {
  const { onnavigate, entityView, searched } = open("everything");
  await settle();
  await type("retry");
  press("ArrowDown");
  press("ArrowDown");
  await paused();
  const asked = searched.length;

  (pane()!.querySelectorAll(".tf-ln-ex")[1] as HTMLElement).click();
  flushSync();
  expect(onnavigate).toHaveBeenCalledTimes(1);
  expect(String(onnavigate.mock.calls[0]![0])).toContain("confluence");
  expect(entityView).not.toHaveBeenCalled();
  expect(searched.length).toBe(asked);
});

test("⌘D to Titles takes the pane away and the dialog back to its size", async () => {
  open("everything");
  await settle();
  await type("retry");
  await paused();
  expect(pane()).not.toBeNull();
  press("d", { metaKey: true });
  await settle();
  expect(pane()).toBeNull();
  expect(dialog().classList.contains("tf-launch-deep")).toBe(false);
});
