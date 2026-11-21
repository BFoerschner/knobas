/**
 * Favourites (#2428, spec #2419 stories 39 and 40): *Favourite* and
 * *Unfavourite* in the Tab sheet on an entity or a saved list, and the empty
 * box's one Favourites block above Recent, lists and entities together in the
 * order they were favourited. Driven through the keyboard with fake ports
 * that keep the favourites as the backend does; the assertions are what is
 * drawn and what was written.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FavouriteTarget, LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import type { EntityRow } from "../ipc/entity";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

const row = (entity_id: string, kind: string, title: string, extra: Partial<EntityRow> = {}): EntityRow =>
  ({
    entity_id,
    kind,
    source_id: "jira",
    title,
    updated_at: null,
    synced_at: "2026-08-25T11:56:00Z",
    path: null,
    state: null,
    cell: null,
    ...extra,
  }) as EntityRow;

const TICKET = row("jira:PAY-231", "ticket", "Retry failed SEPA payouts");
const PR = row("gitea:acme/svc#142", "pr", "Backoff", { source_id: "gitea", path: "acme/svc" });
/** A saved list as the board's block carries it: its `#l` hit's row. */
const LIST = row("sepa-payouts", "list", "SEPA payouts", { source_id: "list", path: "#t sepa" });

/** The backend's favourites, kept in order; `launcherHome` resolves them as `favourites::list` does. */
function backend(initial: FavouriteTarget[]) {
  const kept = [...initial];
  const resolve = (target: FavouriteTarget): EntityRow[] => {
    if ("list" in target) return target.list === LIST.entity_id ? [LIST] : [];
    return [TICKET, PR].filter((candidate) => candidate.entity_id === target.entity);
  };
  const same = (a: FavouriteTarget, b: FavouriteTarget) => JSON.stringify(a) === JSON.stringify(b);
  return {
    kept,
    home: async (): Promise<LauncherHome> =>
      ({ recent: [TICKET, PR], favourites: kept.flatMap(resolve), sources: [], pending_writes: 0, notes_shown: true }) as unknown as LauncherHome,
    add: vi.fn(async (target: FavouriteTarget) => {
      if (!kept.some((one) => same(one, target))) kept.push(target);
    }),
    remove: vi.fn(async (target: FavouriteTarget) => {
      const at = kept.findIndex((one) => same(one, target));
      if (at >= 0) kept.splice(at, 1);
    }),
  };
}

/** `list` answers the saved list as a hit; anything else nothing. */
function response(raw: string): SearchResponse {
  const word = raw.trim();
  const hits =
    word === "list"
      ? [{ ...LIST, rank: 1, snippet: [], excerpts: [] } as unknown as SearchHit]
      : [];
  return {
    interpreted: { text: word, prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
    groups: hits.length ? [{ kind: "list", label: "List", plural: "Lists", monogram: "LI", total: 1, hits }] : [],
    total: hits.length,
    took_ms: 1,
    coverage: [],
  } as SearchResponse;
}

function mountIt(initial: FavouriteTarget[] = []) {
  const store = backend(initial);
  const navigate = vi.fn();
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: navigate,
      onclose: () => {},
      ontimer: undefined,
      ports: {
        search: async (query: SearchQuery) => response(query.raw),
        launcherHome: store.home,
        addFavourite: store.add,
        removeFavourite: store.remove,
      },
      sheetPorts: { checkoutSettings: async () => ({ applications: [], default_application: null }) as never },
    },
  });
  flushSync();
  return { store, navigate };
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
  flushSync();
}

