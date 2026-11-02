/**
 * The box's modes (#2424): the ⌘? overlay of filters the box narrows, the ⌘S
 * naming prompt, and the label in front of the box that names what it asks
 * for. Driven as a reader drives the launcher, through the keyboard, with
 * fake ports; the assertions are what is drawn and what was written.
 *
 * A file of its own beside `Launcher.test.svelte.ts` so the spec's parallel
 * tickets do not all append to one file.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchQuery, SearchResponse } from "../ipc";
import { toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

function response(raw: string): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [
      {
        kind: "ticket",
        label: "Ticket",
        plural: "Tickets",
        monogram: "TK",
        total: 1,
        hits: [
          {
            entity_id: "jira:PAY-231",
            kind: "ticket",
            source_id: "jira",
            updated_at: null,
            synced_at: "2026-08-25T11:56:00Z",
            path: null,
            state: null,
            title: "Retry failed SEPA payouts",
            rank: 1,
            snippet: [],
          },
        ],
      },
    ],
    total: 1,
    took_ms: 3,
    coverage: [],
  };
}

const HOME: LauncherHome = {
  smart_lists: [],
  recent: [],
  sources: [],
  pending_writes: 0,
  notes_shown: true,
} as unknown as LauncherHome;

const SAVED = {
  id: "sepa-mine",
  label: "SEPA mine",
  count: 1,
  changed: false,
  description: "#t sepa",
  saved: true,
  needs_attention: false,
};

/** Mounts the launcher open; `searched` collects every query sent. */
function open(ports: Record<string, unknown> = {}) {
  const searched: string[] = [];
  const createSmartList = vi.fn(async () => SAVED);
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: () => {},
      onclose: () => {},
      ontimer: undefined,
      ports: {
        search: async (query: SearchQuery) => {
          searched.push(query.raw);
          return response(query.raw);
        },
        launcherHome: async () => HOME,
        createSmartList,
        ...ports,
      },
    },
  });
  return { searched, createSmartList };
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
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  input().dispatchEvent(event);
  flushSync();
  return event;
}

/** ⌘? as a Mac keyboard sends it. */
function helpChord() {
  return press("?", { metaKey: true, shiftKey: true });
}

/** The label in front of the box, or `null` while the box is the search. */
function label(): string | null {
  return target.querySelector(".tf-ln-mode")?.textContent?.trim() ?? null;
}

/** The overlay's rows, as `token word`. */
function filters(): string[] {
  return [...target.querySelectorAll('[aria-label="Results"] .tf-ln-syn')].map(
    (row) =>
      `${row.querySelector(".tf-ln-tok")?.textContent?.trim()} ${row.querySelector(".tf-ln-s")?.textContent?.trim()}`,
  );
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
  toasts.items.splice(0);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  vi.useRealTimers();
  target.remove();
});

test("the empty box's placeholder reads only ⌘? for help, and no label stands in front of it", async () => {
  open();
  await settle();
  expect(input().placeholder).toBe("⌘? for help");
  expect(label()).toBeNull();
});

test("⌘? empties the box and shows the filters under a Filter label; ⌘? again returns to the search", async () => {
  open();
  await settle();
  await type("sepa");

  expect(helpChord().defaultPrevented).toBe(true);
  expect(input().value).toBe("");
  expect(label()).toBe("Filter");
  expect(filters()).toContain("#t tickets");
  expect(filters()).toContain("#l lists");
  expect(filters()).toContain("> go to");

  helpChord();
  expect(label()).toBeNull();
  expect(input().value).toBe("sepa");
});
