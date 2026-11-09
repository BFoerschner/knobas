/**
 * The launcher as a reader meets it: the overlay, its keyboard, and what a row
 * actually renders.
 *
 * The pieces that are pure — debounce, sequencing, selection — are pinned in
 * `session.test.svelte.ts`. What is here is everything that only exists once
 * there is a DOM: that source text stays text, that the group order on screen
 * is the backend's, that `Esc` unwinds one rung at a time and does not reach
 * the shell, and that `Tab` steals nothing.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  FilterSuggestions,
  LauncherHome,
  SearchQuery,
  SearchResponse,
  Excerpt,
  Segment,
} from "../ipc";
import type { InboxItem, StateChip } from "../ipc/entity";
import type { KindStateColours, StateWordColour } from "../ipc/sources";
import { EVENTS } from "../ipc";
import { toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { DEBOUNCE_MS } from "./session.svelte";
import { createScope } from "./scope.svelte";

const NOW = new Date("2026-08-25T12:00:00Z");

function hit(over: {
  entity_id: string;
  kind: string;
  title: string;
  source_id?: string;
  snippet?: Segment[];
  excerpts?: Excerpt[];
  synced_at?: string;
  path?: string | null;
  state?: StateChip | null;
  cell?: string;
}) {
  return {
    entity_id: over.entity_id,
    kind: over.kind,
    source_id: over.source_id ?? "jira",
    updated_at: null,
    synced_at: over.synced_at ?? "2026-08-25T11:56:00Z",
    path: over.path ?? null,
    state: over.state ?? null,
    ...(over.cell === undefined ? {} : { cell: over.cell }),
    title: over.title,
    rank: 1,
    snippet: over.snippet ?? [],
    excerpts: over.excerpts ?? [],
  };
}

/** Three groups in the engine's fixed order, so the DOM order means something. */
function response(over: Partial<SearchResponse> = {}): SearchResponse {
  return {
    interpreted: {
      text: "sepa",
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [
      {
        kind: "ticket",
        label: "Ticket",
        plural: "Tickets",
        monogram: "TK",
        total: 42,
        hits: [
          hit({
            entity_id: "jira:PAY-231",
            kind: "ticket",
            title: "Retry failed SEPA payouts",
            snippet: [
              { text: "the ", hit: false },
              { text: "SEPA", hit: true },
              { text: " batch", hit: false },
            ],
          }),
        ],
      },
      {
        kind: "pr",
        label: "Pull request",
        plural: "Pull requests",
        monogram: "PR",
        total: 1,
        hits: [hit({ entity_id: "gitea:acme/svc#142", kind: "pr", title: "Backoff", source_id: "gitea" })],
      },
      {
        kind: "build",
        label: "Build",
        plural: "Builds",
        monogram: "BU",
        total: 1,
        hits: [hit({ entity_id: "teamcity:1187", kind: "build", title: "#1187", source_id: "teamcity" })],
      },
    ],
    total: 44,
    took_ms: 7,
    coverage: [],
    ...over,
  };
}

const HOME: LauncherHome = {
  recent: [
    {
      entity_id: "jira:PAY-240",
      kind: "ticket",
      source_id: "jira",
      title: "Reconcile mandates",
      updated_at: null,
      synced_at: "2026-08-25T11:58:00Z",
      path: null,
      state: null,
    },
  ],
  favourites: [],
  sources: [],
  pending_writes: 0,
  notes_shown: true,
};

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function open(props: Record<string, unknown> = {}) {
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: () => {},
      onclose: () => {},
      // Spelled out rather than omitted: `ontimer` is required so a shell
      // cannot drop it silently, and a fixture with no timer says so.
      ontimer: undefined,
      now: NOW,
      ports: {
        search: async () => response(),
        launcherHome: async () => HOME,
      },
      ...props,
    },
  });
  flushSync();
  return app;
}

/**
 * Let the injected promises settle and the DOM catch up.
 *
 * **Three of what, and why that is not the hazard #86 was.** The count here is
 * a count of **microtask** turns, and a microtask turn is a step down a
 * promise chain rather than an interval of time: every promise these tests
 * await is made by an already-resolved value — an injected `ports.search`, a
 * `vi.mock` factory — so three turns is either always enough or never enough,
 * and a loaded machine cannot change which. `App.test.svelte.ts` had a fixed
 * budget of **macrotask** ticks waiting on a real `await import()`, and that
 * is a time budget wearing a count's clothes: how long module resolution takes
 * is a property of the machine, so the budget lost the race 2 runs in 6 under
 * a parallel fan-out (#86). Nothing in this file dynamically imports anything.
 *
 * The 90 ms debounce is not a counterexample: `setTimeout` is fake in this
 * file, and {@link debounced} moves its clock past the debounce, so no test
 * waits on the wall clock (#2048). The same reading applies to the other
 * microtask-count `settle()` helpers in this repo (`SourcesView`,
 * `Diagnostics`, `AddSource`, `FirstRun`): none of the components they drive
 * dynamically imports anything either.
 */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

/**
 * Past the session's debounce, and the search it sends settled. The debounce
 * is the only timer the launcher arms, so the fake clock moving 90 ms on is
 * the whole wait (#2048; it was a real 120 ms sleep).
 */
async function debounced() {
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function press(key: string, options: KeyboardEventInit = {}) {
  const input = target.querySelector("input");
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...options,
  });
  input?.dispatchEvent(event);
  flushSync();
  return event;
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
  // `recordScrolls()` stubs it; jsdom has none of its own.
  Reflect.deleteProperty(Element.prototype, "scrollIntoView");
});

test("the box takes focus and keeps it while the overlay is open", async () => {
  open();
  await settle();
  const input = target.querySelector("input");
  expect(document.activeElement).toBe(input);
});

/**
 * Roadmap §4 gotcha 7, as a rendered fact rather than as a source scan.
 *
 * `house-rules.test.ts` refuses `{@html}` anywhere in the app, which is the
 * *rule*. This is the *thing*: a title and a snippet segment carrying markup
 * come out of the DOM as characters, with no element behind them. A row that
 * reached for `innerHTML` in some other way — a `setHTML`, an SVG
 * `foreignObject`, a future refactor — would pass the source scan and fail
 * here.
 */
test("markup in source text is rendered as characters, not as elements", async () => {
  const nasty = '<script>alert(1)</script><img src=x onerror="alert(2)">';
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          groups: [
            {
              kind: "ticket",
              label: "Ticket",
              plural: "Tickets",
              monogram: "TK",
              total: 1,
              hits: [
                hit({
                  entity_id: "jira:XSS-1",
                  kind: "ticket",
                  title: nasty,
                  snippet: [
                    { text: '<b onclick="steal()">', hit: false },
                    { text: nasty, hit: true },
                  ],
                }),
              ],
            },
          ],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "xss";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const row = target.querySelector(".tf-ln-res");
  expect(row, "the hit rendered").not.toBeNull();
  expect(row!.textContent).toContain("<script>alert(1)</script>");
  expect(row!.textContent).toContain('onerror="alert(2)"');
  // Nothing was parsed: no element the text merely *looks* like exists.
  expect(target.querySelectorAll("script")).toHaveLength(0);
  expect(target.querySelectorAll("img")).toHaveLength(0);
  expect(target.querySelectorAll("b")).toHaveLength(0);
});

test("groups render in the engine's order", async () => {
  open();
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const headings = [...target.querySelectorAll(".tf-ln-lab")].map((el) => el.textContent?.trim());
  expect(headings).toEqual(["Tickets", "Pull requests", "Builds"]);
});

/**
 * The box before the board's own read lands.
 *
 * Every `open()` in this file renders this frame and no other test looks at
 * it — they all assert after `settle()`, by which time the board has arrived
 * and the line is gone — so it is pinned here or nowhere: `CONTEXT.md`'s
 * **Mirror** entry lists *index* under `_Avoid_`, and this placeholder used
 * the forbidden word from M1 until #518. Hence the assertion before
 * `settle()`, and a `launcherHome` that never resolves. The footnote below it
 * names the store too, and has its own test.
 */
test("the box says which store it is reading while the board is in flight", () => {
  open({ ports: { launcherHome: () => new Promise<never>(() => {}) } });
  expect(target.textContent).toContain("Reading the mirror\u2026");
});

test("Enter on a row navigates to its address and closes the overlay", async () => {
  const onnavigate = vi.fn();
  const onclose = vi.fn();
  open({ onnavigate, onclose });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("ArrowDown");
  press("Enter");
  await settle();

  // The second row is the PR, and its key carries a `#` and a `/` — both of
  // which have to survive the address or the PR page opens on nothing.
  expect(onnavigate).toHaveBeenCalledWith("#/pr/gitea%3Aacme%2Fsvc%23142");
  expect(onclose).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-launch")).toBeNull();
});

/** A pointer event at a place on the screen, on `el`. */
function pointerAt(el: Element, x: number, y: number) {
  el.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, screenX: x, screenY: y }));
  el.dispatchEvent(new MouseEvent("mouseenter", { screenX: x, screenY: y }));
  flushSync();
}

test("a row under a resting pointer takes no selection, and a real move does; new words start on the first row (F-628)", async () => {
  const onnavigate = vi.fn();
  open({ onnavigate });
  await settle();
  const box = target.querySelector("input")!;
  box.value = "sepa";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();
  const rows = () => [...target.querySelectorAll<HTMLElement>(".tf-ln-res")];
  const selected = () => rows().findIndex((row) => row.getAttribute("aria-selected") === "true");

  // Where the pointer already was, then what a layout reports with it still there: the same place, twice.
  pointerAt(rows()[1]!, 300, 330);
  pointerAt(rows()[1]!, 300, 330);
  expect(selected(), "a pointer that did not move").toBe(0);

  pointerAt(rows()[1]!, 300, 334);
  expect(selected(), "a pointer that moved").toBe(1);

  box.value = "sep";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();
  expect(selected(), "new words").toBe(0);
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith(expect.stringContaining("PAY-231"));
});

test("a press on the Titles chip leaves focus in the box (F-631)", async () => {
  open();
  await settle();
  const chip = target.querySelector<HTMLButtonElement>(".tf-ln-scope button")!;
  chip.focus();
  chip.click();
  flushSync();
  await settle();
  expect(document.activeElement).toBe(target.querySelector("input"));
  target.querySelector<HTMLButtonElement>(".tf-ln-scope button")!.click();
  flushSync();
});

test("Esc clears the query first and only then closes", async () => {
  const onclose = vi.fn();
  const shell = vi.fn();
  window.addEventListener("keydown", shell);
  open({ onclose });
  await settle();

  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();

  press("Escape");
  await settle();
  expect(target.querySelector("input")!.value).toBe("");
  expect(onclose).not.toHaveBeenCalled();
  // Rung 1 of the shell's own ladder: the key must not also unwind the room
  // behind the overlay.
  expect(shell).not.toHaveBeenCalled();

  press("Escape");
  await settle();
  expect(onclose).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-launch")).toBeNull();

  window.removeEventListener("keydown", shell);
});

test("Tab steals no focus, and with no job chosen the sheet has no Link row", async () => {
  open();
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const input = target.querySelector("input");
  const event = press("Tab");
  expect(event.defaultPrevented, "Tab is the action sheet, not a focus move").toBe(true);
  expect(document.activeElement).toBe(input);
  const sheet = target.querySelector('[aria-label="Actions on the selected result"]');
  expect(sheet?.textContent).toContain("Work on this");
  expect(sheet?.textContent, "a link needs a job at its other end").not.toContain("Link to");
});

/**
 * #1422: the launcher draws on `TfDialog`, whose `Tab` trap would send a
 * Shift+Tab from its first field to its last control. The box takes `Tab`
 * for the chain, so the trap leaves it alone and focus stays in the box.
 */
test("on the shell's dialog, Shift+Tab in the box keeps focus there", async () => {
  openSaved();
  await settle();
  const input = target.querySelector("input");
  expect(document.activeElement).toBe(input);
  expect(target.querySelectorAll('[aria-label="Results"] [role="option"]').length).toBeGreaterThan(1);
  const event = press("Tab", { shiftKey: true });
  expect(event.defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(input);
});

test("the overlay is the shell's dialog, and a click on the dim closes it", async () => {
  const onclose = vi.fn();
  open({ onclose });
  await settle();
  const dialog = target.querySelector<HTMLElement>('[role="dialog"]');
  expect(dialog?.classList.contains("tf-dialog")).toBe(true);
  expect(dialog?.classList.contains("tf-launch")).toBe(true);
  dialog!.parentElement!.click();
  flushSync();
  expect(onclose).toHaveBeenCalledTimes(1);
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

/** The open entity, as the shell names it to the launcher. */
const OPEN = { entityId: "jira:PAY-999", label: "PAY-999" };

/** The sheet belongs to the row selected when Tab opened it. */
test("leaving the sheet and moving the selection opens it on the next row", async () => {
  open({ openEntity: OPEN });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  expect(target.querySelector(".tf-ln-pinned .tf-ln-t")?.textContent).toBe("Retry failed SEPA payouts");
  press("Escape");
  press("ArrowDown");
  press("Tab");
  expect(target.querySelector(".tf-ln-pinned .tf-ln-t")?.textContent).toBe("#142 Backoff");
});

/** Escape leaves the sheet first, then clears the query, and the shell's ladder never sees it. */
test("Escape leaves the sheet before it clears the query", async () => {
  const reachedTheShell: string[] = [];
  const shell = (event: KeyboardEvent) => reachedTheShell.push(event.key);
  window.addEventListener("keydown", shell);
  open({ openEntity: OPEN });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  press("Escape");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  expect(target.querySelector("input")!.value, "the query survives the first rung").toBe("sepa");

  press("Escape");
  expect(target.querySelector("input")!.value).toBe("");
  expect(reachedTheShell, "one keystroke must not unwind two ladders").not.toContain("Escape");

  window.removeEventListener("keydown", shell);
});

test("⌘K opens the overlay from anywhere, and opening twice is not closing", async () => {
  app = mount(Launcher, {
    target,
    props: {
      open: false,
      onnavigate: () => {},
      onclose: () => {},
      ontimer: undefined,
      ports: { search: async () => response(), launcherHome: async () => HOME },
    },
  });
  flushSync();
  expect(target.querySelector(".tf-launch")).toBeNull();

  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }),
  );
  flushSync();
  await settle();
  expect(target.querySelector(".tf-launch")).not.toBeNull();

  // The shell binds ⌘K too. Two "open" writes on one keystroke must converge,
  // which a toggle would not.
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true, cancelable: true }),
  );
  flushSync();
  await settle();
  expect(target.querySelector(".tf-launch")).not.toBeNull();
});