const input = () => target.querySelector("input")!;

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function narrow(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function press(key: string) {
  input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
}

/** The board as drawn: each heading, then the titles under it. */
function board(): string[] {
  return [...target.querySelectorAll(".tf-ln-h .tf-ln-lab, .tf-ln-res:not(.tf-ln-pinned .tf-ln-res) .tf-ln-t")].map(
    (el) => (el.classList.contains("tf-ln-lab") ? `# ${el.textContent?.trim()}` : (el.textContent?.trim() ?? "")),
  );
}

const sheetLabels = () =>
  [...(target.querySelector('[aria-label="Actions on the selected result"]')?.querySelectorAll(".tf-ln-nm") ?? [])].map((el) =>
    el.textContent?.trim(),
  );

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
  resetToasts();
});

test("the empty box draws one Favourites block, lists and entities in their order, then Recent", async () => {
  mountIt([{ entity: PR.entity_id }, { list: LIST.entity_id }, { entity: TICKET.entity_id }]);
  await settle();
  expect(board()).toEqual([
    "# Favourites",
    "#142 Backoff",
    "SEPA payouts",
    "Retry failed SEPA payouts",
    "# Recent",
    "Retry failed SEPA payouts",
    "#142 Backoff",
  ]);
});

test("no favourites, no Favourites heading", async () => {
  mountIt();
  await settle();
  expect(board()).toEqual(["# Recent", "Retry failed SEPA payouts", "#142 Backoff"]);
});

test("Favourite on a recent row writes it and the empty box redraws at once", async () => {
  const { store } = mountIt([{ list: LIST.entity_id }]);
  await settle();
  // The block's one row, then Recent's: the pull request is the third row.
  press("ArrowDown");
  press("ArrowDown");
  press("Tab");
  await settle();
  expect(sheetLabels()).toContain("Favourite");
  narrow("favourite");
  press("Enter");
  await settle();
  expect(store.add).toHaveBeenCalledWith({ entity: PR.entity_id });
  expect(toasts.items.at(-1)?.text).toBe("Favourited Backoff");
  expect(board()).toEqual(["# Favourites", "SEPA payouts", "#142 Backoff", "# Recent", "Retry failed SEPA payouts", "#142 Backoff"]);
});

test("Unfavourite on a favourite takes it out of the block, and the last one takes the heading", async () => {
  const { store } = mountIt([{ entity: TICKET.entity_id }]);
  await settle();
  press("Tab");
  await settle();
  expect(sheetLabels()).toContain("Unfavourite");
  expect(sheetLabels()).not.toContain("Favourite");
  narrow("unfav");
  press("Enter");
  await settle();
  expect(store.remove).toHaveBeenCalledWith({ entity: TICKET.entity_id });
  expect(toasts.items.at(-1)?.text).toBe("Unfavourited PAY-231");
  expect(board()).toEqual(["# Recent", "Retry failed SEPA payouts", "#142 Backoff"]);
});

test("a saved list found by its words is favourited as a list, and sits in the block", async () => {
  const { store } = mountIt();
  await settle();
  await type("list");
  press("Tab");
  await settle();
  expect(sheetLabels()).toEqual(["Apply", "Rename", "Delete", "Favourite", "Assign key…"]);
  narrow("fav");
  press("Enter");
  await settle();
  expect(store.add).toHaveBeenCalledWith({ list: LIST.entity_id });
  await type("");
  expect(board()).toEqual(["# Favourites", "SEPA payouts", "# Recent", "Retry failed SEPA payouts", "#142 Backoff"]);
  // Tab on it in the block offers Unfavourite among the list's own actions.
  press("Tab");
  await settle();
  expect(sheetLabels()).toEqual(["Apply", "Rename", "Delete", "Unfavourite", "Assign key…"]);
});

test("Enter on a favourite list applies its text; on a favourite entity it opens it", async () => {
  const { navigate } = mountIt([{ list: LIST.entity_id }, { entity: TICKET.entity_id }]);
  await settle();
  press("Enter");
  await settle();
  expect(input().value).toBe("#t sepa ");
  expect(navigate).not.toHaveBeenCalled();

  await type("");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(navigate).toHaveBeenCalledWith("#/ticket/jira%3APAY-231");
});
