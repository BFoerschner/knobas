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
  Segment,
} from "../ipc";
import type { InboxItem } from "../ipc/entity";
import type { CredentialHealth, SourceSyncStatus } from "../ipc/sources";
import Launcher from "./Launcher.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

const NOW = new Date("2026-08-25T12:00:00Z");

function hit(over: {
  entity_id: string;
  kind: string;
  title: string;
  source_id?: string;
  snippet?: Segment[];
  synced_at?: string;
  path?: string | null;
}) {
  return {
    entity_id: over.entity_id,
    kind: over.kind,
    source_id: over.source_id ?? "jira",
    updated_at: null,
    synced_at: over.synced_at ?? "2026-08-25T11:56:00Z",
    path: over.path ?? null,
    state: null,
    title: over.title,
    rank: 1,
    snippet: over.snippet ?? [],
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
  smart_lists: [
    {
      id: "mine",
      label: "My items",
      count: 3,
      changed: true,
      description: "Yours.",
      saved: false,
      needs_attention: false,
    },
    {
      id: "just-synced",
      label: "Just synced",
      count: 9,
      changed: false,
      description: "New.",
      saved: false,
      needs_attention: false,
    },
  ],
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
  expect(row!.textContent).toContain('onclick="steal()"');
  // Nothing was parsed: no element the text merely *looks* like exists.
  expect(target.querySelectorAll("script")).toHaveLength(0);
  expect(target.querySelectorAll("img")).toHaveLength(0);
  expect(target.querySelectorAll("b")).toHaveLength(0);
  // The highlight is an element the component made, from the `hit` flag.
  expect(target.querySelectorAll("mark").length).toBeGreaterThan(0);
});

test("groups render in the engine's order, with the count and what the page left behind", async () => {
  open();
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const headings = [...target.querySelectorAll(".tf-ln-lab")].map((el) => el.textContent?.trim());
  expect(headings).toEqual(["Tickets", "Pull requests", "Builds"]);

  // 42 matched, one on the page: §4 wants both numbers or the reader believes
  // there is one ticket.
  const first = target.querySelector(".tf-ln-n")!;
  expect(first.textContent).toContain("42");
  expect(first.textContent).toContain("41 more");

  // And every row carries its provenance, which is what §4 asks for on all of
  // them rather than on the ones that happen to have room.
  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 4 min ago", "synced 4 min ago", "synced 4 min ago"]);
});

test("a source that is refusing the credential replaces the age on its rows", async () => {
  const sources: CredentialHealth[] = [
    {
      source_id: "gitea",
      state: "unauthorized",
      checked_at: null,
      detail: "401",
      secret_expires_at: null,
    },
    // `unknown` is 0002's default and means *never tested*. It must NOT
    // replace the age, or every row of a fresh install reads as broken.
    {
      source_id: "jira",
      state: "unknown",
      checked_at: null,
      detail: null,
      secret_expires_at: null,
    },
  ];
  open({ sources });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 4 min ago", "gitea · sign in again", "synced 4 min ago"]);
});

/**
 * The conflation the fallback introduced, now that a shell does supply the prop.
 *
 * `sources.length > 0 ? sources : home` reads an **empty** live list as
 * "unsupplied" and silently reaches for the board's per-opening copy instead.
 * That was correct while nothing supplied the prop, and it stops being correct
 * the moment `App.svelte` passes a live `source:health` store: a store that has
 * just been told the only source is fine — or that is still seeding — is
 * *supplied and empty*, and falling back to a stale board fetch is the launcher
 * drawing a 401 the backend has already retracted.
 *
 * `undefined` means unsupplied. `[]` means "I am watching, and there is
 * nothing to complain about."
 */