test("an unknown token is greyed out with a reason rather than silently ignored", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          interpreted: {
            text: "",
            prefix: null,
            filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
            unknown_tokens: ["env:prod", "source:cf"],
            left_out_tokens: [],
          },
          groups: [],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "env:prod source:cf";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const greyed = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-off")].map((el) => el.textContent?.trim());
  expect(greyed).toEqual([
    "env:prod — the estate arrives with assets",
    "source:cf — no source matches source:cf",
  ]);
});

test("a token the launcher left out says so, in its own words and not an unknown token's", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          interpreted: {
            text: "",
            prefix: null,
            filters: { sources: [], kinds: ["build"], updated_within_days: null, mine: false, authors: [] },
            unknown_tokens: [],
            left_out_tokens: ["kind:build", "type:commit", "under:Old-Repo"],
          },
          groups: [],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "kind:build type:commit under:Old-Repo";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const greyed = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-off")].map((el) => el.textContent?.trim());
  expect(greyed).toEqual([
    "kind:build — the launcher leaves this kind out",
    "type:commit — the launcher leaves this kind out",
    "under:Old-Repo — the launcher leaves out everything beneath it",
  ]);
});

/* ── #2420: `#` letters that combine, the key a word, `/` gone ──────── */

test("a combined # filter is sent as typed and draws one chip per letter", async () => {
  const sent: string[] = [];
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => {
        sent.push(query.raw);
        return response({
          interpreted: {
            text: "sepa",
            prefix: null,
            filters: { sources: [], kinds: ["repo", "list"], updated_within_days: null, mine: false, authors: [] },
            unknown_tokens: [],
            left_out_tokens: [],
          },
        });
      },
    },
  });
  await settle();
  typeInBox("#rl sepa");
  await debounced();

  // The backend parses (ruling P2): the box's text goes over exactly as typed.
  expect(sent.at(-1)).toBe("#rl sepa");
  const chips = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set")].map((el) => el.textContent?.trim());
  // The words the `?` card gives each letter, one chip per kind the answer filtered on.
  expect(chips).toEqual(["Repositories", "Lists"]);
});

test("a key typed as a word finds its ticket and draws no filter chip", async () => {
  const sent: string[] = [];
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => {
        sent.push(query.raw);
        return response({
          interpreted: {
            text: "pay-231",
            prefix: null,
            filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
            unknown_tokens: [],
            left_out_tokens: [],
          },
        });
      },
    },
  });
  await settle();
  typeInBox("pay-231");
  await debounced();

  expect(sent.at(-1)).toBe("pay-231");
  expect(target.querySelectorAll(".tf-ln-fchip")).toHaveLength(0);
  expect(target.textContent).toContain("Retry failed SEPA payouts");
});

test("an empty answer under titles only offers Search everything, which searches every field", async () => {
  open({ ports: { launcherHome: async () => HOME, search: async () => response({ groups: [] }) } });
  await settle();
  await search("zzzqqq");
  expect(target.textContent).toContain("Nothing matches");
  const widen = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-ln-opt")].find((b) => b.textContent?.includes("Search everything"));
  expect(widen()).toBeDefined();
  widen()!.click();
  await debounced();
  // Every field is searched now, so there is nothing wider to offer.
  expect(widen()).toBeUndefined();
  expect(target.textContent).toContain("Nothing matches");
});

