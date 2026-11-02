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

test("⌘⇧/, as most keyboards send ⌘?, opens the overlay too", async () => {
  open();
  await settle();
  press("/", { metaKey: true, shiftKey: true });
  expect(label()).toBe("Filter");
  // Plain ⌘/ is not this key: it is the section's search (#2425).
  press("Escape");
  press("/", { metaKey: true });
  expect(label()).toBeNull();
});

test("Esc, or Backspace on the empty box, returns to the search with its words intact", async () => {
  const { searched } = open();
  await settle();
  await type("#t sepa");

  helpChord();
  const esc = press("Escape");
  expect(esc.defaultPrevented, "Esc is the overlay's, not the dialog's").toBe(true);
  expect(label()).toBeNull();
  expect(input().value).toBe("#t sepa");
  expect(target.querySelector('[role="dialog"], .tf-launch'), "the dialog stays open").not.toBeNull();

  helpChord();
  await type("li");
  press("Backspace");
  expect(label(), "Backspace on a box with text edits it").toBe("Filter");
  await type("");
  press("Backspace");
  expect(label()).toBeNull();
  expect(input().value).toBe("#t sepa");
  expect(searched.at(-1), "the overlay's typing never reached the search").toBe("#t sepa");
});

test("typing narrows the filters, letters in order", async () => {
  open();
  await settle();
  helpChord();
  await type("rep");
  expect(filters()).toEqual(["#r repositories"]);
  await type("zzz");
  expect(filters()).toEqual([]);
});

test("Enter on a # filter merges its letter into the # token already there", async () => {
  const { searched } = open();
  await settle();
  await type("#r sepa");

  helpChord();
  await type("lists");
  press("Enter");
  await settle();
  expect(label()).toBeNull();
  expect(input().value).toBe("#rl sepa ");
  expect(searched.at(-1)).toBe("#rl sepa ");

  // A letter the token holds already changes nothing.
  helpChord();
  await type("repos");
  press("Enter");
  await settle();
  expect(input().value).toBe("#rl sepa ");
});

test("Enter on a filter with no # token there, or on another prefix, puts it first", async () => {
  open();
  await settle();
  await type("sepa");
  helpChord();
  await type("tick");
  press("Enter");
  await settle();
  expect(input().value).toBe("#t sepa ");

  helpChord();
  await type("mine");
  press("Enter");
  await settle();
  expect(input().value).toBe("@me #t sepa ");
});

test("the arrows walk the filters and a click picks one", async () => {
  open();
  await settle();
  await type("sepa");
  helpChord();
  press("ArrowDown");
  const second = filters()[1]!;
  expect(target.querySelector('[aria-label="Results"] .tf-ln-on .tf-ln-tok')?.textContent?.trim()).toBe(
    second.split(" ")[0],
  );
  target.querySelectorAll<HTMLButtonElement>('[aria-label="Results"] .tf-ln-syn')[0]!.click();
  await settle();
  expect(input().value).toBe("#t sepa ");
});

test("the keys the box answers sit at the bottom of the card", async () => {
  open();
  await settle();
  helpChord();
  const keys = target.querySelector('[aria-label="Keys"]');
  expect(keys).not.toBeNull();
  const named = [...keys!.querySelectorAll("kbd")].map((kbd) => kbd.textContent?.trim());
  expect(named).toEqual(["↵", "Tab", "⌘S", "⌘?", "⌘D", "Esc"]);
  // Last in the card: after every filter row.
  const list = target.querySelector('[aria-label="Results"]')!;
  expect(list.lastElementChild).toBe(keys);
});

test("⌘S asks for the list's name with the query prefilled and selected; Enter saves and the toast says so", async () => {
  const { createSmartList } = open();
  await settle();
  await type("#t sepa");

  expect(press("s", { metaKey: true }).defaultPrevented).toBe(true);
  await settle();
  expect(label()).toBe("Name");
  expect(input().value).toBe("#t sepa");
  expect(input().selectionStart).toBe(0);
  expect(input().selectionEnd).toBe("#t sepa".length);

  await type("SEPA mine");
  press("Enter");
  await settle();
  await settle();
  expect(createSmartList).toHaveBeenCalledWith("SEPA mine", "#t sepa");
  expect(label()).toBeNull();
  expect(input().value).toBe("#t sepa");
  expect(toasts.items.map((toast) => toast.text)).toContain("Saved as list SEPA mine");
});

test("Esc on the naming prompt keeps searching and saves nothing", async () => {
  const { createSmartList } = open();
  await settle();
  await type("sepa");
  press("s", { metaKey: true });
  await settle();
  await type("never mind");
  const esc = press("Escape");
  expect(esc.defaultPrevented).toBe(true);
  expect(label()).toBeNull();
  expect(input().value).toBe("sepa");
  expect(target.querySelector(".tf-launch"), "the dialog stays open").not.toBeNull();
  expect(createSmartList).not.toHaveBeenCalled();
});

test("a failed save raises no toast", async () => {
  open({
    createSmartList: async () => {
      throw { code: "internal", message: "the disk is full" };
    },
  });
  await settle();
  await type("sepa");
  press("s", { metaKey: true });
  await settle();
  press("Enter");
  await settle();
  await settle();
  expect(toasts.items).toEqual([]);
  expect(target.textContent).toContain("the disk is full");
});

test("⌘S on the empty box asks for nothing", async () => {
  open();
  await settle();
  press("s", { metaKey: true });
  await settle();
  expect(label()).toBeNull();
});

test("the footer's Save as list opens the same prompt", async () => {
  open();
  await settle();
  await type("sepa");
  target.querySelector<HTMLButtonElement>("button.tf-ln-save")!.click();
  await settle();
  expect(label()).toBe("Name");
  expect(input().value).toBe("sepa");
});

test("the label is its own element in front of the box, not part of its text", async () => {
  open();
  await settle();
  await type("sepa");
  helpChord();
  const mode = target.querySelector(".tf-ln-mode")!;
  expect(mode.closest("input")).toBeNull();
  expect(mode.querySelector("input")).toBeNull();
  // In front of the box, in document order.
  expect(mode.compareDocumentPosition(input()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(input().value, "the box holds none of the label").toBe("");
  // Backspace on the empty box leaves the mode rather than deleting the label.
  press("Backspace");
  expect(label()).toBeNull();
});

test("closing the launcher in a mode reopens it on the search, which ⌘K opens empty (#2425)", async () => {
  open();
  await settle();
  await type("sepa");

  for (const enter of [() => helpChord(), () => press("s", { metaKey: true })]) {
    enter();
    await settle();
    expect(label()).not.toBeNull();
    target.querySelector<HTMLButtonElement>("button.tf-ln-x")!.click();
    flushSync();
    expect(target.querySelector(".tf-launch")).toBeNull();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
    flushSync();
    await settle();
    expect(label(), "the mode ended with the close").toBeNull();
    expect(input().value, "⌘K opens on an empty box, never the last words (#2425)").toBe("");
    await type("sepa");
  }
});