test("an explicitly empty sources prop is a live reading, not an unsupplied one", async () => {
  open({
    sources: [],
    ports: {
      search: async () => response(),
      launcherHome: async (): Promise<LauncherHome> => ({
        ...HOME,
        // The board's copy is stale: this 401 has already been resolved, and
        // the live store the shell supplies knows it.
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
    },
  });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 4 min ago", "synced 4 min ago", "synced 4 min ago"]);
});

/**
 * The same behaviour on the path production actually takes.
 *
 * The test above passes `sources` at mount. `App.svelte` does not — nothing in
 * `app/src` polls credential health in M1 — so for one review round the
 * component could replace the age with the credential complaint and never did
 * it in the shipped app: the test was green about a path production does not
 * take, which is the whole finding.
 *
 * This mounts the way `App.svelte` mounts, with **no `sources` prop at all**,
 * and asserts the complaint appears anyway — from `LauncherHome.sources`,
 * which the launcher already fetches on every opening.
 */
test("with no sources prop, the rows take their health from the board", async () => {
  open({
    ports: {
      search: async () => response(),
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
          // Still the `unknown` control: 0002's default must not read as a
          // fault, or a fresh install reports every source broken.
          {
            source_id: "jira",
            state: "unknown",
            checked_at: null,
            detail: null,
            secret_expires_at: null,
          },
        ],
      }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  // Three rows from three different sources, only the middle one failing, so
  // a fixture that could not witness a mix-up is not what is being read here.
  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 4 min ago", "gitea · sign in again", "synced 4 min ago"]);
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

/**
 * The footnote is one of the launcher's four accounts of the store, and the
 * count beside it is the board's own.
 *
 * A **non-zero** `pending_writes`, because `session.home?.pending_writes ?? 0`
 * renders the same `0` for a board that arrived with none and for a board that
 * never arrived — so on `HOME` as it stands a footnote wired to nothing at all
 * would still pass. The negative assertion in the paste-miss test below is not
 * a substitute for this one: it would accept *local cache* as happily as
 * *mirror*.
 */
test("the footnote names the store and counts the board's pending writes", async () => {
  open({ ports: { launcherHome: async () => ({ ...HOME, pending_writes: 4 }) } });
  await settle();
  expect(target.textContent).toContain("mirror \u00b7 4 pending writes");
});

test("the footnote counts one pending write in the singular", async () => {
  open({ ports: { launcherHome: async () => ({ ...HOME, pending_writes: 1 }) } });
  await settle();
  expect(target.textContent).toContain("mirror \u00b7 1 pending write");
  expect(target.textContent).not.toContain("1 pending writes");
});

test("an empty box shows the smart lists and the recent items", async () => {
  open();
  await settle();
  const text = target.textContent ?? "";
  expect(text).toContain("My items");
  expect(text).toContain("Just synced");
  expect(text).toContain("Reconcile mandates");
  // The change badge, which is the only thing distinguishing a list worth
  // opening from one that has not moved.
  expect(target.querySelector(".tf-ln-c.tf-ln-chg")?.textContent).toContain("3");
});

/**
 * The board strip and the chips are **one** reading, not two.
 *
 * The strip used to map `home.sources` — the per-opening `launcher_home` fetch
 * — while the chips above it were drawn from the live `source:health` store.
 * They shared `isActionable`, which is a shared *rule* over two sets of data,
 * and a comment in `Board.svelte` claimed on that basis that the two "cannot
 * come to different conclusions about the same source". They could: this test
 * hands the board a stale 401 the live store has already retracted, which is
 * exactly the shape of #27's finding one layer along.
 */
test("the board strip reads the live health, not the copy the board arrived with", async () => {
  open({
    // Supplied and disagreeing with the board's own copy: the store says gitea
    // is fine, the per-opening fetch still remembers a 401.
    sources: [
      {
        source_id: "gitea",
        state: "ok",
        checked_at: "2026-08-25T12:00:00Z",
        detail: null,
        secret_expires_at: null,
      },
    ],
    ports: {
      search: async () => response(),
      launcherHome: async () => ({
        ...HOME,
        sources: [
          {
            source_id: "gitea",
            state: "unauthorized",
            checked_at: "2026-08-25T11:00:00Z",
            detail: "401 from Gitea",
            secret_expires_at: null,
          },
        ],
      }),
    },
  });
  await settle();

  const strip = [...target.querySelectorAll(".tf-ln-strip .tf-ln-src")];
  expect(strip.length, "the strip did not render at all").toBe(1);
  expect(strip[0]!.className).not.toContain("fail");
  expect(strip[0]!.textContent).not.toContain("401 from Gitea");
});

/**
 * The recent rows read the same live health as the strip above them (#1420).
 *
 * They used to be handed `home.sources`, the copy the board arrived with, so a
 * 401 the live store had already cleared left the strip and stayed on a recent
 * row as *sign in again* until the next opening. The board's copy here still
 * remembers the 401 and the live store says the source is fine; both the strip
 * and the row must read the store. The `synced` line is the positive control:
 * a row that drew no provenance at all would pass the negative half.
 */
test("the recent rows read the live health, as the strip above them does", async () => {
  open({
    sources: [
      {
        source_id: "gitea",
        state: "ok",
        checked_at: "2026-08-25T12:00:00Z",
        detail: null,
        secret_expires_at: null,
      },
    ],
    ports: {
      search: async () => response(),
      launcherHome: async (): Promise<LauncherHome> => ({
        ...HOME,
        recent: [
          {
            entity_id: "gitea:build-7",
            kind: "build",
            source_id: "gitea",
            title: "payments main #7",
            updated_at: null,
            synced_at: "2026-08-25T11:58:00Z",
            path: null,
            state: null,
          },
        ],
        sources: [
          {
            source_id: "gitea",
            state: "unauthorized",
            checked_at: "2026-08-25T11:00:00Z",
            detail: "401 from Gitea",
            secret_expires_at: null,
          },
        ],
      }),
    },
  });
  await settle();

  const strip = [...target.querySelectorAll(".tf-ln-strip .tf-ln-src")];
  expect(strip.length, "the strip did not render at all").toBe(1);
  expect(strip[0]!.className).not.toContain("fail");
  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 2 min ago"]);
});

/**
 * The board's third section (spec §4: the empty box shows source health beside
 * the lists). Not selectable — a reading, not a destination — so nothing else
 * in this file would notice if it stopped being drawn.
 */
test("the board shows one line per source, and says which one needs attention", async () => {
  const sources: CredentialHealth[] = [
    {
      source_id: "jira",
      state: "ok",
      checked_at: "2026-08-25T11:00:00Z",
      detail: null,
      secret_expires_at: null,
    },
    {
      source_id: "gitea",
      state: "unauthorized",
      checked_at: "2026-08-25T11:00:00Z",
      detail: "401 from Gitea",
      secret_expires_at: null,
    },
  ];
  open({
    ports: {
      search: async () => response(),
      launcherHome: async () => ({ ...HOME, sources }),
    },
  });
  await settle();

  const lines = [...target.querySelectorAll(".tf-ln-src")].map((el) => el.textContent?.replace(/\s+/g, " ").trim());
  // `GI`, not `GT`: the monogram is the first two letters of the id, because
  // §3a forbids a table of adapter names. Written `GT` at first, from the
  // mockup's hand-curated map — the second time this fixture-vs-derivation gap
  // has been caught by a test rather than by reading the code.
  expect(lines).toEqual(["JI jira checked 1 h ago", "GI gitea 401 from Gitea"]);
  // Only the broken one is marked, and `ok` is not "needs attention".
  expect(target.querySelectorAll(".tf-ln-src.tf-ln-fail")).toHaveLength(1);
  expect(target.querySelector(".tf-ln-src.tf-ln-fail")?.textContent).toContain("gitea");
});

test("the strip marks a source whose last run ended in an error, as the source page reads it (#1581)", async () => {
  const health = (source_id: string): CredentialHealth => ({
    source_id,
    state: "ok",
    checked_at: "2026-08-25T11:00:00Z",
    detail: null,
    secret_expires_at: null,
  });
  const run = (source_id: string, last_outcome: "ok" | "error"): SourceSyncStatus => ({
    source_id,
    running: false,
    run_id: 1,
    started_at: null,
    last_finished_at: "2026-08-25T11:30:00Z",
    last_outcome,
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
  });
  open({
    sources: [
      health("jira"),
      health("teamcity"),
      { ...health("gitea"), state: "unreachable", detail: "connection refused" },
    ],
    runs: [run("jira", "ok"), run("teamcity", "error"), run("gitea", "ok")],
  });
  await settle();

  const lines = [...target.querySelectorAll(".tf-ln-src")].map((el) => el.textContent?.replace(/\s+/g, " ").trim());
  expect(lines).toEqual([
    "JI jira checked 1 h ago",
    "TE teamcity Failed 30 min ago",
    "GI gitea connection refused",
  ]);
  // Warn, not the credential's err: the credential is fine.
  expect(target.querySelectorAll(".tf-ln-src.tf-ln-warn")).toHaveLength(1);
  expect(target.querySelector(".tf-ln-src.tf-ln-warn")?.textContent).toContain("teamcity");
  // A credential reading still wins, in its own tone and words.
  expect(target.querySelector(".tf-ln-src.tf-ln-fail")?.textContent).toContain("gitea");
  expect(target.querySelectorAll(".tf-ln-src.tf-ln-fail.tf-ln-warn")).toHaveLength(0);
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

test("Tab steals no focus, and offers nothing while no detail is open", async () => {
  // `onlink` is supplied and `openEntity` is not, which is exactly how the
  // shell mounts this with no detail open: the handler is always there, the
  // open entity is what comes and goes.
  const onlink = vi.fn();
  open({ onlink });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const input = target.querySelector("input");
  const event = press("Tab");
  expect(event.defaultPrevented, "Tab is the action chain, not a focus move").toBe(true);
  expect(document.activeElement).toBe(input);
  // #55: the chain's one action needs a second end. With nothing open there is
  // nothing to link to, so there is no chain at all.
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  press("Enter");
  expect(onlink, "Enter opens the row; there was no action to run").not.toHaveBeenCalled();
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

test("the overlay is the shell's dialog: no .scrim, no .dlg, and a click on the dim closes it", async () => {
  const onclose = vi.fn();
  open({ onclose });
  await settle();
  const dialog = target.querySelector<HTMLElement>('[role="dialog"]');
  expect(dialog?.classList.contains("tf-dialog")).toBe(true);
  expect(dialog?.classList.contains("tf-launch")).toBe(true);
  expect(target.querySelector(".scrim, .dlg")).toBeNull();
  dialog!.parentElement!.click();
  flushSync();
  expect(onclose).toHaveBeenCalledTimes(1);
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

/** The open entity, as the shell names it to the launcher. */
const OPEN = { entityId: "jira:PAY-999", label: "PAY-999" };

/**
 * #55. The chain exists only while a detail is open, it names what is open,
 * and pressing it hands the *result* back — the launcher never writes.
 */
test("Tab offers Link to the open entity, and Enter hands the result to the shell", async () => {
  const onlink = vi.fn();
  open({ openEntity: OPEN, onlink });
  await settle();
  // Search, so the selected row is a result rather than the board.
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  const chain = target.querySelector('[aria-label="Actions on the selected result"]');
  expect(chain?.textContent).toContain("Link to PAY-999");

  press("Enter");
  // The first result of the fixture's first group.
  expect(onlink).toHaveBeenCalledWith({ entity_id: "jira:PAY-231", title: "Retry failed SEPA payouts", kind: "ticket" });
  // The launcher steps out of the way of the job's link dialog (#1500).
  expect(target.querySelector(".tf-launch"), "the launcher stayed open over the job's dialog").toBeNull();
});

/** The chain acts on the row that was selected when it opened, not on a later one. */
test("moving the selection abandons an open chain", async () => {
  const onlink = vi.fn();
  open({ openEntity: OPEN, onlink });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).not.toBeNull();

  // With a chain open the arrows walk *it*, so the selection is moved from a
  // closed one: Escape first.
  press("Escape");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  press("ArrowDown");
  press("Tab");
  press("Enter");

  expect(onlink).toHaveBeenCalledWith({ entity_id: "gitea:acme/svc#142", title: "Backoff", kind: "pr" });
});

/** An entity cannot be linked to itself, so its own row offers nothing. */
test("the open entity's own row has no chain", async () => {
  const onlink = vi.fn();
  open({ openEntity: { entityId: "jira:PAY-231", label: "PAY-231" }, onlink });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  expect(onlink).not.toHaveBeenCalled();
});

/**
 * *Start timer* on the selected result (#278, story 11).
 *
 * It needs no open detail — that is the difference from *Link to…*, which
 * needs a second end — so this drives the launcher with `openEntity` absent,
 * which is how the shell mounts it from a room with nothing open.
 *
 * The launcher only says *which* result was chosen. **Stopping the running
 * timer first is the shell's**, and the two are separated here on purpose: a
 * launcher that stopped a timer itself would be a second owner of the clock.
 */
test("Tab offers Start timer on a result, with no detail open", async () => {
  const ontimer = vi.fn();
  open({ ontimer });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  const chain = target.querySelector('[aria-label="Actions on the selected result"]');
  expect(chain?.textContent).toContain("Start timer on Retry failed SEPA payouts");

  press("Enter");
  expect(ontimer).toHaveBeenCalledWith("jira:PAY-231", "Retry failed SEPA payouts");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
});

/**
 * **The refusal, in the launcher.** A stored context is a row in
 * `knobas.entity` and comes back from search like anything else, so the row
 * has to be taken out rather than assumed away — the same rule
 * `shell/timer.ts` states and `start_timer` enforces.
 *
 * The fixture puts the context group **first**, so the chain under test is the
 * one on the selected row, and *Link to…* is offered alongside: the assertion
 * is that the timer row is gone and the link row is not, which a chain that
 * simply had nothing in it could not satisfy.
 */
test("a stored context result is not offered a timer, though it can still be linked", async () => {
  const ontimer = vi.fn();
  const onlink = vi.fn();
  const withContext = response({
    groups: [
      {
        kind: "ctx",
        label: "Context",
        plural: "Contexts",
        monogram: "CX",
        total: 1,
        hits: [
          hit({
            entity_id: "ctx:5b1c0f1e",
            kind: "ctx",
            title: "SEPA migration",
            source_id: "ctx",
          }),
        ],
      },
      ...response().groups,
    ],
  });
  open({
    ontimer,
    onlink,
    openEntity: OPEN,
    ports: { search: async () => withContext, launcherHome: async () => HOME },
  });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  press("Tab");
  const chain = target.querySelector('[aria-label="Actions on the selected result"]');
  expect(chain?.textContent, "a context was offered a timer").not.toContain("Start timer");
  expect(chain?.textContent, "the whole chain vanished, so this proves nothing").toContain(
    "Link to PAY-999",
  );

  press("Enter");
  expect(ontimer).not.toHaveBeenCalled();
});

/**
 * Escape unwinds the chain first — one rung per press, and the shell's ladder
 * still never sees the key.
 */
test("Escape closes the chain before it clears the query", async () => {
  // Only `Escape` matters here: the shell binds it (and ⌘K), and `Tab` is
  // preventDefault-ed rather than stopped, because nothing above listens for it.
  const reachedTheShell: string[] = [];
  const shell = (event: KeyboardEvent) => reachedTheShell.push(event.key);
  window.addEventListener("keydown", shell);
  open({ openEntity: OPEN, onlink: () => {} });
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
            unknown_tokens: ["env:prod", "/cf"],
          },
          groups: [],
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "env:prod /cf";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const greyed = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-off")].map((el) => el.textContent?.trim());
  expect(greyed).toEqual([
    "env:prod — the estate arrives with assets",
    "/cf — no source matches /cf",
  ]);
});

/**
 * The third reader of the actionable-health rule, and the one nothing else in
 * this file covers.
 *
 * A `/gitea` chip's dot and the row badge beside it are the same claim about
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
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "/gitea /jira sepa";
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
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "/gitea /nowhere sepa";
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
          },
        }),
    },
  });
  await settle();
  target.querySelector("input")!.value = "@me @jonas sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();

  const chips = [...target.querySelectorAll(".tf-ln-fchip.tf-ln-set")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["people", "@me", "jonas"]);
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
  expect(target.textContent).toContain("tickets only");

  rows[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await settle();
  expect(target.querySelector("input")!.value).toBe("#");
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
    "Nothing in the mirror matches",
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

test("an asset hit draws its monogram and the path from the root", async () => {
  open({ ports: { search: async () => response(ESTATE), launcherHome: async () => HOME } });
  await settle();
  await search("kuma");

  const rows = [...target.querySelectorAll(".tf-ln-res")].map((row) => row.textContent);
  expect(rows[0]).toContain("AS");
  expect(rows[0]).toContain("postgres");
  // Where it sits, which is what tells two containers called `postgres` apart.
  expect(target.querySelector(".tf-ln-res .tf-ln-pa")?.textContent).toBe("hel1 › vm-db-01");
  expect(rows[1]).toContain("RO");
  expect(rows[1]).toContain("Uptime Kuma");
  // The group headings are the backend's plurals, so the estate reads as a
  // section of the launcher rather than as unknown kinds at the bottom.
  const headings = [...target.querySelectorAll(".tf-ln-h .tf-ln-lab")].map((el) => el.textContent);
  expect(headings).toEqual(["Assets", "Routes"]);
});

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

/**
 * Every hit whose kind declares a parent arrives with the parent chain on its
 * `path` (#596), and the launcher draws whatever path a hit carries — no kind
 * is named here, so a later adapter's parent needs no launcher code. Two
 * "fix typo" commits from two repos are told apart by that line, and a hit
 * with no path draws no line at all.
 */
test("two same-titled commits are told apart by their repo on the path line", async () => {
  const commits: SearchResponse = {
    ...response(),
    groups: [
      {
        kind: "commit",
        label: "Commit",
        plural: "Commits",
        monogram: "CO",
        total: 2,
        hits: [
          hit({
            entity_id: "gitea:tidewater/payout-service@c90d11",
            kind: "commit",
            source_id: "gitea",
            title: "fix typo",
            path: "tidewater/payout-service",
          }),
          hit({
            entity_id: "gitea:tidewater/ledger@4e1a07",
            kind: "commit",
            source_id: "gitea",
            title: "fix typo",
            path: "tidewater/ledger",
          }),
        ],
      },
      {
        kind: "monitor",
        label: "Monitor",
        plural: "Monitors",
        monogram: "MO",
        total: 1,
        hits: [hit({ entity_id: "kuma:8", kind: "monitor", source_id: "kuma", title: "payout health" })],
      },
    ],
    total: 3,
  };
  open({ ports: { search: async () => commits, launcherHome: async () => HOME } });
  await settle();
  await search("fix typo");

  const rows = [...target.querySelectorAll(".tf-ln-res")];
  expect(rows.map((row) => row.querySelector(".tf-ln-pa")?.textContent ?? null)).toEqual([
    "tidewater/payout-service",
    "tidewater/ledger",
    null,
  ]);
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
test("Tab offers Link to the job, then Start timer, and nothing else", async () => {
  const onlink = vi.fn();
  const ontimer = vi.fn();
  open({
    openEntity: OPEN,
    onlink,
    ontimer,
    ports: { search: async () => response(ESTATE), launcherHome: async () => HOME },
  });
  await settle();
  await search("kuma");

  press("Tab");
  const chain = target.querySelector('[aria-label="Actions on the selected result"]');
  expect([...chain!.querySelectorAll(".tf-ln-chain-a")].map((el) => el.textContent?.trim())).toEqual([
    "Link to PAY-999",
    "Start timer on postgres",
  ]);

  press("ArrowDown");
  press("Enter");
  expect(ontimer).toHaveBeenCalledWith("asset:9f1c", "postgres");
  expect(onlink).not.toHaveBeenCalled();
});

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
  expect(navigated).toEqual(["#/job/jira%3APAY-231"]);
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
// Saved smart lists (#506)
// ---------------------------------------------------------------------------

/** A board carrying one built-in, one saved list, and one that needs attention. */
const SAVED_HOME: LauncherHome = {
  ...HOME,
  smart_lists: [
    {
      id: "mine",
      label: "My items",
      count: 3,
      changed: true,
      description: "Yours.",
      saved: false,
      needs_attention: false,
    },
    {
      id: "sepa-retries",
      label: "SEPA retries",
      count: 12,
      changed: true,
      description: "#sepa @me",
      saved: true,
      needs_attention: false,
    },
    {
      id: "legacy-palette",
      label: "Palette",
      count: 0,
      changed: false,
      description:
        "Needs attention: the saved query starts with a prefix that is not a search. Delete it and save the search again.",
      saved: true,
      needs_attention: true,
    },
  ],
};

/** The smart-list rows on the board, as one object per row. */
function railRows() {
  return [...target.querySelectorAll(".tf-ln-slrow")].map((row) => ({
    text: row.textContent ?? "",
    /** The right-hand cell: the count, or the words a refused query draws. */
    right: row.querySelector(".tf-ln-c")?.textContent?.trim() ?? "",
    controls: [...row.querySelectorAll("button.tf-ln-mini")].map(
      (button) => button.textContent?.trim() ?? "",
    ),
    open: row.querySelector<HTMLButtonElement>("button.tf-ln-sl"),
  }));
}

/**
 * Saved and built-in lists are one rail, and only the saved ones carry the two
 * controls (#506, stories 57 and 58).
 *
 * The built-in row is asserted **beside** the saved one rather than on its
 * own: what the story asks for is one kind with one difference, and a test
 * that only looked at the saved row could not see a *Rename* that had appeared
 * on every row.
 */
test("the rail draws saved lists beside the built-ins, and only they can be renamed", async () => {
  open({ ports: { launcherHome: async () => SAVED_HOME, search: async () => response() } });
  await settle();

  const rows = railRows();
  expect(rows).toHaveLength(3);
  // The built-in: its count, its badge, and nothing to press but the row.
  expect(rows[0]!.text).toContain("My items");
  expect(rows[0]!.text).toContain("3");
  expect(rows[0]!.controls).toEqual([]);
  // The saved one: the same count and badge, plus the two controls.
  expect(rows[1]!.text).toContain("SEPA retries");
  expect(rows[1]!.text).toContain("12");
  // Its blurb is the query it stands for.
  expect(rows[1]!.text).toContain("#sepa @me");
  expect(rows[1]!.controls).toEqual(["Rename", "Delete"]);
  // Both badges are drawn, so "like the built-ins" is a rendered fact.
  expect(target.querySelectorAll(".tf-ln-dot")).toHaveLength(2);
});

/**
 * A saved query today's grammar refuses reads *needs attention* and cannot be
 * opened (story 60).
 *
 * The second half is the one that matters and the one a text assertion alone
 * would miss: a row that said the right words and still put `list:legacy-…`
 * in the box would send the reader to the backend's refusal.
 */
test("a saved list that needs attention says so and does not open", async () => {
  open({ ports: { launcherHome: async () => SAVED_HOME, search: async () => response() } });
  await settle();

  const row = railRows()[2]!;
  // The **count cell** and not the row's whole text: the blurb under the name
  // carries these two words too, so a row that had gone back to drawing `0`
  // beside a needs-attention blurb would satisfy a whole-row `toContain`.
  expect(row.right).toBe("Needs attention");
  expect(row.open?.disabled).toBe(true);

  row.open?.click();
  flushSync();
  await settle();
  expect(target.querySelector("input")!.value).toBe("");

  // And from the keyboard, which does not go through the button at all: the
  // arrows and `Enter` reach `activate` directly, so `disabled` alone would
  // leave `Enter` putting `list:legacy-palette` in the box and sending the
  // reader to the backend's refusal.
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(target.querySelector("input")!.value).toBe("");

  // The positive control, and the assertion above is worth nothing without
  // it: an empty box is also what a launcher whose arrows never moved and
  // whose `Enter` never fired would show. `ArrowUp` steps back onto the
  // runnable saved row, and that one *does* open -- so the three keystrokes
  // above are known to have landed on the row they were aimed at.
  press("ArrowUp");
  press("Enter");
  await settle();
  expect(target.querySelector("input")!.value).toBe("list:sepa-retries");
});

/**
 * *Save as list* is offered on a query and not on the board (story 56).
 *
 * Both halves, because the control's whole rule is *when*: an empty box is the
 * board, and a board has nothing to save.
 */
test("save as list is offered on a query, never on an empty box", async () => {
  const created = vi.fn(async () => SAVED_HOME.smart_lists[1]!);
  const home = vi.fn(async () => SAVED_HOME);
  open({
    ports: { launcherHome: home, search: async () => response(), createSmartList: created },
  });
  await settle();
  expect(target.querySelector("button.tf-ln-save"), "an empty box is the board").toBeNull();

  await search("sepa retries");
  const button = target.querySelector<HTMLButtonElement>("button.tf-ln-save");
  expect(button?.textContent?.trim()).toBe("Save as list");

  const before = home.mock.calls.length;
  button!.click();
  await settle();

  // The **raw box text**, both as the query and as the name — one click, and
  // the name is what *Rename* is for.
  expect(created).toHaveBeenCalledWith("sepa retries", "sepa retries");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Saved as list");
  expect(
    home.mock.calls.length,
    "the rail is re-read rather than patched in place",
  ).toBeGreaterThan(before);

  // And the next keystroke is a different query, so the control comes back.
  await search("sepa retries and more");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Save as list");
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
  const created = vi.fn(async () => SAVED_HOME.smart_lists[1]!);
  for (const prefix of ["action", "help"] as const) {
    open({
      ports: {
        launcherHome: async () => SAVED_HOME,
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
 * The control is off the two queries that answer as results and that the
 * backend still refuses to save, and off a search that failed (#1413).
 *
 * An opened list (`list:`) would be a list of a list, `t ` is a worklog query
 * and not a search, and a failed search leaves an error line with no answer
 * under it. All three read as *results* to the mode helper, so the gate is the
 * backend's `interpreted.prefix` and the presence of an answer. The plain
 * query at the end is the positive control: without it a launcher that never
 * drew the control would pass.
 */
test("save as list is off an opened list, a time query and a failed search", async () => {
  const created = vi.fn(async () => SAVED_HOME.smart_lists[1]!);
  const cases = [
    { raw: "list:not-monitored", prefix: "list" as const, fails: false },
    { raw: "t today", prefix: "time" as const, fails: false },
    { raw: "sepa retries", prefix: null, fails: true },
  ];
  for (const { raw, prefix, fails } of cases) {
    open({
      ports: {
        launcherHome: async () => SAVED_HOME,
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
      launcherHome: async () => SAVED_HOME,
      search: async () => response(),
      createSmartList: created,
    },
  });
  await settle();
  await search("sepa retries");
  expect(target.querySelector("button.tf-ln-save")?.textContent?.trim()).toBe("Save as list");
});

/**
 * Renaming: Enter commits the field, Escape abandons it — and Escape does not
 * reach the launcher's own ladder while the field is open.
 *
 * The last clause is the one that would be silently wrong: the ladder clears
 * the box on `Esc` and closes on the next one, so a rename field that let the
 * key through would close the overlay over the name being typed.
 */
test("a saved list is renamed in place, and Escape abandons rather than commits", async () => {
  const renamed = vi.fn(async () => {});
  const closed = vi.fn();
  open({
    onclose: closed,
    ports: {
      launcherHome: async () => SAVED_HOME,
      search: async () => response(),
      renameSmartList: renamed,
    },
  });
  await settle();

  const rename = () =>
    [...target.querySelectorAll<HTMLButtonElement>("button.tf-ln-mini")].find(
      (button) => button.textContent?.trim() === "Rename",
    )!;

  // Abandoned.
  rename().click();
  flushSync();
  const field = target.querySelector<HTMLInputElement>("input.tf-ln-ren")!;
  expect(field.value).toBe("SEPA retries");
  field.value = "Half a name";
  const escape = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  field.dispatchEvent(escape);
  flushSync();
  await settle();
  expect(renamed).not.toHaveBeenCalled();
  expect(target.querySelector("input.tf-ln-ren")).toBeNull();
  expect(closed, "the launcher's own Esc ladder never saw the key").not.toHaveBeenCalled();

  // Committed.
  rename().click();
  flushSync();
  const again = target.querySelector<HTMLInputElement>("input.tf-ln-ren")!;
  again.value = "SEPA, retried";
  again.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  flushSync();
  await settle();
  expect(renamed).toHaveBeenCalledWith("sepa-retries", "SEPA, retried");
  expect(target.querySelector("input.tf-ln-ren")).toBeNull();
});

/** Deleting takes two presses on the row, and the first one only arms it. */
test("deleting a saved list takes a second press", async () => {
  const removed = vi.fn(async () => {});
  open({
    ports: {
      launcherHome: async () => SAVED_HOME,
      search: async () => response(),
      deleteSmartList: removed,
    },
  });
  await settle();

  const remove = () =>
    [...target.querySelectorAll<HTMLButtonElement>("button.tf-ln-mini.tf-ln-del")].find((button) =>
      ["Delete", "Confirm"].includes(button.textContent?.trim() ?? ""),
    )!;

  remove().click();
  flushSync();
  await settle();
  expect(removed, "the first press only arms it").not.toHaveBeenCalled();
  expect(remove().textContent?.trim()).toBe("Confirm");

  remove().click();
  flushSync();
  await settle();
  expect(removed).toHaveBeenCalledWith("sepa-retries");
});

/**
 * The `Tab` chain is the keyboard's way to the same two writes, and a built-in
 * has no chain at all.
 *
 * The negative is the half that keeps the rule legible: *Rename* on a built-in
 * would be a row offering a write the backend answers `not_found` to.
 */
test("the Tab chain renames and deletes a saved list, and offers nothing on a built-in", async () => {
  const removed = vi.fn(async () => {});
  open({
    ports: {
      launcherHome: async () => SAVED_HOME,
      search: async () => response(),
      deleteSmartList: removed,
    },
  });
  await settle();

  // Row 0 is the built-in, and `Tab` on it opens nothing.
  press("Tab");
  expect(target.querySelector(".tf-ln-chain")).toBeNull();

  // Row 1 is the saved one.
  press("ArrowDown");
  press("Tab");
  const chain = [...target.querySelectorAll(".tf-ln-chain-a")].map((row) => row.textContent?.trim());
  expect(chain).toEqual(["Rename SEPA retries", "Delete SEPA retries"]);

  press("ArrowDown");
  press("Enter");
  await settle();
  expect(removed).toHaveBeenCalledWith("sepa-retries");
  expect(target.querySelector(".tf-ln-chain"), "the chain closes behind the write").toBeNull();
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

test("a supplied source list that is still being read is not 'No source is configured'", async () => {
  const retry = vi.fn();
  open({ sources: [], sourcesRead: { phase: "loading", error: null, retry } });
  await settle();
  expect(target.textContent).not.toContain("No source is configured yet");
  expect(target.textContent).toContain("Reading the sources");
});

test("a failed source read says why and offers a retry, never 'No source is configured'", async () => {
  const retry = vi.fn();
  open({ sources: [], sourcesRead: { phase: "failed", error: "the keychain is locked", retry } });
  await settle();
  expect(target.textContent).not.toContain("No source is configured yet");
  expect(target.textContent).toContain("Could not read the sources");
  expect(target.textContent).toContain("the keychain is locked");
  const button = [...target.querySelectorAll("button")].find((b) => b.textContent?.trim().startsWith("Retry"));
  button!.click();
  expect(retry).toHaveBeenCalledTimes(1);
});

test("a ready, empty source read still says no source is configured", async () => {
  open({ sources: [], sourcesRead: { phase: "ready", error: null, retry: () => {} } });
  await settle();
  expect(target.textContent).toContain("No source is configured yet");
});

test("a /source token is not 'no source matches' while the sources are unread", async () => {
  const unknown = (raw: string) =>
    response({
      interpreted: { ...response().interpreted, text: raw, unknown_tokens: ["/cf"] },
    });
  open({
    sources: [],
    sourcesRead: { phase: "failed", error: "down", retry: () => {} },
    ports: { launcherHome: async () => HOME, search: async () => unknown("x") },
  });
  await settle();
  typeInBox("/cf x");
  await debounced();
  expect(target.textContent).toContain("/cf");
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

test("a search that never answers says Searching, never 'Nothing in the mirror matches'", async () => {
  open({
    ports: { launcherHome: async () => HOME, search: () => new Promise<never>(() => {}) },
  });
  await settle();
  typeInBox("sepa");
  await debounced();
  expect(target.textContent).toContain("Searching");
  expect(target.textContent).not.toContain("Nothing in the mirror matches");
});

test("a rejected search shows the reason, never 'Nothing in the mirror matches'", async () => {
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
  expect(target.textContent).not.toContain("Nothing in the mirror matches");
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

/** The board with its saved lists, as the rename and chain tests need it. */
function openSaved() {
  open({ ports: { launcherHome: async () => SAVED_HOME, search: async () => response() } });
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
      launcherHome: async () => ({ ...HOME, smart_lists: [], recent: [] }),
      search: async () => response(),
    },
  });
  await settle();
  expect(box().hasAttribute("aria-activedescendant")).toBe(false);
  expect(box().getAttribute("aria-controls")).toBe(listbox("Results").id);
});

test("a list being renamed has no option, so the box names none", async () => {
  openSaved();
  await settle();
  press("ArrowDown");
  press("Tab");
  press("Enter");
  expect(target.querySelector("input.tf-ln-ren")).not.toBeNull();
  expect(box().hasAttribute("aria-activedescendant")).toBe(false);
});

test("the Tab chain's active row is scrolled into view and named by the box", async () => {
  openSaved();
  await settle();
  press("ArrowDown");
  const row = box().getAttribute("aria-activedescendant");
  const scrolls = recordScrolls();
  press("Tab");
  const chain = listbox("Actions on the selected result");
  const rows = [...chain.querySelectorAll('[role="option"]')];
  expect(rows).toHaveLength(2);
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

  // Closing the chain hands the box back to the results, on the same row.
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
  expect(labels).toHaveLength(24);
  expect(labels.slice(0, 11)).toEqual([
    "Tickets",
    "Inbox",
    "Code",
    "Builds",
    "Docs",
    "Dependencies",
    "Monitors",
    "Link sets",
    "Proposed links",
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

// ── Rename takes the focus it needs (#1412) ────────────────────────────────

/** The name field, focused, with its whole text selected. */
function expectRenameFocused() {
  const field = target.querySelector<HTMLInputElement>("input.tf-ln-ren");
  expect(field, "the rename field is open").not.toBeNull();
  expect(document.activeElement, "focus is in the name field, not the query box").toBe(field);
  expect(field!.selectionStart).toBe(0);
  expect(field!.selectionEnd).toBe("SEPA retries".length);
}

test("Rename from the Tab chain focuses the name field with the name selected", async () => {
  openSaved();
  await settle();
  press("ArrowDown");
  press("Tab");
  press("Enter");
  await settle();
  expectRenameFocused();
});

test("the row's Rename button focuses the name field with the name selected", async () => {
  openSaved();
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button.tf-ln-mini")]
    .find((button) => button.textContent?.trim() === "Rename")!
    .click();
  flushSync();
  await settle();
  expectRenameFocused();
});

test("what is typed after the chain's Rename goes into the name, not the query", async () => {
  const renamed = vi.fn(async () => {});
  open({
    ports: { launcherHome: async () => SAVED_HOME, search: async () => response(), renameSmartList: renamed },
  });
  await settle();
  press("ArrowDown");
  press("Tab");
  press("Enter");
  await settle();

  // Typing is whatever holds focus receiving the text; jsdom does not insert
  // it for a keydown, so the test writes into the focused element.
  const focused = document.activeElement as HTMLInputElement;
  focused.value = "X";
  focused.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();

  const query = target.querySelector<HTMLInputElement>("input:not(.tf-ln-ren)")!;
  expect(query.value, "the query box is untouched").toBe("");
  expect(target.querySelector<HTMLInputElement>("input.tf-ln-ren")!.value).toBe("X");
  expect(railRows(), "the board is still drawn").toHaveLength(3);

  focused.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  flushSync();
  await settle();
  expect(renamed).toHaveBeenCalledWith("sepa-retries", "X");
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

test("a value suggestion draws inline, a chain row spans one column, and a prefix row keeps two", async () => {
  const style = launcherStyles();
  try {
    await openPalette();
    const prefix = target.querySelector<HTMLElement>('[aria-label="Results"] .tf-ln-opt');
    expect(prefix, "the palette draws prefix rows").not.toBeNull();
    expect(getComputedStyle(prefix!).display).toBe("grid");
    expect(getComputedStyle(prefix!).gridTemplateColumns).toBe("240px 1fr");

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
    const chainRow = target.querySelector<HTMLElement>(".tf-ln-chain [role='option']")!;
    expect(chainRow.classList.contains("tf-ln-opt"), "the chain row is a prefix row underneath").toBe(true);
    expect(getComputedStyle(chainRow).gridTemplateColumns).toBe("1fr");
  } finally {
    style.remove();
  }
});

/** One inbox line, as `inbox_items` answers it (#1418). */
function inboxItem(over: Partial<InboxItem> & Pick<InboxItem, "key">): InboxItem {
  return {
    category: "mention",
    source_id: "jira",
    entity_id: "jira:PAY-1",
    kind: "ticket",
    title: "Reconcile mandates",
    reason: "mara mentioned you",
    occurred_at: "2026-08-25T11:30:00Z",
    web_url: null,
    snoozed_until: null,
    ...over,
  };
}

const INBOX = {
  count: 3,
  items: [
    inboxItem({
      key: "review_request:gitea:acme/payouts#144",
      category: "review_request",
      source_id: "gitea",
      entity_id: "gitea:acme/payouts#144",
      kind: "pr",
      title: "Retry SEPA batches",
      reason: "jonas asked for your review",
    }),
    inboxItem({
      key: "failed_build:tc:payments-main-88",
      category: "failed_build",
      source_id: "tc",
      entity_id: "tc:payments-main-88",
      kind: "build",
      title: "payments main #88",
      reason: "build failed",
      occurred_at: "2026-08-24T15:10:00Z",
    }),
    inboxItem({ key: "mention:jira:PAY-1" }),
  ],
};

/**
 * The board's inbox preview (#1418, spec §4: *"inbox preview"*): the count the
 * top bar reads and the newest items, each a row of the board's own list.
 *
 * With nothing in the inbox there is no block at all — an empty column with a
 * heading over it reads as a broken app, which is the board header's own rule.
 */
test("the board previews the inbox: the count and its newest three, and nothing when it is empty", async () => {
  open({ inbox: INBOX });
  await settle();
  const block = target.querySelector(".tf-ln-inbox");
  expect(block, "no inbox block").not.toBeNull();
  expect(target.querySelector(".tf-ln-h.tf-ln-inboxh")?.textContent).toMatch(/Inbox\s*3/);
  const rows = [...target.querySelectorAll(".tf-ln-inbox [role=option]")].map((el) =>
    el.textContent?.replace(/\s+/g, " ").trim(),
  );
  expect(rows).toEqual([
    "jonas asked for your review: Retry SEPA batches 13:30",
    // The Inbox page's err dot on a failure.
    "\u25cf Build failed: payments main #88 yesterday 17:10",
    "Mara mentioned you: Reconcile mandates 13:30",
  ]);
  if (app) unmount(app);
  app = undefined;

  open({ inbox: { count: 0, items: [] } });
  await settle();
  expect(target.querySelector(".tf-ln-inbox")).toBeNull();
  expect(target.querySelector(".tf-ln-inboxh")).toBeNull();
  // The board itself still drew: the absence is the inbox's, not the board's.
  expect(target.textContent).toContain("Reconcile mandates");
});

/**
 * Today's time on the board (#1418, spec §4: *"today's time"*): the Day view's
 * tracked and to-log figures, in its words, and nothing on a day with none.
 */
test("the board gives today's tracked and to-log time, and nothing on a day with none", async () => {
  open({ today: { tracked: 130, toLog: 65 } });
  await settle();
  const block = target.querySelector(".tf-ln-today [role=option]");
  expect(block?.textContent?.replace(/\s+/g, " ").trim()).toBe("2h 10m tracked 1h 5m to log");
  if (app) unmount(app);
  app = undefined;

  open({ today: { tracked: 0, toLog: 0 } });
  await settle();
  expect(target.querySelector(".tf-ln-today")).toBeNull();
  expect(target.textContent).toContain("Reconcile mandates");
});

/**
 * Pressing an inbox row opens its item; pressing the time block opens Time ›
 * Day (#1418). Both by pointer and by the keyboard, since they are rows of the
 * board's one list: `↓` from the last smart list lands on the first inbox row.
 */
test("an inbox row opens its item and the time block opens the day", async () => {
  const onnavigate = vi.fn();
  async function reopen() {
    if (app) unmount(app);
    app = undefined;
    open({ onnavigate, inbox: INBOX, today: { tracked: 130, toLog: 0 } });
    await settle();
  }
  await reopen();
  target.querySelectorAll<HTMLButtonElement>(".tf-ln-inbox [role=option]")[1]!.click();
  expect(onnavigate).toHaveBeenLastCalledWith("#/build/tc%3Apayments-main-88");

  await reopen();
  target.querySelector<HTMLButtonElement>(".tf-ln-today [role=option]")!.click();
  expect(onnavigate).toHaveBeenLastCalledWith("#/time");

  // The keyboard: two smart lists, then the inbox's three, then the day.
  await reopen();
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  expect(onnavigate).toHaveBeenLastCalledWith("#/pr/gitea%3Aacme%2Fpayouts%23144");
  await reopen();
  for (let i = 0; i < 5; i += 1) press("ArrowDown");
  press("Enter");
  expect(onnavigate).toHaveBeenLastCalledWith("#/time");
  expect(onnavigate).toHaveBeenCalledTimes(4);
});

test("two sources' PAY-231: each row names its source before its age, a unique key's row does not (#1476)", async () => {
  const twins = response({
    groups: [
      {
        kind: "ticket",
        label: "Ticket",
        plural: "Tickets",
        monogram: "TK",
        total: 3,
        hits: [
          hit({ entity_id: "mock:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", source_id: "mock", synced_at: "2026-08-25T11:59:50Z" }),
          hit({ entity_id: "jira:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", source_id: "jira", synced_at: "2026-08-25T11:35:00Z" }),
          hit({ entity_id: "jira:PAY-232", kind: "ticket", title: "Metrics for SEPA retry attempts" }),
        ],
      },
    ],
  });
  const onnavigate = vi.fn();
  open({ onnavigate, ports: { search: async () => twins, launcherHome: async () => HOME } });
  await settle();
  target.querySelector("input")!.value = "PAY-231";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await debounced();
  const ages = [...target.querySelectorAll(".tf-ln-sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["mock · synced just now", "jira · synced 25 min ago", "synced 4 min ago"]);
  // The Jira row opens Jira's PAY-231, by the address that names its source.
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(onnavigate).toHaveBeenCalledWith("#/job/jira%3APAY-231");
});

// ── Reopening selects the kept query (#1480) ───────────────────────────────

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

test("⌘K reopens on the kept query with the whole of it selected", async () => {
  open();
  await settle();
  await search("PAY-231");
  closeByX();
  expect(target.querySelector(".tf-launch"), "the × closed the overlay").toBeNull();
  await reopen();

  const input = target.querySelector("input")!;
  expect(input.value, "the session keeps what was searched for").toBe("PAY-231");
  expect(document.activeElement).toBe(input);
  expect(input.selectionStart).toBe(0);
  expect(input.selectionEnd).toBe("PAY-231".length);
});

test("typing after a reopen replaces the kept query instead of appending to it", async () => {
  open();
  await settle();
  await search("PAY-231");
  closeByX();
  await reopen();

  // Typing replaces whatever is selected; jsdom does not insert text for a
  // keydown, so the test does what the WebView does with a keystroke.
  const input = target.querySelector("input")!;
  input.setRangeText("x", input.selectionStart!, input.selectionEnd!, "end");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  expect(input.value).toBe("x");
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
 * The re-focus after a click on a row keeps the caret where it was. Witnessed
 * through a row whose click leaves the box's text alone (Rename, then Esc out
 * of the name): a row that sets the box's text moves the caret to its end on
 * the value write whatever the re-focus did, and could not tell the two apart.
 * Blanks keep the board, and its rail, on screen.
 */
test("a click on a row re-focuses the box without selecting its text", async () => {
  openSaved();
  await settle();
  const input = target.querySelector("input")!;
  input.value = "   ";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  input.setSelectionRange(1, 1);

  [...target.querySelectorAll<HTMLButtonElement>("button.tf-ln-mini")]
    .find((button) => button.textContent?.trim() === "Rename")!
    .click();
  flushSync();
  await settle();
  const field = target.querySelector<HTMLInputElement>("input.tf-ln-ren")!;
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  await settle();

  expect(document.activeElement, "focus is back in the box").toBe(input);
  expect(input.value).toBe("   ");
  expect(input.selectionStart).toBe(1);
  expect(input.selectionEnd).toBe(1);
});