test("the placeholder says what the box does, with no grammar in it, and ⌘? is among the keys at the foot", async () => {
  open();
  await settle();
  // #2424: the grammar is on the ⌘? overlay, not in the placeholder.
  const placeholder = target.querySelector("input")!.placeholder;
  expect(placeholder).toBe("Search tickets, code, builds and docs");
  expect(placeholder).not.toMatch(/[/#:]/);
  expect(document.body.textContent).toContain("⌘?");
});

/**
 * The third reader of the actionable-health rule, and the one nothing else in
 * this file covers.
 *
 * An `in:gitea` chip's dot and the row badge beside it are the same claim about
 * the same source, so they are drawn from one rule (`shell/health.svelte::isActionable`)
 * rather than from three copies of the state list — which is what #37 asked for
 * and what this asserts is actually wired up. `ok` is the control: without it a
 * chip renderer that marked every source failing would pass.
 */
test("a source chip is marked when that source is refusing the credential", async () => {
  open({
    ports: {
      launcherHome: async (): Promise<LauncherHome> => ({
        ...HOME,
        sources: [
          {
            source_id: "gitea",
            state: "unauthorized",
            checked_at: null,
            detail: "401",
            secret_expires_at: null,
          },
          {
            source_id: "jira",
            state: "ok",
            checked_at: null,
            detail: null,
            secret_expires_at: null,
          },
        ],
      }),
      search: async () =>
        response({
          interpreted: {
            text: "sepa",
            prefix: null,
            filters: {
              sources: ["gitea", "jira"],
              kinds: [],
              updated_within_days: null,
              mine: false,
              authors: [],
            },
            unknown_tokens: [],
            left_out_tokens: [],
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "in:gitea in:jira sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const chips = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["gitea", "jira"]);
  const failing = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set.tf-ln-fail")].map((el) => el.textContent?.trim());
  expect(failing).toEqual(["gitea"]);
});

/**
 * The chip whose source the health store has no row for at all.
 *
 * `/nowhere` is a source id the user can type before the scheduler's first run
 * has produced a reading for it — or one that never existed. The chip renders
 * either way, and "we have not heard from it" is not "it is refusing our
 * credential": marking it red would put a complaint on a fresh install, which
 * is the same wrong `unknown` is deliberately kept out of `ACTIONABLE` to
 * avoid.
 *
 * Before #72 this was the call site's own `state !== undefined` guard; now it
 * is `shell/health.svelte::isActionable` answering for its own missing case.
 * The assertion is the same either way, which is the point — the fold moved
 * where the clause is written, not what it decides. `gitea` is the control:
 * without a genuinely failing chip in the same render, a `fail` that was
 * never set at all would pass.
 */
test("a chip for a source with no health reading is not marked failing", async () => {
  open({
    ports: {
      launcherHome: async (): Promise<LauncherHome> => ({
        ...HOME,
        sources: [
          {
            source_id: "gitea",
            state: "unauthorized",
            checked_at: null,
            detail: "401",
            secret_expires_at: null,
          },
        ],
      }),
      search: async () =>
        response({
          interpreted: {
            text: "sepa",
            prefix: null,
            filters: {
              sources: ["gitea", "nowhere"],
              kinds: [],
              updated_within_days: null,
              mine: false,
              authors: [],
            },
            unknown_tokens: [],
            left_out_tokens: [],
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "in:gitea in:nowhere sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const chips = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["gitea", "nowhere"]);
  const failing = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set.tf-ln-fail")].map((el) => el.textContent?.trim());
  expect(failing).toEqual(["gitea"]);
});

/**
 * The chip that ruling **E-Q1** made possible, and the two halves it keeps
 * apart.
 *
 * `@me` and a named person are two dimensions of one predicate, and the
 * launcher has to draw both — but only the name. The usernames `@me` resolved
 * to stay in the backend: a chip row that listed them would be claiming a
 * filter the user never typed, and the `@me` chip beside it already says the
 * same thing.
 */
test("a named person is a chip, and nothing about them is greyed out", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          interpreted: {
            text: "sepa",
            prefix: "person",
            filters: {
              sources: [],
              kinds: [],
              updated_within_days: null,
              mine: true,
              authors: ["jonas"],
            },
            unknown_tokens: [],
            left_out_tokens: [],
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "@me @jonas sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const chips = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["@me", "jonas"]);
  // The prefix is a mode, said in front of the box where a section's chip stands, and it moves no row (F-626).
  expect(target.querySelector(".tf-ln-boxrow .tf-ln-mode")?.textContent).toBe("People");
  expect([...target.querySelectorAll(".tf-ln-fchip.tf-ln-off")]).toEqual([]);
});

test("`?` lists the syntax, and clicking a row inserts it", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          interpreted: {
            text: "",
            prefix: "help",
            filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
            unknown_tokens: [],
            left_out_tokens: [],
          },
          groups: [],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "?";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const rows = [...target.querySelectorAll(".tf-ln-opt")];
  expect(rows.length).toBeGreaterThan(8);
  // One row per `#` letter, each one word (#2420), and no `/` source row.
  const tokens = rows.map((row) => row.querySelector(".tf-ln-tok")?.textContent?.trim() ?? "");
  for (const letter of ["#t", "#p", "#r", "#b", "#m", "#d", "#l", "#i"]) {
    expect(tokens.some((token) => token.startsWith(letter))).toBe(true);
  }
  expect(tokens.some((token) => token.startsWith("/"))).toBe(false);

  rows[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await settle();
  expect(target.querySelector("input")!.value).toBe("#t ");
});

/**
 * **Issue #141, as the reader meets it.**
 *
 * An `author:` search over a corpus that includes a build source comes back
 * empty, and until this the overlay said only *nothing matches* — which is
 * exactly what a broken token would say. The explanation sits above the
 * results, where the results would have been.
 */
test("an author search says which sources could not be asked", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          interpreted: {
            text: "",
            prefix: "person",
            filters: {
              sources: [],
              kinds: [],
              updated_within_days: null,
              mine: false,
              authors: ["jonas"],
            },
            unknown_tokens: [],
            left_out_tokens: [],
          },
          groups: [],
          total: 0,
          coverage: [
            {
              dimension: "author",
              sources: [
                { source_id: "jira", display_name: "Jira", answer: "answered" },
                { source_id: "teamcity", display_name: "Buildserver", answer: "no_values" },
              ],
            },
          ],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "@jonas";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const gap = target.querySelector(".tf-ln-gap");
  expect(gap?.textContent).toContain("An author search cannot be answered by:");
  expect(gap?.textContent).toContain("Buildserver");
  expect(gap?.textContent).toContain("nothing it has synced names a person");
  // The source that *did* answer is not named: it has nothing to explain, and
  // listing it would turn the explanation into a roll call.
  expect(gap?.textContent).not.toContain("Jira");
  // And the ordinary empty-result line is still there — the gap explains part
  // of the emptiness, it does not replace the answer. Its words, not merely
  // its presence: this is another of the launcher's accounts of the store it
  // read, and it used the word `CONTEXT.md`'s **Mirror** entry forbids until
  // #518.
  expect(target.querySelector(".tf-ln-none")?.textContent).toContain(
    "Nothing matches",
  );
});

/**
 * The miss direction the ruling names: a corpus whose sources all answered
 * yields no explanation.
 *
 * Without this, a component that drew the block unconditionally — or drew it
 * from `coverage.length` rather than from the verdicts — would pass the test
 * above and put "not every source can answer" on screen for a search where
 * every source did.
 */
test("an author search every source could answer explains nothing", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({
          coverage: [
            {
              dimension: "author",
              sources: [{ source_id: "jira", display_name: "Jira", answer: "answered" }],
            },
          ],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "@jonas sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  // The results are there, so this is not green because nothing rendered.
  expect(target.textContent).toContain("Retry failed SEPA payouts");
  expect(target.querySelector(".tf-ln-gap")).toBeNull();
});

/**
 * **The estate, as a reader meets it in ⌘K** (#436, spec §4 story 40).
 *
 * One response with an Assets group and a Routes group, in the engine's own
 * fixed order, and three things asserted about the rows: the monogram, the
 * path from the root under the name, and the address `Enter` goes to.
 *
 * The path is the point of the ticket — *"⌘K finds a container by name and
 * tells me which host it is on"* — and it is drawn by the same `Row.svelte`
 * that draws a Confluence page's ancestors, which is why the assertion is on
 * what is on screen rather than on a prop.
 */
const ESTATE = {
  groups: [
    {
      kind: "asset",
      label: "Asset",
      plural: "Assets",
      monogram: "AS",
      total: 1,
      hits: [
        hit({
          entity_id: "asset:9f1c",
          kind: "asset",
          source_id: "asset",
          title: "postgres",
          path: "hel1 › vm-db-01",
        }),
      ],
    },
    {
      kind: "route",
      label: "Route",
      plural: "Routes",
      monogram: "RO",
      total: 1,
      hits: [
        hit({
          entity_id: "route:2ab7",
          kind: "route",
          source_id: "route",
          title: "Uptime Kuma",
          path: "hel1 › vm-db-01 › kuma",
        }),
      ],
    },
  ],
  total: 2,
};

/** Type a query and let the debounce and the injected search settle. */
async function search(text: string) {
  target.querySelector("input")!.value = text;
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();
}

test("Enter on a Jira view opens that view in Tickets, never Code (#1094)", async () => {
  const onnavigate = vi.fn();
  const views = {
    groups: [
      {
        kind: "jira_view",
        label: "Jira view",
        plural: "Jira views",
        monogram: "JV",
        total: 1,
        hits: [hit({ entity_id: "jira:board-21", kind: "jira_view", source_id: "jira", title: "PAY board" })],
      },
    ],
    total: 1,
  };
  open({ onnavigate, ports: { search: async () => response(views), launcherHome: async () => HOME } });
  await settle();
  await search("pay board");

  press("Enter");
  await settle();

  expect(onnavigate).toHaveBeenCalledWith("#/tickets/jira-board-21");
  expect(onnavigate).not.toHaveBeenCalledWith("#/code");
});

/** A saved ticket view, a link set and a pipeline, as the engine groups them (#2422). */
const OWN_KINDS = {
  groups: [
    {
      kind: "view",
      label: "Ticket view",
      plural: "Ticket views",
      monogram: "VW",
      total: 1,
      hits: [hit({ entity_id: "payments-backlog", kind: "view", source_id: "view", title: "Payments backlog", path: 'Team · project:PAY status:"To Do"' })],
    },
    {
      kind: "set",
      label: "Link set",
      plural: "Link sets",
      monogram: "LS",
      total: 1,
      hits: [hit({ entity_id: "payout-stack", kind: "set", source_id: "set", title: "Payout stack", path: "6 items · 2 patterns · 4 tickets" })],
    },
    {
      kind: "pipeline",
      label: "Pipeline",
      plural: "Pipelines",
      monogram: "PL",
      total: 1,
      hits: [
        hit({
          entity_id: "teamcity:buildType:Payout_Deploy",
          kind: "pipeline",
          source_id: "teamcity",
          title: "Payout_Deploy",
          path: "Tidewater › Payouts",
          state: { text: "FAILURE", tone: "err" },
        }),
      ],
    },
  ],
  total: 3,
};

test("a view, a set and a pipeline each draw their group and their key cell (#2422)", async () => {
  open({ ports: { search: async () => response(OWN_KINDS), launcherHome: async () => HOME } });
  await settle();
  await search("pay");

  const headings = [...target.querySelectorAll(".tf-ln-lab")].map((el) => el.textContent?.trim());
  expect(headings).toEqual(["Ticket views", "Link sets", "Pipelines"]);
  const rows = [...target.querySelectorAll(".tf-ln-res")].map((row) => [
    row.querySelector(".tf-ln-mg")?.textContent?.trim(),
    row.querySelector(".tf-ln-k")?.textContent?.trim(),
    row.querySelector(".tf-ln-t")?.textContent?.trim(),
  ]);
  expect(rows).toEqual([
    ["Vw", "Team", "Payments backlog"],
    ["Ls", "link set", "Payout stack"],
    ["Pl", "Payouts", "Payout_Deploy"],
  ]);
  // A pipeline is coloured by its newest run, the engine's state.
  expect(target.querySelectorAll(".tf-ln-mg")[2]?.getAttribute("data-tone")).toBe("err");
});

test("Enter on a view opens its Tickets page, on a set its page, on a pipeline Builds at it (#2422)", async () => {
  const addresses = ["#/tickets/payments-backlog", "#/sets/payout-stack", "#/builds/teamcity%3AbuildType%3APayout_Deploy"];
  for (const [at, address] of addresses.entries()) {
    const onnavigate = vi.fn();
    open({ onnavigate, ports: { search: async () => response(OWN_KINDS), launcherHome: async () => HOME } });
    await settle();
    await search("pay");
    for (let step = 0; step < at; step += 1) press("ArrowDown");
    press("Enter");
    await settle();
    expect(onnavigate, address).toHaveBeenCalledWith(address);
    if (app) unmount(app);
    app = undefined;
  }
});

/**
 * #1504: while notes are shown a note hit opens its own page, and a note the
 * standup protocol rule names ("Standup 2026-09-03", the backend's own example)
 * opens Time › Standup at its date.
 */
test("Enter on a note opens its page, and on a protocol note the Standup at its date (#1504)", async () => {
  for (const [title, address] of [
    ["Standup 2026-09-03", "#/time/standup/2026-09-03"],
    ["Retry backoff, thinking aloud", "#/note/note%3A7a1"],
  ] as const) {
    const onnavigate = vi.fn();
    const notes = {
      groups: [
        {
          kind: "note",
          label: "Note",
          plural: "Notes",
          monogram: "NO",
          total: 1,
          hits: [hit({ entity_id: "note:7a1", kind: "note", source_id: "knobas", title })],
        },
      ],
      total: 1,
    };
    open({ onnavigate, ports: { search: async () => response(notes), launcherHome: async () => HOME } });
    await settle();
    await search("standup");
    press("Enter");
    await settle();
    expect(onnavigate, title).toHaveBeenCalledWith(address);
    unmount(app!);
    app = undefined;
  }
});

test("Enter on an asset opens the Tree at it, and on a route at its exposing asset", async () => {
  const onnavigate = vi.fn();
  open({
    onnavigate,
    ports: { search: async () => response(ESTATE), launcherHome: async () => HOME },
  });
  await settle();
  await search("kuma");

  press("Enter");
  await settle();
  // Story 35: an asset's address *is* the Tree with a selection, not a room
  // with a slide-over over it.
  expect(onnavigate).toHaveBeenCalledWith("#/asset/asset%3A9f1c");

  open({
    onnavigate,
    ports: { search: async () => response(ESTATE), launcherHome: async () => HOME },
  });
  await settle();
  await search("kuma");
  press("ArrowDown");
  press("Enter");
  await settle();
  // A route is not a surface of its own (#432): its address is the Tree, at the
  // asset that exposes it.
  expect(onnavigate).toHaveBeenLastCalledWith("#/route/route%3A2ab7");
});

/**
 * **The chain's two rows, in order** (#1500, the 2026-10-21 ruling on #1416):
 * *Link to <key>* then *Start timer on <title>*, and nothing else, on an
 * asset as on a ticket.
 */
// -- a pasted link (#496, spec #491 stories 9 and 10) ------------------------

const PASTED = "https://jira.example/browse/PAY-231";

// Typed through `search` above: what makes a paste a paste is the *text*, not
// a different gesture, and using the same helper is how this file says so.

/**
 * Story 9: *"a link from chat is one paste away from its detail"*.
 *
 * The seam is the whole overlay: text into the box, and the address the shell
 * is handed out. What the shell then does with that address is the shell's,
 * which is the same division every other row in this component follows.
 */
test("a pasted link that resolves opens the entity it names", async () => {
  const navigated: string[] = [];
  const closed = vi.fn();
  open({
    onnavigate: (hash: string) => navigated.push(hash),
    onclose: closed,
    ports: {
      search: async () => response(),
      launcherHome: async () => HOME,
      resolveUrl: async () => ({ entity_id: "jira:PAY-231", kind: "ticket" }),
    },
  });
  await settle();
  await search(PASTED);

  // The ticket-first shell's job, never the rooms UI's `#/ticket/…` (#1085).
  expect(navigated).toEqual(["#/ticket/jira%3APAY-231"]);
  expect(closed, "the overlay gets out of the way first").toHaveBeenCalled();
});

/** An asset resolves through the Tree's own encoder, not the room's. */
test("a pasted link to an asset opens the Tree at it", async () => {
  const navigated: string[] = [];
  open({
    onnavigate: (hash: string) => navigated.push(hash),
    ports: {
      search: async () => response(),
      launcherHome: async () => HOME,
      resolveUrl: async () => ({ entity_id: "asset:hel1", kind: "asset" }),
    },
  });
  await settle();
  await search("https://kuma.example/dashboard/8");

  expect(navigated).toEqual(["#/asset/asset%3Ahel1"]);
});

/**
 * Story 10: the paste never dead-ends. What is on screen says the mirror does
 * not hold the link, and the one thing left to do with it is offered.
 */
test("a pasted link the mirror does not hold offers the browser instead", async () => {
  const opened: string[] = [];
  const navigated: string[] = [];
  open({
    onnavigate: (hash: string) => navigated.push(hash),
    openExternal: async (url: string) => {
      opened.push(url);
    },
    ports: {
      search: async () => response(),
      launcherHome: async () => HOME,
      resolveUrl: async () => null,
    },
  });
  await settle();
  // A fragment on the way in, so the assertion below can see which URL the
  // button hands over: the raw one, not the normalised one the resolver used.
  await search(`${PASTED}#comment-42`);

  // The address as pasted is what missed, not the page (#1475): the source
  // may serve it under a form this adapter does not declare.
  expect(target.querySelector(".tf-ln-miss")?.textContent).toContain(
    "Not in the mirror as this address; it may be stored under another form.",
  );
  // The mirror's own word, and not a synonym `CONTEXT.md`'s **Mirror** entry
  // says to avoid. The whole overlay now, not just the panel: #518 took the
  // forbidden word off the footnote, the empty-result line and the reading
  // placeholder, so every word the launcher draws is in scope.
  //
  // What this guards: the launcher is the screen that names the store to the
  // reader — the footnote, the reading placeholder, the empty-result line and
  // this panel — so a further string here saying *index* is far likelier to
  // be the forbidden synonym than a database object. It is a fence, not the
  // witness; the footnote and the panel each have a positive assertion of
  // their own.
  //
  // The day a feature has a legitimate use of the word on this screen (the
  // Postgres index `Diagnostics.svelte` names, say), **narrow** this to the
  // footnote and `.miss` rather than deleting it. Do not narrow it back to
  // `.miss` alone: at that scope the footnote's mutant was invisible, which
  // is why it widened. (Deputy's ruling of 2026-09-08 on #518, part 2.)
  expect(
    target.textContent?.toLowerCase(),
    "`index` is not the word for the mirror",
  ).not.toContain("index");
  expect(navigated, "there is nothing to open in the app").toEqual([]);

  const button = [...target.querySelectorAll("button")].find((element) =>
    element.textContent?.includes("Open in browser"),
  );
  expect(button, "the miss offers the browser").toBeDefined();
  button!.click();
  await settle();

  expect(opened).toEqual([`${PASTED}#comment-42`]);
});

/** The same, from the keyboard: a miss is not a state the box cannot leave. */
test("Enter on a missed paste opens it in the browser", async () => {
  const opened: string[] = [];
  open({
    openExternal: async (url: string) => {
      opened.push(url);
    },
    ports: {
      search: async () => response(),
      launcherHome: async () => HOME,
      resolveUrl: async () => null,
    },
  });
  await settle();
  await search(PASTED);
  press("Enter");
  await settle();

  expect(opened).toEqual([PASTED]);
});

/**
 * The wiring, and the reason it is a test rather than a type.
 *
 * `resolveUrl` is optional on `SessionPorts` — a session without one searches
 * a link like any other query, which is what the two link pickers want. That
 * makes the launcher's own default the only thing standing between this
 * feature and silence, and dropping it would be a component that compiles,
 * type-checks and quietly goes back to searching for the words in a hostname.
 *
 * So the ports here deliberately supply **no** resolver: the real one is what
 * the component must reach for, and in a test environment with no Tauri bridge
 * reaching for it fails. What is asserted is that the search engine was not
 * asked — which is true only if the paste took the resolver's path.
 */
test("the launcher's own default resolver is what a paste reaches", async () => {
  const searchPort = vi.fn(async () => response());
  open({
    ports: { search: searchPort, launcherHome: async () => HOME },
  });
  await settle();
  await search(PASTED);

  expect(searchPort, "a pasted link must not reach the search engine").not.toHaveBeenCalled();
});

// ---------------------------------------------------------------------------
// Saved lists (#506, #2423)
// ---------------------------------------------------------------------------

/** What `create_smart_list` answers with for the query saved below. */
const SAVED = {
  id: "sepa-retries",
  label: "SEPA retries",
  count: 12,
  description: "sepa retries",
  needs_attention: false,
};

/** A list hit, as the engine answers `#l` with one (kind `list`, the text as its path line). */
function listHit(id: string, title: string, text: string, state: StateChip | null = null) {
  return hit({ entity_id: id, kind: "list", source_id: "list", title, path: text, state });
}

/** `#l`'s answer: the saved lists, one needing attention. */
function listsAnswer(): SearchResponse {
  return response({
    interpreted: {
      text: "",
      prefix: null,
      filters: { sources: [], kinds: ["list"], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [
      {
        kind: "list",
        label: "List",
        plural: "Lists",
        monogram: "LI",
        total: 2,
        hits: [
          listHit("sepa-tickets", "SEPA tickets", "#t sepa @me"),
          listHit("old-gitea", "Old Gitea search", "/gitea payout", {
            text: "Needs attention: the saved query names a source as `/gitea`. Delete it and save the search again.",
            tone: "warn",
          }),
        ],
      },
    ],
    total: 2,
  });
}

/**
 * Enter on a saved list puts its own text back into the box, filters and
 * words, and runs it; the reader stays in the box (#2423, spec #2419 story
 * 14). The row's key cell is the kind's word and its monogram `LI`.
 */
test("enter on a saved list applies its text in the box and stays", async () => {
  const onnavigate = vi.fn();
  const asked: string[] = [];
  open({
    onnavigate,
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => {
        asked.push(query.raw);
        return query.raw.startsWith("#l") ? listsAnswer() : response();
      },
    },
  });
  await settle();
  await search("#l");
  const row = target.querySelector(".tf-ln-res");
  expect(row?.querySelector(".tf-ln-mg")?.textContent).toBe("Li");
  expect(row?.querySelector(".tf-ln-k")?.textContent).toBe("list");
  expect(row?.querySelector(".tf-ln-t")?.textContent).toBe("SEPA tickets");

  press("Enter");
  await settle();
  expect(target.querySelector("input")!.value).toBe("#t sepa @me ");
  expect(asked.at(-1)).toBe("#t sepa @me ");
  expect(onnavigate, "a list is applied, not opened somewhere").not.toHaveBeenCalled();
  expect(target.querySelector("input"), "the launcher stays open").not.toBeNull();
});

/**
 * A list whose text today's grammar refuses says so and does not apply: its
 * row is in the warning tone with the reason as its name, and Enter raises the
 * reason instead of putting the text into the box.
 */
test("a saved list that needs attention says why and does not apply", async () => {
  toasts.items.splice(0);
  const asked: string[] = [];
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => {
        asked.push(query.raw);
        return listsAnswer();
      },
    },
  });
  await settle();
  await search("#l");
  const broken = target.querySelectorAll(".tf-ln-res")[1]!;
  expect(broken.querySelector(".tf-ln-mg")?.getAttribute("data-tone")).toBe("warn");
  expect(broken.querySelector(".tf-ln-mg")?.getAttribute("title")).toContain("Needs attention");

  press("ArrowDown");
  press("Enter");
  await settle();
  expect(target.querySelector("input")!.value).toBe("#l");
  expect(asked).toEqual(["#l"]);
  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "Needs attention: the saved query names a source as `/gitea`. Delete it and save the search again.",
  ]);
  toasts.items.splice(0);
});

/** Three inbox lines, out of order, so "newest first" is the launcher's doing. */
const INBOX = {
  count: 3,
  items: [
    inboxItem("a", "review_request", "Review: SEPA backoff", "2026-08-25T09:15:00Z", "gitea:tidewater/payout-service#142", "pr"),
    inboxItem("b", "failed_build", "Build #1187 failed", "2026-08-25T11:40:00Z", "teamcity:1187", "build"),
    inboxItem("c", "new_assignment", "Assigned: reconcile mandates", "2026-08-24T16:05:00Z", "jira:PAY-240", "ticket"),
  ],
};

function inboxItem(
  key: string,
  category: InboxItem["category"],
  title: string,
  occurred_at: string,
  entity_id: string,
  kind: string,
): InboxItem {
  return {
    key: `${category}:${key}`,
    category,
    source_id: entity_id.split(":")[0]!,
    entity_id,
    kind,
    title,
    reason: "",
    occurred_at,
    web_url: null,
    snoozed_until: null,
  };
}

/** `#i`'s answer: no groups (nothing in the index is an inbox line), kind `inbox`. */
function inboxAnswer(text: string): SearchResponse {
  return response({
    interpreted: {
      text,
      prefix: null,
      filters: { sources: [], kinds: ["inbox"], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [],
    total: 0,
  });
}

/**
 * `#i` answers the inbox store's lines newest first, the time as the key
 * cell; `#i review` narrows them; Enter opens the line's opener (#2423, spec
 * #2419 story 15).
 */
test("hash i lists the inbox lines newest first and enter opens one", async () => {
  const onnavigate = vi.fn();
  open({
    inbox: INBOX,
    onnavigate,
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => inboxAnswer(query.raw.replace(/^#i\s*/u, "").trim()),
    },
  });
  await settle();
  await search("#i");
  const rows = [...target.querySelectorAll(".tf-ln-res")];
  expect(rows.map((row) => row.querySelector(".tf-ln-t")?.textContent)).toEqual([
    "Build #1187 failed",
    "Review: SEPA backoff",
    "Assigned: reconcile mandates",
  ]);
  const times = rows.map((row) => row.querySelector(".tf-ln-k")?.textContent ?? "");
  expect(times[0]).toMatch(/^\d\d:\d\d$/u);
  expect(times[2]).toMatch(/^yesterday \d\d:\d\d$/u);
  expect(target.textContent).not.toContain("Nothing matches");

  await search("#i review");
  const narrowed = [...target.querySelectorAll(".tf-ln-res")];
  expect(narrowed.map((row) => row.querySelector(".tf-ln-t")?.textContent)).toEqual(["Review: SEPA backoff"]);
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
});

/**
 * Under *Titles* a word printed on the line's Inbox row finds it, its reason as
 * much as its title, since the row reads as one sentence (Round 17: *down* on
 * "knobas-jira is down" found nothing); a word on no row finds nothing.
 */
test("hash i under Titles matches what the line's row prints, its reason too", async () => {
  const failed = { ...INBOX.items[1]!, reason: "Step deploy-staging exited 1" };
  open({
    inbox: { count: 1, items: [failed] },
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => inboxAnswer(query.raw.replace(/^#i\s*/u, "").trim()),
    },
  });
  await settle();
  const titles = () => [...target.querySelectorAll(".tf-ln-res")].map((row) => row.querySelector(".tf-ln-t")?.textContent);
  await search("#i staging");
  expect(titles()).toEqual(["Build #1187 failed: Step deploy-staging exited 1"]);
  await search("#i nowhere");
  expect(titles()).toEqual([]);
  await search("#i build");
  expect(titles()).toEqual(["Build #1187 failed: Step deploy-staging exited 1"]);
});

/**
 * *Save as list* is offered on a query and not on the board (story 56).
 *
 * Both halves, because the control's whole rule is *when*: an empty box is the
 * board, and a board has nothing to save.
 */
test("save as list is offered on a query, never on an empty box", async () => {
  const created = vi.fn(async () => SAVED);
  const searched = vi.fn(async () => response());
  open({
    ports: { launcherHome: async () => HOME, search: searched, createSmartList: created },
  });
  await settle();
  expect(target.querySelector("button.tf-ln-save"), "an empty box is the board").toBeNull();

  await search("sepa retries");
  const button = target.querySelector<HTMLButtonElement>("button.tf-ln-save");
  expect(button?.textContent?.trim()).toBe("Save as list…");

  button!.click();
  await settle();

  // The button asks for the name in the box, the query prefilled (#2424, as
  // ⌘S does; `Launcher.overlay.test.svelte.ts`). Enter keeps the prefill.
  expect(created).not.toHaveBeenCalled();
  press("Enter");
  await settle();
  expect(created).toHaveBeenCalledWith("sepa retries", "sepa retries");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Saved as list");
  expect(
    searched.mock.calls.length,
    "the query is asked again rather than the answer patched in place",
  ).toBeGreaterThan(1);

  // And the next keystroke is a different query, so the control comes back.
  await search("sepa retries and more");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Save as list…");
});

/**
 * The control is off the panels that are not searches (#506).
 *
 * `>` and `?` have text in the box and are not queries the engine answers, so
 * `create_smart_list` would refuse them — and the launcher must not offer a
 * button whose only outcome is that refusal. The gate is `Session.mode`, which
 * is read off the **backend's own interpretation**, so this is the backend's
 * grammar deciding and not a copy of the prefix table on this side.
 */
test("save as list is off the palette and the help card", async () => {
  const created = vi.fn(async () => SAVED);
  for (const prefix of ["action", "help"] as const) {
    open({
      ports: {
        launcherHome: async () => HOME,
        search: async () =>
          response({ interpreted: { ...response().interpreted, prefix }, groups: [] }),
        createSmartList: created,
      },
    });
    await settle();
    await search(prefix === "action" ? "> sources" : "?");
    expect(target.querySelector("button.tf-ln-save"), `${prefix} is not a query`).toBeNull();
    if (app) unmount(app);
    app = undefined;
  }
  expect(created).not.toHaveBeenCalled();
});

/**
 * The control is off the query that answers as results and that the backend
 * still refuses to save, and off a search that failed (#1413).
 *
 * `t ` is a worklog query and not a search, and a failed search leaves an
 * error line with no answer under it. Both read as *results* to the mode helper, so the gate is the
 * backend's `interpreted.prefix` and the presence of an answer. The plain
 * query at the end is the positive control: without it a launcher that never
 * drew the control would pass.
 */
test("save as list is off a time query and a failed search", async () => {
  const created = vi.fn(async () => SAVED);
  const cases = [
    { raw: "t today", prefix: "time" as const, fails: false },
    { raw: "sepa retries", prefix: null, fails: true },
  ];
  for (const { raw, prefix, fails } of cases) {
    open({
      ports: {
        launcherHome: async () => HOME,
        search: async () => {
          if (fails) throw { code: "internal", message: "the engine is down" };
          return response({ interpreted: { ...response().interpreted, prefix } });
        },
        createSmartList: created,
      },
    });
    await settle();
    await search(raw);
    if (fails) expect(target.textContent).toContain("the engine is down");
    expect(target.querySelector("button.tf-ln-save"), `${raw} cannot be saved`).toBeNull();
    if (app) unmount(app);
    app = undefined;
  }
  expect(created).not.toHaveBeenCalled();

  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () => response(),
      createSmartList: created,
    },
  });
  await settle();
  await search("sepa retries");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Save as list…");
});

// ── Value suggestions for `kind:`, `source:` and `under:` (#601) ──────────

const SUGGESTIONS: FilterSuggestions = {
  kinds: [
    { kind: "commit", label: "Commit" },
    { kind: "repo", label: "Repository" },
    { kind: "ticket", label: "Ticket" },
  ],
  sources: [
    { id: "gitea", display_name: "Gitea" },
    { id: "jira-eu", display_name: "Jira EU" },
  ],
  parents: [
    { name: "payout-service", kind: "page" },
    { name: "payout-service", kind: "repo" },
    { name: "Runbooks Overview", kind: "page" },
    { name: "tidewater/payout-service", kind: "repo" },
  ],
  fields: [
    { key: "status", value: "In Progress", detail: "" },
    { key: "status", value: "In Review", detail: "" },
    { key: "assignee", value: "@me", detail: "you" },
    { key: "sprint", value: "current", detail: "the active sprint" },
  ],
};

/** A launcher whose bridge answers the suggestion read, and records searches. */
function openSuggesting() {
  const searched: string[] = [];
  const filterSuggestions = vi.fn(async () => SUGGESTIONS);
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => {
        searched.push(query.raw);
        return response();
      },
      filterSuggestions,
    },
  });
  return { searched, filterSuggestions };
}

async function typeInto(value: string) {
  const input = target.querySelector("input")!;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
}

/** What the suggestion list shows, one string per option. */
function offered(): string[] {
  return [...target.querySelectorAll('[aria-label="Suggestions"] [role="option"]')].map(
    (el) => el.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
}

test("kind: opens the kinds that exist, and typing narrows them", async () => {
  openSuggesting();
  await settle();
  await typeInto("kind:");
  expect(offered()).toEqual(["commit · Commit", "repo · Repository", "ticket · Ticket"]);

  await typeInto("kind:re");
  expect(offered()).toEqual(["repo · Repository"]);
});

/**
 * #1515: the detail is set off by a space on each side of its dot, and the
 * space before the dot is the detail's own. The option is a flex row, where
 * the whitespace between the value and the detail is not drawn, so the
 * option's whole `textContent` (and `offered()`, which collapses it) holds a
 * space the reader never sees. The detail's own text is what is drawn.
 */
test("a value suggestion's detail carries the space before its dot", async () => {
  openSuggesting();
  await settle();
  await typeInto("kind:re");
  const option = target.querySelector('[aria-label="Suggestions"] [role="option"]');
  expect(option?.querySelector(".tf-ln-s")?.textContent).toBe(" · Repository");
});

test("source: opens the configured sources by id, with the name the sources list shows", async () => {
  openSuggesting();
  await settle();
  await typeInto("sepa source:");
  expect(offered()).toEqual(["gitea · Gitea", "jira-eu · Jira EU"]);
});

test("under: values show their kind, so a repo and a same-named page are told apart", async () => {
  openSuggesting();
  await settle();
  await typeInto("under:payout");
  expect(offered()).toEqual([
    "payout-service · page",
    "payout-service · repo",
    "tidewater/payout-service · repo",
  ]);
});

test("a ticket key opens the values the mirror holds, a value without a detail drawn alone (#811)", async () => {
  openSuggesting();
  await settle();
  await typeInto("status:in");
  expect(offered()).toEqual(["In Progress", "In Review"]);
  await typeInto("sepa -assignee:");
  expect(offered()).toEqual(["@me · you"]);
  await typeInto("sprint:");
  expect(offered()).toEqual(["current · the active sprint"]);
});

test("picking a ticket-key value with a space quotes it and runs the query (#811)", async () => {
  const { searched } = openSuggesting();
  await settle();
  await typeInto('status:"in r');
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(target.querySelector("input")!.value).toBe('status:"In Review" ');
  expect(searched.at(-1)).toBe('status:"In Review" ');
});

test("a picked value completes the token and runs the query", async () => {
  const { searched } = openSuggesting();
  await settle();
  await typeInto("jitter under:payout");
  press("ArrowDown");
  press("Enter");
  await settle();

  const input = target.querySelector("input")!;
  expect(input.value).toBe("jitter under:payout-service ");
  expect(searched.at(-1)).toBe("jitter under:payout-service ");
  // The token is complete, so the list has closed.
  expect(offered()).toEqual([]);
});

test("a clicked value completes the token, and a name with a space is quoted", async () => {
  openSuggesting();
  await settle();
  await typeInto("under:runb");
  const option = target.querySelector<HTMLButtonElement>(
    '[aria-label="Suggestions"] [role="option"]',
  )!;
  option.click();
  await settle();
  expect(target.querySelector("input")!.value).toBe('under:"Runbooks Overview" ');
});

test("Esc closes the suggestions before it clears the box", async () => {
  openSuggesting();
  await settle();
  await typeInto("kind:");
  expect(offered()).not.toEqual([]);

  const event = press("Escape");
  expect(event.defaultPrevented).toBe(true);
  expect(offered()).toEqual([]);
  expect(target.querySelector("input")!.value).toBe("kind:");
});

test("free-text keys open nothing, and the read is never asked for them", async () => {
  const { filterSuggestions } = openSuggesting();
  await settle();
  for (const raw of ["author:", "author:ma", "updated:", "updated:7d", "sepa", "kind:repo "]) {
    await typeInto(raw);
    expect(offered(), raw).toEqual([]);
  }
  // `kind:repo ` is a finished token -- the box ends in a space -- so none of
  // these is a value being typed, and the read is not made for any of them.
  expect(filterSuggestions).toHaveBeenCalledTimes(0);
});

test("under: asks the engine for the value being typed and draws its answer (#761)", async () => {
  // The engine answers parents by prefix: what it was asked for is what the
  // list may offer, so a list built from the opening's first read would show
  // names this one never returned.
  const asked: (string | undefined)[] = [];
  const filterSuggestions = vi.fn(async (under?: string) => {
    asked.push(under);
    return {
      ...SUGGESTIONS,
      parents: SUGGESTIONS.parents.filter((p) =>
        p.name.toLowerCase().startsWith((under ?? "").toLowerCase()),
      ),
    };
  });
  open({
    ports: { launcherHome: async () => HOME, search: async () => response(), filterSuggestions },
  });
  await settle();

  await typeInto("under:");
  expect(asked).toEqual([""]);
  await typeInto("under:tide");
  expect(asked).toEqual(["", "tide"]);
  expect(offered()).toEqual(["tidewater/payout-service · repo"]);

  // The same value is not asked twice, and kind: needs no prefix.
  await typeInto("under:tide ");
  await typeInto("under:tide kind:");
  expect(asked).toEqual(["", "tide", ""]);
});

test("typing several under: values reads the kinds and the sources once (#768)", async () => {
  // The parents alone answer a keystroke: an engine that read the kinds and
  // the sources again would hand them back, so this one hands back none, and
  // the lists drawn after them can only be the opening's.
  const asked: [string | undefined, boolean | undefined][] = [];
  const filterSuggestions = vi.fn(async (under?: string, parentsOnly?: boolean) => {
    asked.push([under, parentsOnly]);
    return parentsOnly ? { kinds: [], sources: [], parents: SUGGESTIONS.parents } : SUGGESTIONS;
  });
  open({
    ports: { launcherHome: async () => HOME, search: async () => response(), filterSuggestions },
  });
  await settle();

  await typeInto("under:");
  await typeInto("under:t");
  await typeInto("under:ti");
  await typeInto("under:tide");
  expect(asked).toEqual([
    ["", false],
    ["t", true],
    ["ti", true],
    ["tide", true],
  ]);
  expect(offered()).toEqual(["tidewater/payout-service · repo"]);

  // The kinds are still the opening's, and asking for them reads nothing.
  await typeInto("under:tide kind:");
  expect(offered()).toEqual(["commit · Commit", "repo · Repository", "ticket · Ticket"]);
  expect(asked.filter(([, parentsOnly]) => parentsOnly === false)).toHaveLength(1);
});

test("a second under: value typed while the opening's read is in flight reads no kinds either (#768)", async () => {
  let land!: (v: FilterSuggestions) => void;
  const filterSuggestions = vi.fn((_under?: string, parentsOnly?: boolean) =>
    parentsOnly
      ? Promise.resolve({ kinds: [], sources: [], parents: SUGGESTIONS.parents })
      : new Promise<FilterSuggestions>((resolve) => (land = resolve)),
  );
  open({
    ports: { launcherHome: async () => HOME, search: async () => response(), filterSuggestions },
  });
  await settle();
  await typeInto("under:");
  await typeInto("under:t");
  land(SUGGESTIONS);
  await settle();
  expect(filterSuggestions.mock.calls.map(([, parentsOnly]) => parentsOnly)).toEqual([false, true]);
  await typeInto("under:t kind:");
  expect(offered()).toEqual(["commit · Commit", "repo · Repository", "ticket · Ticket"]);
});

test("an older under: answer that lands late does not replace a newer one", async () => {
  const pending = new Map<string, (v: FilterSuggestions) => void>();
  const filterSuggestions = vi.fn(
    (under?: string) =>
      new Promise<FilterSuggestions>((resolve) => pending.set(under ?? "", resolve)),
  );
  open({
    ports: { launcherHome: async () => HOME, search: async () => response(), filterSuggestions },
  });
  await settle();
  await typeInto("under:r");
  await typeInto("under:ru");
  pending.get("ru")!({ ...SUGGESTIONS, parents: [{ name: "Runbooks Overview", kind: "page" }] });
  await settle();
  pending.get("r")!({ ...SUGGESTIONS, parents: [{ name: "rust", kind: "repo" }] });
  await settle();
  expect(offered()).toEqual(["Runbooks Overview · page"]);
});

/* ── #746: a read that is waiting or failed is never an empty state ───── */

function typeInBox(raw: string) {
  const input = target.querySelector("input")!;
  input.value = raw;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

test("a source: token is not 'no source matches' while the sources are unread", async () => {
  const unknown = (raw: string) =>
    response({
      interpreted: { ...response().interpreted, text: raw, unknown_tokens: ["source:cf"] },
    });
  open({
    sources: [],
    sourcesRead: { phase: "failed", error: "down", retry: () => {} },
    ports: { launcherHome: async () => HOME, search: async () => unknown("x") },
  });
  await settle();
  typeInBox("source:cf x");
  await debounced();
  expect(target.textContent).toContain("source:cf");
  expect(target.textContent).not.toContain("no source matches");
});

test("a launcher_home that is rejected shows the reason and a retry, not an empty board", async () => {
  let fail = true;
  open({
    ports: {
      search: async () => response(),
      launcherHome: async () => {
        if (fail) throw { code: "internal", message: "database is busy" };
        return HOME;
      },
    },
  });
  await settle();
  expect(target.textContent).toContain("database is busy");
  expect(target.textContent).not.toContain("Nothing has been synced yet");
  expect(target.textContent).not.toContain("No source is configured yet");
  fail = false;
  [...target.querySelectorAll("button")].find((b) => b.textContent?.trim().startsWith("Retry"))!.click();
  await settle();
  expect(target.textContent).toContain("Reconcile mandates");
});

test("a search that never answers says Searching, never 'Nothing matches'", async () => {
  open({
    ports: { launcherHome: async () => HOME, search: () => new Promise<never>(() => {}) },
  });
  await settle();
  typeInBox("sepa");
  await debounced();
  expect(target.textContent).toContain("Searching");
  expect(target.textContent).not.toContain("Nothing matches");
});

test("a rejected search shows the reason, never 'Nothing matches'", async () => {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () => {
        throw { code: "internal", message: "index is rebuilding" };
      },
    },
  });
  await settle();
  typeInBox("sepa");
  await debounced();
  expect(target.textContent).toContain("index is rebuilding");
  expect(target.textContent).not.toContain("Nothing matches");
});

// ── The selected row stays on screen (#1411) ─────────────────────────────────────

/**
 * Every `scrollIntoView` the launcher asked for, with the element and the
 * options. jsdom draws nothing and has no `scrollIntoView`, so the stub is the
 * only witness; the screenshot in #1411's PR is the other one.
 */
function recordScrolls() {
  const calls: { el: Element; options: ScrollIntoViewOptions | boolean | undefined }[] = [];
  Element.prototype.scrollIntoView = function (this: Element, options?: ScrollIntoViewOptions | boolean) {
    calls.push({ el: this, options });
  };
  return calls;
}

/** A recent item on the board. */
function recent(entity_id: string, title: string): LauncherHome["recent"][number] {
  return { entity_id, kind: "ticket", source_id: "jira", title, updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: null };
}

/**
 * The board with three recent items, each with its sheet: *Link to* the
 * open job, *Start timer* and the ticket's own.
 */
function openSaved() {
  open({
    openEntity: OPEN,
    ontimer: vi.fn(),
    ports: {
      launcherHome: async () => ({
        ...HOME,
        recent: [recent("jira:PAY-240", "Reconcile mandates"), recent("jira:PAY-241", "Batch cut-off"), recent("jira:PAY-242", "Return codes")],
      }),
      search: async () => response(),
    },
  });
}

/** The results listbox's options in flat-list order. */
function resultOptions(): Element[] {
  return [...target.querySelectorAll('[aria-label="Results"] [role="option"]')];
}

test("the arrows scroll the newly active row into view, and only as far as it needs", async () => {
  openSaved();
  await settle();
  const scrolls = recordScrolls();

  press("ArrowDown");
  press("ArrowDown");
  const rows = resultOptions();
  expect(scrolls.at(-1)?.el).toBe(rows[2]);
  expect(scrolls.at(-1)?.options).toEqual({ block: "nearest" });

  press("ArrowUp");
  expect(scrolls.at(-1)?.el).toBe(rows[1]);
  expect(scrolls.at(-1)?.options).toEqual({ block: "nearest" });
});

// ── The box is a combobox (#1665, FE-1) ────────────────────────────────────
//
// Focus never leaves the input, so the input is where a screen reader looks
// for the active option: an `aria-activedescendant` on a listbox that never
// has focus is announced by nothing. Every assertion below reads the box, not
// the list.

/** The launcher's input, which carries the combobox's attributes. */
function box(): HTMLInputElement {
  return target.querySelector<HTMLInputElement>('input[aria-label="Search or act"]')!;
}

function listbox(label: string): Element {
  return target.querySelector(`[role="listbox"][aria-label="${label}"]`)!;
}

test("the box is a combobox that names the active result, and names nothing with no row", async () => {
  openSaved();
  await settle();
  const results = listbox("Results");
  const named = () => box().getAttribute("aria-activedescendant");
  const active = () => target.querySelector('[aria-label="Results"] [aria-selected="true"]');

  expect(box().getAttribute("role")).toBe("combobox");
  expect(box().getAttribute("aria-expanded")).toBe("true");
  expect(box().getAttribute("aria-autocomplete")).toBe("list");
  expect(results.id).not.toBe("");
  expect(box().getAttribute("aria-controls")).toBe(results.id);
  // On the box and only there: a second copy on the list is a second answer.
  expect(results.hasAttribute("aria-activedescendant")).toBe(false);

  expect(named()).toBeTruthy();
  expect(named()).toBe(active()?.id);
  for (const key of ["ArrowDown", "ArrowDown", "ArrowUp"]) {
    press(key);
    expect(named(), key).toBe(active()?.id);
  }
  expect(named()).toBe(resultOptions()[1]!.id);
  // Every option has an id of its own, so the name points at exactly one row.
  const ids = resultOptions().map((el) => el.id);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids.every((id) => id !== "")).toBe(true);

  unmount(app!);
  app = undefined;
  open({
    ports: {
      launcherHome: async () => ({ ...HOME, recent: [] }),
      search: async () => response(),
    },
  });
  await settle();
  expect(box().hasAttribute("aria-activedescendant")).toBe(false);
  expect(box().getAttribute("aria-controls")).toBe(listbox("Results").id);
});

test("the Tab sheet's active row is scrolled into view and named by the box", async () => {
  openSaved();
  await settle();
  press("ArrowDown");
  const row = box().getAttribute("aria-activedescendant");
  const scrolls = recordScrolls();
  press("Tab");
  const chain = listbox("Actions on the selected result");
  const rows = [...chain.querySelectorAll('[role="option"]')];
  expect(rows.length).toBeGreaterThan(1);
  // The box controls the list the arrows now walk.
  expect(chain.id).not.toBe("");
  expect(box().getAttribute("aria-controls")).toBe(chain.id);
  expect(box().getAttribute("aria-activedescendant")).toBe(rows[0]!.id);
  expect(chain.hasAttribute("aria-activedescendant")).toBe(false);
  expect(scrolls.at(-1)).toEqual({ el: rows[0], options: { block: "nearest" } });

  press("ArrowDown");
  expect(box().getAttribute("aria-activedescendant")).toBe(rows[1]!.id);
  expect(scrolls.at(-1)).toEqual({ el: rows[1], options: { block: "nearest" } });

  press("ArrowUp");
  expect(box().getAttribute("aria-activedescendant")).toBe(rows[0]!.id);
  expect(scrolls.at(-1)).toEqual({ el: rows[0], options: { block: "nearest" } });

  // Leaving the sheet hands the box back to the results, on the same row.
  press("Tab");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  expect(box().getAttribute("aria-controls")).toBe(listbox("Results").id);
  expect(box().getAttribute("aria-activedescendant")).toBe(row);
});

test("while value suggestions are open the box names the suggestion the arrows walk", async () => {
  openSuggesting();
  await settle();
  await typeInto("kind:");
  const list = listbox("Suggestions");
  const options = [...list.querySelectorAll('[role="option"]')];
  expect(options).toHaveLength(3);
  expect(list.id).not.toBe("");
  expect(box().getAttribute("aria-controls")).toBe(list.id);
  expect(box().getAttribute("aria-activedescendant")).toBe(options[0]!.id);

  press("ArrowDown");
  expect(box().getAttribute("aria-activedescendant")).toBe(options[1]!.id);
  expect(new Set(options.map((el) => el.id)).size).toBe(3);

  // Esc closes the list, and the box goes back to naming the results.
  press("Escape");
  expect(target.querySelector('[aria-label="Suggestions"]')).toBeNull();
  expect(box().getAttribute("aria-controls")).toBe(listbox("Results").id);
});

test("typing resets the selection to the first row, and that row is scrolled back into view", async () => {
  open();
  await settle();
  press("ArrowDown");
  press("ArrowDown");
  const scrolls = recordScrolls();
  await typeInto("sepa");
  await debounced();
  const first = resultOptions()[0];
  expect(first?.textContent).toContain("Retry failed SEPA payouts");
  expect(scrolls.at(-1)).toEqual({ el: first, options: { block: "nearest" } });

  // A new answer with the first row already selected brings it back too.
  scrolls.length = 0;
  await typeInto("sepa payouts");
  await debounced();
  expect(scrolls.at(-1)).toEqual({ el: resultOptions()[0], options: { block: "nearest" } });
});

/** The `>` palette, open with every action: the answer says the prefix is `>`. */
async function openPalette() {
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({ interpreted: { ...response().interpreted, prefix: "action", text: "" }, groups: [] }),
    },
  });
  await settle();
  await search(">");
  expect(target.querySelector('[aria-label="Results"] .tf-ln-h')?.textContent).toContain("Go to");
}

test("the palette's rows are followed as well", async () => {
  await openPalette();
  const scrolls = recordScrolls();
  press("ArrowDown");
  const rows = resultOptions();
  expect(rows.length).toBeGreaterThan(1);
  expect(scrolls.at(-1)).toEqual({ el: rows[1], options: { block: "nearest" } });
  expect(box().getAttribute("aria-activedescendant")).toBe(rows[1]!.id);
});

// ── `>` is the app's navigation menu (#1417) ───────────────────────────────

/** The palette, answered as the backend answers `>` followed by `text`. */
function openPaletteOn(text: string, props: Record<string, unknown> = {}) {
  open({
    ...props,
    ports: {
      launcherHome: async () => HOME,
      search: async () =>
        response({ interpreted: { ...response().interpreted, prefix: "action", text }, groups: [] }),
    },
  });
}

function paletteLabels(): string[] {
  return resultOptions().map((row) => row.querySelector(".tf-ln-nm")?.textContent ?? "");
}

test("> with nothing after it lists every section, then Settings and its sections", async () => {
  openPaletteOn("");
  await settle();
  await search(">");
  const labels = paletteLabels();
  expect(labels).toHaveLength(22);
  expect(labels.slice(0, 9)).toEqual([
    "Tickets",
    "Inbox",
    "Code",
    "Builds",
    "Docs",
    "Monitors",
    "Link sets",
    "Time",
    "Settings",
  ]);
  expect(labels.at(-1)).toBe("Settings › Logs and database");
  expect(labels).not.toContain("Job");
});

test(">In puts Inbox first, and >inbox draws Inbox alone", async () => {
  openPaletteOn("In");
  await settle();
  await search(">In");
  expect(paletteLabels()[0]).toBe("Inbox");
  if (app) unmount(app);
  openPaletteOn("inbox");
  await settle();
  await search(">inbox");
  expect(paletteLabels()).toEqual(["Inbox"]);
});

test("pressing a palette row navigates to its address and closes the launcher", async () => {
  const onnavigate = vi.fn();
  const onclose = vi.fn();
  openPaletteOn("", { onnavigate, onclose });
  await settle();
  await search(">");
  const row = resultOptions().find((option) => option.querySelector(".tf-ln-nm")?.textContent === "Settings › Time");
  (row as HTMLElement).click();
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/settings/time");
  expect(onclose).toHaveBeenCalledTimes(1);

  if (app) unmount(app);
  onnavigate.mockClear();
  openPaletteOn("", { onnavigate, onclose });
  await settle();
  await search(">");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/inbox");
});

// ── The chain rows and the suggestions keep their own layout (#1414) ───────

/**
 * The launcher's stylesheet (`tf/css/launcher.css` since #1422), put into the
 * document so jsdom computes the cascade the WebView would.
 */
function launcherStyles(): HTMLStyleElement {
  const css = readFileSync(join(process.cwd(), "src/lib/tf/css/launcher.css"), "utf8");
  expect(css, "the launcher has a stylesheet").toContain(".tf-ln-opt");
  // Its rules on a button name `.tf`, where the launcher is drawn.
  target.classList.add("tf");
  const style = document.createElement("style");
  style.textContent = css!;
  document.head.append(style);
  return style;
}

test("a value suggestion draws inline, a sheet's action keeps two columns like a prefix row", async () => {
  const style = launcherStyles();
  try {
    await openPalette();
    const prefix = target.querySelector<HTMLElement>('[aria-label="Results"] .tf-ln-opt');
    expect(prefix, "the palette draws prefix rows").not.toBeNull();
    expect(getComputedStyle(prefix!).display).toBe("grid");
    // The name spans the tag and the key cell, so what follows starts where a result's title does (F-625).
    expect(getComputedStyle(prefix!).gridTemplateColumns).toBe("minmax(var(--ln-name), max-content) minmax(0, 1fr)");

    unmount(app!);
    app = undefined;
    openSuggesting();
    await settle();
    await typeInto("kind:");
    const suggestion = target.querySelector<HTMLElement>('[aria-label="Suggestions"] [role="option"]')!;
    expect(suggestion.classList.contains("tf-ln-opt"), "the suggestion is a prefix row underneath").toBe(true);
    expect(getComputedStyle(suggestion).display).toBe("flex");

    unmount(app!);
    app = undefined;
    openSaved();
    await settle();
    press("ArrowDown");
    press("Tab");
    const action = target.querySelector<HTMLElement>(".tf-ln-sheet button[role='option']")!;
    expect(action.classList.contains("tf-ln-opt"), "a sheet action is a prefix row").toBe(true);
    expect(getComputedStyle(action).gridTemplateColumns).toBe("minmax(var(--ln-name), max-content) minmax(0, 1fr)");
  } finally {
    style.remove();
  }
});

// ── ⌘K always opens empty; a section's keys open with its filter (#2425) ──

/** Close by the ×, which leaves the session's query in place (Esc empties it first). */
function closeByX() {
  target.querySelector<HTMLButtonElement>('.tf-ln-q button[aria-label="Close"]')!.click();
  flushSync();
}

/** ⌘K, as the window hears it. */
async function reopen() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  flushSync();
  await settle();
}

test("⌘K opens with an empty box every time, never the last query (#2425)", async () => {
  const searched: string[] = [];
  open({ ports: { search: async (query: SearchQuery) => (searched.push(query.raw), response()), launcherHome: async () => HOME } });
  await settle();
  await search("PAY-231");
  closeByX();
  expect(target.querySelector(".tf-launch"), "the × closed the overlay").toBeNull();
  await reopen();

  const input = target.querySelector("input")!;
  expect(input.value).toBe("");
  expect(document.activeElement).toBe(input);
  expect(target.textContent, "the board, not the last answer").not.toContain("Retry failed SEPA payouts");

  await search("ledger");
  closeByX();
  await reopen();
  expect(target.querySelector("input")!.value, "and again on the next opening").toBe("");
  expect(searched).toEqual(["PAY-231", "ledger"]);
});

test("opened with a section's filter, the query holds it and the box draws its section's name as a chip (F-008, #2425)", async () => {
  const searched: string[] = [];
  open({
    query: "#b ",
    ports: { search: async (query: SearchQuery) => (searched.push(query.raw), response()), launcherHome: async () => HOME },
  });
  await settle();
  await debounced();
  const input = target.querySelector("input")!;
  expect(input.value, "no grammar in the box").toBe("");
  expect(input.placeholder).toBe("Search builds");
  expect(target.querySelector(".tf-ln-seed")?.textContent?.trim()).toBe("Builds");
  expect(document.activeElement).toBe(input);
  expect(searched).toEqual(["#b "]);

  // ⌘K after that is the global search: empty, whatever the last opening held.
  closeByX();
  await reopen();
  expect(target.querySelector("input")!.value).toBe("");
  expect(target.querySelector(".tf-ln-seed")).toBeNull();
});

test("words typed after the chip are asked under its filter, and the box holds only the words", async () => {
  const searched: string[] = [];
  open({
    query: "#rp ",
    ports: { search: async (query: SearchQuery) => (searched.push(query.raw), response()), launcherHome: async () => HOME },
  });
  await settle();
  await debounced();
  typeInBox("ledger");
  await debounced();
  expect(target.querySelector("input")!.value).toBe("ledger");
  expect(searched.at(-1)).toBe("#rp ledger");
});

test("Esc takes the words and then closes, so a seeded box with no words closes in one press (F-008)", async () => {
  const onclose = vi.fn();
  open({ query: "#b ", onclose, ports: { search: async () => response(), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  typeInBox("ledger");
  await debounced();
  press("Escape");
  await settle();
  expect(target.querySelector("input")!.value).toBe("");
  expect(target.querySelector(".tf-ln-seed"), "the chip outlives the words").not.toBeNull();
  expect(onclose).not.toHaveBeenCalled();
  press("Escape");
  await settle();
  expect(onclose).toHaveBeenCalledOnce();
});

test.each([
  ["Backspace on the empty box", () => press("Backspace")],
  ["its ×", () => target.querySelector<HTMLButtonElement>(".tf-ln-seedx")!.click()],
])("%s takes the chip off, and the box is the whole search again (F-008)", async (_, remove) => {
  open({ query: "#b ", ports: { search: async () => response(), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  remove();
  await settle();
  expect(target.querySelector(".tf-ln-seed")).toBeNull();
  expect(target.querySelector("input")!.placeholder).toBe("Search tickets, code, builds and docs");
});

test("the chip's × with words in the box searches everything for those words, and keeps them (the live run, 2026-11-09)", async () => {
  const searched: string[] = [];
  open({ query: "#t ", ports: { search: async (query: SearchQuery) => (searched.push(query.raw), response()), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  typeInBox("sepa");
  await debounced();
  expect(searched.at(-1)).toBe("#t sepa");
  target.querySelector<HTMLButtonElement>(".tf-ln-seedx")!.click();
  await settle();
  expect(target.querySelector(".tf-ln-seed")).toBeNull();
  expect(target.querySelector("input")!.value).toBe("sepa");
  expect(searched.at(-1)).toBe("sepa");
});

test("Backspace with words in the box edits them and leaves the chip", async () => {
  open({ query: "#b ", ports: { search: async () => response(), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  typeInBox("le");
  await debounced();
  press("Backspace");
  await settle();
  expect(target.querySelector(".tf-ln-seed")).not.toBeNull();
});

test("text that leads with its own panel goes in the box whole, not under the chip", async () => {
  const searched: string[] = [];
  open({ query: "#b ", ports: { search: async (query: SearchQuery) => (searched.push(query.raw), response()), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  typeInBox(">");
  await debounced();
  expect(searched.at(-1)).toBe(">");
  expect(target.querySelector(".tf-ln-seed")).toBeNull();
});

test("the bar under the box does not repeat the kinds the chip names", async () => {
  const interpreted = {
    text: "",
    prefix: null,
    filters: { sources: [], kinds: ["repo", "pr"], updated_within_days: null, mine: true, authors: [] },
    unknown_tokens: [],
    left_out_tokens: [],
  };
  open({ query: "#rp ", ports: { search: async () => response({ interpreted }), launcherHome: async () => HOME } });
  await settle();
  await debounced();
  const bar = [...target.querySelectorAll(".tf-ln-fbar .tf-ln-fchip")].map((chip) => chip.textContent);
  expect(bar).toEqual(["@me"]);
});

/** An answer of no hits to `text`, with nothing else narrowing it. */
const noHits = (text: string) =>
  response({
    groups: [],
    interpreted: { text, prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
  });

const optionNames = () => [...target.querySelectorAll('[role="option"]')].map((row) => (row.querySelector(".tf-ln-nm") ?? row).textContent?.trim());
const onRow = () => target.querySelector('[role="option"].tf-ln-on .tf-ln-nm')?.textContent?.trim();

test("a place's name is offered above the answer, on part of a word (F-603)", async () => {
  const onnavigate = vi.fn();
  open({ onnavigate, ports: { search: async (query: SearchQuery) => noHits(query.raw), launcherHome: async () => HOME } });
  await settle();
  await search("sett");
  expect(optionNames()[0]).toBe("Settings");
  expect(target.querySelector(".tf-ln-h")?.textContent?.trim()).toBe("Go to");
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/settings");
});

test("a place stays on top of a search that found things, and a hit is one ↓ away (F-603)", async () => {
  const onnavigate = vi.fn();
  const words = "code";
  open({
    onnavigate,
    ports: {
      search: async () => {
        const answer = response(EVERY_KIND);
        return { ...answer, interpreted: { ...answer.interpreted, text: words } };
      },
      launcherHome: async () => HOME,
    },
  });
  await settle();
  await search(words);
  expect(optionNames()[0]).toBe("Code");
  expect(onRow()).toBe("Code");
  press("ArrowDown");
  await settle();
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith(expect.stringContaining("PAY-231"));
});

test("Settings' sections are offered by their own names, and by what they hold where nothing answered (F-603)", async () => {
  open({ ports: { search: async (query: SearchQuery) => noHits(query.raw), launcherHome: async () => HOME } });
  await settle();
  await search("appearance");
  expect(optionNames()).toContain("Settings › Appearance");
  await search("theme");
  expect(optionNames()).toContain("Settings › Appearance");
  // Not fourteen rows for the one word.
  await search("settings");
  expect(optionNames().filter((name) => name?.startsWith("Settings"))).toEqual(["Settings"]);
});

test("a filter over an answer offers no place: the reader is narrowing, not naming one", async () => {
  const held = hit({ entity_id: "teamcity:Settings_Build", kind: "pipeline", title: "Settings_Build" });
  open({
    query: "#b ",
    ports: {
      search: async () =>
        response({
          ...noHits("settings"),
          groups: [{ kind: "pipeline", label: "Pipeline", plural: "Pipelines", monogram: "PL", total: 1, hits: [held] }],
        }),
      launcherHome: async () => HOME,
    },
  });
  await settle();
  await debounced();
  typeInBox("settings");
  await debounced();
  expect(optionNames().join(" ")).toContain("Settings_Build");
  expect(optionNames()).toHaveLength(1);
});

test("a filter that finds nothing offers the place the words name, over Search everything, and says what it searched (F-042)", async () => {
  open({
    query: "#rp ",
    ports: {
      search: async () => noHits("asset"),
      launcherHome: async () => HOME,
    },
  });
  await settle();
  await debounced();
  typeInBox("asset");
  await debounced();
  expect(optionNames()).toEqual(["Asset tree", "Search everything"]);
  expect(onRow()).toBe("Asset tree");
  expect(target.querySelector(".tf-ln-none")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Nothing in Code matches asset.");
});

test("a filter typed by hand that finds nothing makes no claim about everything (F-042)", async () => {
  open({ ports: { search: async () => noHits("asset"), launcherHome: async () => HOME } });
  await settle();
  typeInBox("#b asset");
  await debounced();
  expect(optionNames()).toEqual(["Asset tree", "Search everything"]);
  expect(target.querySelector(".tf-ln-none")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Nothing matches asset.");
});

test("nothing found: the line says so under the place, and ↑↓ reach Search everything (F-602)", async () => {
  open({ scope: createScope(() => null), ports: { search: async (query: SearchQuery) => noHits(query.raw), launcherHome: async () => HOME } });
  await settle();
  await search("settings");
  expect(optionNames()).toEqual(["Settings", "Search everything"]);
  expect(onRow()).toBe("Settings");
  const line = target.querySelector(".tf-ln-none")!;
  expect(line.textContent).toContain("No tickets, code, builds or docs match");
  expect(line.compareDocumentPosition(target.querySelector('[role="option"]')!) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
  press("ArrowDown");
  await settle();
  expect(onRow()).toBe("Search everything");
  press("ArrowUp");
  await settle();
  expect(onRow()).toBe("Settings");
});

const footerWords = () => [...target.querySelectorAll(".tf-launch > footer > span")].map((hint) => hint.textContent?.trim());

test("the footer says what ↵ does: go to a place, search everything, or save a name (F-607)", async () => {
  open({ scope: createScope(() => null), ports: { search: async (query: SearchQuery) => noHits(query.raw), launcherHome: async () => HOME } });
  await settle();
  await search("settings");
  expect(footerWords()).toContain("↵ go to Settings");
  press("ArrowDown");
  await settle();
  expect(footerWords()).toContain("↵ search everything");
});

test("the ⌘? overlay has its own legend and no band for one (F-607)", async () => {
  open();
  await settle();
  press("?", { metaKey: true });
  await settle();
  expect(target.querySelector(".tf-ln-keys")).not.toBeNull();
  expect(target.querySelector(".tf-launch > footer")).toBeNull();
});

/** A `#d` answer: pages only, as the engine sends it for `#d ledger retry`. */
function pagesAnswer(kinds: string[]): SearchResponse {
  return response({
    interpreted: {
      text: "ledger retry",
      prefix: null,
      filters: { sources: [], kinds, updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [
      {
        kind: "page",
        label: "Page",
        plural: "Pages",
        monogram: "PG",
        total: 1,
        hits: [hit({ entity_id: "confluence:7", kind: "page", title: "Ledger retries", source_id: "confluence" })],
      },
    ],
    total: 1,
  });
}

test("a #d answer ends with Search Docs for its words, which opens the Docs area searching them (#2425)", async () => {
  const onnavigate = vi.fn();
  open({ onnavigate, ports: { search: async () => pagesAnswer(["page"]), launcherHome: async () => HOME } });
  await settle();
  await search("#d ledger retry");
  const rows = [...target.querySelectorAll('[role="option"]')];
  expect(rows.at(-1)?.textContent).toContain("Search Docs for “ledger retry”");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/docs/search/ledger%20retry");
});

test("an answer that is not #d, or #d with no words, has no Search Docs row (#2425)", async () => {
  let answer = pagesAnswer([]);
  open({ ports: { search: async () => answer, launcherHome: async () => HOME } });
  await settle();
  await search("ledger retry");
  expect(target.textContent).not.toContain("Search Docs for");
  answer = { ...pagesAnswer(["page"]), interpreted: { ...pagesAnswer(["page"]).interpreted, text: "" } };
  await search("#d ");
  expect(target.textContent).not.toContain("Search Docs for");
});

test("an empty box opens with the caret in it and nothing to select", async () => {
  open();
  await settle();
  const input = target.querySelector("input")!;
  expect(document.activeElement).toBe(input);
  expect(input.selectionStart).toBe(0);
  expect(input.selectionEnd).toBe(0);
});

/**
 * The row is three cells (#2421): the kind's monogram, a key cell a person
 * reads, and the title. One group per kind here, each with one hit whose
 * cell rule is the kind's.
 */
function oneOf(kind: string, label: string, monogram: string, over: Parameters<typeof hit>[0]) {
  return { kind, label, plural: `${label}s`, monogram, total: 1, hits: [hit(over)] };
}

const EVERY_KIND = {
  groups: [
    oneOf("ticket", "Ticket", "TK", { entity_id: "jira:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", state: { text: "In Progress", tone: "info" } }),
    oneOf("pr", "Pull request", "PR", { entity_id: "gitea:tidewater/payout-service#142", kind: "pr", title: "Backoff on SEPA retries", path: "tidewater/payout-service", state: { text: "merged", tone: "ok" } }),
    oneOf("repo", "Repository", "RP", { entity_id: "gitea:tidewater/payout-service", kind: "repo", title: "tidewater/payout-service" }),
    oneOf("page", "Page", "PG", { entity_id: "confluence:98307", kind: "page", title: "SEPA retry design", path: "Engineering › Payments" }),
    oneOf("pipeline", "Pipeline", "PL", { entity_id: "teamcity:Payout_Build", kind: "pipeline", title: "Build", path: "Payments › Payout service", state: { text: "FAILURE", tone: "err" } }),
    oneOf("monitor", "Monitor", "MO", { entity_id: "kuma:8", kind: "monitor", title: "payout health", path: "http", state: { text: "down", tone: "err" } }),
    oneOf("view", "View", "VW", { entity_id: "knobas:view-3", kind: "view", title: "My open SEPA work", path: "Payments · status:open" }),
    oneOf("note", "Note", "NT", { entity_id: "note:7a1", kind: "note", title: "Ask Jonas about the cut-off" }),
  ],
  total: 8,
};

/** Each row's three cells, as text: monogram, key cell, title. */
function cells() {
  return [...target.querySelectorAll(".tf-ln-res")].map((row) =>
    [...row.children].map((cell) => cell.textContent?.trim()),
  );
}

test("a ticket whose key another row holds says its source, and the others don't (#1476, F-636)", async () => {
  const twin = {
    groups: [
      {
        kind: "ticket",
        label: "Ticket",
        plural: "Tickets",
        monogram: "TK",
        total: 3,
        hits: [
          hit({ entity_id: "jira:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", source_id: "jira" }),
          hit({ entity_id: "mock:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", source_id: "mock" }),
          hit({ entity_id: "jira:PAY-228", kind: "ticket", title: "Ledger", source_id: "jira" }),
        ],
      },
    ],
    total: 3,
  };
  open({ ports: { search: async () => response(twin), launcherHome: async () => HOME } });
  await settle();
  await search("pay");
  expect(cells()).toEqual([
    ["Tk", "PAY-231", "Retry failed SEPA payouts", "jira"],
    ["Tk", "PAY-231", "Retry failed SEPA payouts", "mock"],
    ["Tk", "PAY-228", "Ledger"],
  ]);
});

test("a result row is three cells, the key cell following the kind's rule", async () => {
  open({ ports: { search: async () => response(EVERY_KIND), launcherHome: async () => HOME } });
  await settle();
  await search("sepa");

  expect(cells()).toEqual([
    ["Tk", "PAY-231", "Retry failed SEPA payouts"],
    ["Pr", "payout-service", "#142 Backoff on SEPA retries"],
    // The owner is the cell, so the repository's own name is the title.
    ["Rp", "tidewater", "payout-service"],
    ["Pg", "Engineering", "SEPA retry design"],
    ["Pl", "Payout service", "Build"],
    ["Mo", "http", "payout health"],
    ["Vw", "Payments", "My open SEPA work"],
    // Nothing better known: the kind's word.
    ["Nt", "note", "Ask Jonas about the cut-off"],
  ]);
  // Only a ticket's key is an identifier, drawn in the mono face.
  const mono = [...target.querySelectorAll(".tf-ln-res .tf-ln-k")].map((cell) => cell.classList.contains("tf-ln-mono"));
  expect(mono).toEqual([true, false, false, false, false, false, false, false]);
});

/**
 * The engine's key cell (#2452): a hit or a recent row that carries `cell`
 * draws it, ahead of what its path line says. A monitor's type is on no path
 * line, and a page's space is not its top ancestor page.
 */
test("a row draws the key cell the engine sends, on a hit and on the board", async () => {
  open({
    ports: {
      search: async () =>
        response({
          groups: [
            oneOf("page", "Page", "PG", { entity_id: "confluence:98307", kind: "page", title: "SEPA retry design", path: "Handbook › Payments", cell: "Engineering" }),
            oneOf("monitor", "Monitor", "MO", { entity_id: "kuma:8", kind: "monitor", title: "payout health", path: null, cell: "http" }),
          ],
          total: 2,
        }),
      launcherHome: async () => ({
        ...HOME,
        recent: [
          { entity_id: "kuma:9", kind: "monitor", source_id: "kuma", title: "ledger health", updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: null, cell: "keyword" },
        ],
      }),
    },
  });
  await settle();
  expect(cells()).toEqual([["Mo", "keyword", "ledger health"]]);

  await search("health");
  expect(cells()).toEqual([
    ["Pg", "Engineering", "SEPA retry design"],
    ["Mo", "http", "payout health"],
  ]);
});

test("a row draws no second line, no provenance and no state chip", async () => {
  open({ ports: { search: async () => response(), launcherHome: async () => HOME } });
  await settle();
  await search("sepa");

  const row = target.querySelector(".tf-ln-res")!;
  expect(row.children).toHaveLength(3);
  expect(row.textContent).not.toContain("synced");
  expect(row.textContent, "the excerpt is not drawn").not.toContain("batch");
  expect(target.querySelectorAll(".tf-chip, .tf-ln-sy, .tf-ln-pa, .tf-ln-sn, mark")).toHaveLength(0);
});

test("the monogram takes the state's tone, and a row with no state is uncoloured", async () => {
  open({ ports: { search: async () => response(EVERY_KIND), launcherHome: async () => HOME } });
  await settle();
  await search("sepa");

  const monograms = [...target.querySelectorAll<HTMLElement>(".tf-ln-res .tf-ln-mg")];
  expect(monograms.map((mg) => mg.dataset["tone"] ?? null)).toEqual(["info", "ok", null, null, "err", "err", null, null]);
  // The colour is the only sign, so the state is named for a screen reader.
  expect(monograms[0]!.getAttribute("aria-label")).toBe("Ticket: In progress");
  expect(monograms[2]!.getAttribute("aria-label")).toBe("Repository");
});

test("headings carry no count and the footer no match count", async () => {
  open();
  await settle();
  await search("sepa");

  expect(target.querySelectorAll(".tf-ln-n, .tf-ln-more, .tf-ln-took")).toHaveLength(0);
  expect(target.querySelector("footer")?.textContent).not.toMatch(/match/);
});

test("the empty box draws the recent items alone, with the same three cells", async () => {
  open({
    inbox: { count: 2, items: [] },
    ports: {
      search: async () => response(),
      launcherHome: async () => ({
        ...HOME,
        recent: [
          { entity_id: "jira:PAY-240", kind: "ticket", source_id: "jira", title: "Reconcile mandates", updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: { text: "Done", tone: "ok" } },
          { entity_id: "gitea:tidewater/ledger", kind: "repo", source_id: "gitea", title: "tidewater/ledger", updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: null },
        ],
      }),
    },
  });
  await settle();

  const headings = [...target.querySelectorAll(".tf-ln-h .tf-ln-lab")].map((el) => el.textContent?.trim());
  expect(headings).toEqual(["Recent"]);
  expect(cells()).toEqual([
    ["Tk", "PAY-240", "Reconcile mandates"],
    ["Rp", "tidewater", "ledger"],
  ]);
  expect(target.querySelector<HTMLElement>(".tf-ln-res .tf-ln-mg")!.dataset["tone"]).toBe("ok");
  // No smart lists, inbox, time or source strip.
  expect(target.querySelectorAll(".tf-ln-sl, .tf-ln-strip, .tf-ln-src")).toHaveLength(0);
  expect(target.textContent).not.toContain("My items");
});

test("the box has no footnote, no ⌘K badge and no caret bar", async () => {
  open();
  await settle();

  const box = target.querySelector(".tf-ln-q")!;
  expect(box.querySelectorAll(".tf-ln-foot, kbd, .tf-ln-caret")).toHaveLength(0);
  expect(box.textContent).not.toContain("pending write");
});

/** One offered word of the *state colours* read, as `sources::state_colours::read` answers it. */
function word(over: Partial<StateWordColour> & { word: string }): StateWordColour {
  const declared_tone = over.declared_tone ?? null;
  const yours = over.yours ?? null;
  return {
    declared: over.declared ?? true,
    declared_tone,
    yours,
    tone: over.tone !== undefined ? over.tone : yours === null ? declared_tone : yours === "none" ? null : yours,
    origin: over.origin ?? (yours !== null ? "yours" : declared_tone !== null ? "adapter" : "unset"),
    ...over,
    category_tone: over.category_tone ?? null,
  };
}

/** The read the legend is drawn from: Jira's ticket words (two seen), Gitea's pull requests, Kuma's monitors. */
const COLOURS: KindStateColours[] = [
  {
    kind: "monitor",
    has_category: false,
    words: [
      word({ word: "up", declared_tone: "ok" }),
      word({ word: "down", declared_tone: "err" }),
      // The reader's: `pending` is loud for them.
      word({ word: "pending", declared_tone: "warn", yours: "err" }),
      word({ word: "maintenance", declared_tone: "muted" }),
    ],
  },
  {
    kind: "pr",
    has_category: false,
    words: [word({ word: "merged", declared_tone: "ok" }), word({ word: "open", declared_tone: "info" }), word({ word: "closed", declared_tone: "muted" })],
  },
  {
    kind: "ticket",
    has_category: true,
    words: [
      word({ word: "Done", declared_tone: "ok" }),
      // Set to *No colour*: it matches and draws no tone, so no legend line names it.
      word({ word: "In Progress", declared_tone: "info", yours: "none" }),
      word({ word: "Resolved", declared_tone: "ok" }),
      // Seen at Jira's open candidate; one coloured by the reader, one left alone.
      word({ word: "Blocked", declared: false, yours: "err" }),
      word({ word: "To Do", declared: false }),
    ],
  },
];

/** The legend as drawn: per kind its label and each tone's words. */
function legendDrawn() {
  const legend = target.querySelector('[aria-label="Colours"]');
  if (!legend) return null;
  return [...legend.querySelectorAll(".tf-ln-lg")].map((line) => ({
    kind: line.querySelector(".tf-ln-lgk")?.textContent?.trim(),
    tones: [...line.querySelectorAll<HTMLElement>(".tf-ln-mg")].map((mg) => [mg.dataset["tone"], mg.nextElementSibling?.textContent?.trim()]),
  }));
}

const HELP_SEARCH = async () =>
  response({
    interpreted: { text: "", prefix: "help", filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [], left_out_tokens: [] },
    groups: [],
  });

/** A legend's ports over a fixed read, with the `activity:new` handlers kept so a test can fire one. */
function legendFake(first: KindStateColours[]) {
  const fake = {
    answer: first,
    reads: 0,
    heard: [] as { event: string; handler: () => void }[],
    stateColours: async () => (fake.reads++, fake.answer),
    listen: async (event: string, handler: () => void) => {
      const entry = { event, handler };
      fake.heard.push(entry);
      return () => void fake.heard.splice(fake.heard.indexOf(entry), 1);
    },
    fire(event: string) {
      for (const { event: name, handler } of [...fake.heard]) if (name === event) handler();
    },
  };
  return fake;
}

/**
 * The `?` card ends with a colour legend per kind (#2421), built from the
 * merged map the *state colours* read answers (#2467, spec #2463), so it
 * explains the colours a row actually draws: the reader's overrides and the
 * seen words they coloured, and nothing for a word set to *No colour*.
 */
test("the ? card ends with a legend per kind, from the merged state colours", async () => {
  const legendPorts = legendFake(COLOURS);
  open({ ports: { launcherHome: async () => HOME, search: HELP_SEARCH }, legendPorts });
  await settle();
  await search("?");
  await settle();

  const legend = target.querySelector('[aria-label="Colours"]')!;
  expect(legend, "the legend is drawn").not.toBeNull();
  // After the syntax rows, never among them: nothing in it is an option.
  expect(legend.querySelectorAll('[role="option"]')).toHaveLength(0);
  expect(legendDrawn()).toEqual([
    // `pending` under the reader's `err`, and with it no `warn` line at all.
    { kind: "Monitors", tones: [["ok", "Up"], ["err", "Down, Pending"], ["muted", "Maintenance"]] },
    { kind: "Pull requests", tones: [["ok", "Merged"], ["info", "Open"], ["muted", "Closed"]] },
    { kind: "Tickets", tones: [["ok", "Done, Resolved"], ["err", "Blocked"]] },
  ]);
});

/**
 * Jira's status category colours every status no name maps (#2468), so the
 * ticket section does not list the seen words left at their category's tone:
 * it ends with one line saying so. A seen word the reader coloured is still
 * listed under the reader's tone.
 */
test("the ticket legend ends with the category line instead of the words at their category's tone", async () => {
  const withCategory = COLOURS.map((kind) =>
    kind.kind !== "ticket"
      ? kind
      : {
          ...kind,
          words: [
            ...kind.words,
            word({ word: "Ready for QA", declared: false, category_tone: "info", tone: "info", origin: "category" }),
            word({ word: "Shipped", declared: false, category_tone: "ok", tone: "ok", origin: "category" }),
            word({ word: "Parked", declared: false, category_tone: "info", yours: "warn" }),
          ],
        },
  );
  open({ ports: { launcherHome: async () => HOME, search: HELP_SEARCH }, legendPorts: legendFake(withCategory) });
  await settle();
  await search("?");
  await settle();

  expect(legendDrawn()?.find((line) => line.kind === "Tickets")?.tones).toEqual([
    ["ok", "Done, Resolved"],
    ["warn", "Parked"],
    ["err", "Blocked"],
  ]);
  const lines = [...target.querySelectorAll('[aria-label="Colours"] .tf-ln-lg')];
  const tickets = lines.find((line) => line.querySelector(".tf-ln-lgk")?.textContent?.trim() === "Tickets")!;
  expect(tickets.lastElementChild?.textContent?.trim()).toBe("other statuses: by Jira category");
  // No other kind reads a category, so none ends with the line.
  for (const line of lines.filter((l) => l !== tickets)) expect(line.textContent).not.toContain("by Jira category");
});

/**
 * The category line is the declaration's, not the corpus's (#2468): a
 * mirror that has seen no status in a toned category yet still ends the
 * ticket section with it, since the next status Jira adds is drawn by it;
 * a kind whose adapter reads no category never does.
 */
test("the ticket legend ends with the category line before any status is seen in a toned category", async () => {
  const unseen = COLOURS.map((kind) => (kind.kind !== "ticket" ? kind : { ...kind, words: kind.words.filter((w) => w.declared) }));
  open({ ports: { launcherHome: async () => HOME, search: HELP_SEARCH }, legendPorts: legendFake(unseen) });
  await settle();
  await search("?");
  await settle();

  const lines = [...target.querySelectorAll('[aria-label="Colours"] .tf-ln-lg')];
  const tickets = lines.find((line) => line.querySelector(".tf-ln-lgk")?.textContent?.trim() === "Tickets")!;
  expect(tickets.lastElementChild?.textContent?.trim()).toBe("other statuses: by Jira category");
  for (const line of lines.filter((l) => l !== tickets)) expect(line.textContent).not.toContain("by Jira category");
});

/**
 * A set or a clear while the card is open announces itself on `activity:new`
 * (#2465 Ruling 1), and the legend reads the merged map again: the reader
 * sees the word under its new tone without closing the card. The ⌘? overlay
 * draws the same legend from the same read.
 */
test("after a set while the card is open, the legend re-reads and shows the new tone", async () => {
  const legendPorts = legendFake(COLOURS);
  open({ ports: { launcherHome: async () => HOME, search: HELP_SEARCH }, legendPorts });
  await settle();
  press("?", { metaKey: true, shiftKey: true });
  await settle();
  expect(legendDrawn()?.find((line) => line.kind === "Pull requests")?.tones).toEqual([
    ["ok", "Merged"],
    ["info", "Open"],
    ["muted", "Closed"],
  ]);

  // The reader moves `closed` to `err` in Settings; the command emits.
  legendPorts.answer = COLOURS.map((kind) =>
    kind.kind === "pr" ? { ...kind, words: kind.words.map((w) => (w.word === "closed" ? word({ word: "closed", declared_tone: "muted", yours: "err" }) : w)) } : kind,
  );
  legendPorts.fire(EVENTS.activityNew);
  await settle();

  expect(legendDrawn()?.find((line) => line.kind === "Pull requests")?.tones).toEqual([
    ["ok", "Merged"],
    ["info", "Open"],
    ["err", "Closed"],
  ]);

  // The reader clears it again: the word goes back under the adapter's tone.
  legendPorts.answer = COLOURS;
  legendPorts.fire(EVENTS.activityNew);
  await settle();

  expect(legendDrawn()?.find((line) => line.kind === "Pull requests")?.tones).toEqual([
    ["ok", "Merged"],
    ["info", "Open"],
    ["muted", "Closed"],
  ]);
});

/** Closed, the launcher neither reads the colours nor listens for a change to them. */
test("a closed launcher stops listening, and an open one reads the colours only for the legend", async () => {
  const legendPorts = legendFake(COLOURS);
  open({ ports: { launcherHome: async () => HOME, search: HELP_SEARCH }, legendPorts });
  await settle();
  expect(legendPorts.reads, "the board draws no legend, so nothing is read").toBe(0);
  await search("?");
  await settle();
  expect(legendPorts.reads).toBe(1);
  expect(legendPorts.heard.map(({ event }) => event)).toEqual([EVENTS.activityNew]);

  press("Escape");
  press("Escape");
  await settle();
  expect(legendPorts.heard, "the listener is let go with the card").toEqual([]);
});

/** A storage that outlives a launcher, as the webview's does a restart. */
function memoryStorage() {
  const kept = new Map<string, string>();
  return {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, value),
    removeItem: (key: string) => void kept.delete(key),
  };
}

/** Words typed and the debounced search they send, settled. */
async function asked(text: string) {
  await typeInto(text);
  await debounced();
}

const chip = () => target.querySelector<HTMLButtonElement>(".tf-ln-scope button");
const footer = () => target.querySelector("footer")?.textContent ?? "";

test("Titles is the first choice, and the search says so with the titles flag", async () => {
  const sent: SearchQuery[] = [];
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => (sent.push(query), response()),
    },
  });
  await settle();
  await asked("pay-210");

  expect(sent.at(-1)).toMatchObject({ raw: "pay-210", titles: true });
  expect(chip()?.textContent?.trim()).toBe("Titles");
  // The footer names the other mode, the one ⌘D goes to.
  expect(footer()).toContain("⌘D everything");
});

test("⌘D flips to Everything and asks again at once, and flips back", async () => {
  const sent: SearchQuery[] = [];
  open({
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => (sent.push(query), response()),
    },
  });
  await settle();
  await asked("sepa");
  const before = sent.length;

  const event = press("d", { metaKey: true });
  await settle();
  // The webview's own ⌘D (a bookmark) never runs.
  expect(event.defaultPrevented).toBe(true);
  expect(sent).toHaveLength(before + 1);
  expect(sent.at(-1)).toMatchObject({ raw: "sepa", titles: false });
  expect(chip()?.textContent?.trim()).toBe("Everything");
  expect(footer()).toContain("⌘D titles only");

  press("d", { metaKey: true });
  await settle();
  expect(sent.at(-1)).toMatchObject({ raw: "sepa", titles: true });
  expect(chip()?.textContent?.trim()).toBe("Titles");
});

test("⌘D on an empty box flips the choice and sends nothing", async () => {
  const search = vi.fn(async () => response());
  open({ ports: { launcherHome: async () => HOME, search } });
  await settle();

  press("d", { metaKey: true });
  await settle();
  expect(chip()?.textContent?.trim()).toBe("Everything");
  expect(search).not.toHaveBeenCalled();
});

test("the chip flips it too, and the choice survives closing, reopening and a restart", async () => {
  const storage = memoryStorage();
  const scope = createScope(() => storage);
  open({ scope });
  await settle();
  chip()!.click();
  flushSync();
  expect(chip()?.textContent?.trim()).toBe("Everything");

  // Closed and opened again, in the same run.
  unmount(app!);
  open({ scope });
  await settle();
  expect(chip()?.textContent?.trim()).toBe("Everything");

  // A restart: a new choice read from what the webview kept.
  unmount(app!);
  const sent: SearchQuery[] = [];
  open({
    scope: createScope(() => storage),
    ports: {
      launcherHome: async () => HOME,
      search: async (query: SearchQuery) => (sent.push(query), response()),
    },
  });
  await settle();
  expect(chip()?.textContent?.trim()).toBe("Everything");
  await asked("sepa");
  expect(sent.at(-1)).toMatchObject({ titles: false });
});
