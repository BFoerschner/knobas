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
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchResponse, Segment } from "../ipc";
import type { CredentialHealth } from "../ipc/sources";
import Launcher from "./Launcher.svelte";

const NOW = new Date("2026-08-25T12:00:00Z");

function hit(over: {
  entity_id: string;
  kind: string;
  title: string;
  source_id?: string;
  snippet?: Segment[];
  synced_at?: string;
}) {
  return {
    entity_id: over.entity_id,
    kind: over.kind,
    source_id: over.source_id ?? "jira",
    updated_at: null,
    synced_at: over.synced_at ?? "2026-08-25T11:56:00Z",
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
    { id: "mine", label: "My items", count: 3, changed: true, description: "Yours." },
    { id: "just-synced", label: "Just synced", count: 9, changed: false, description: "New." },
  ],
  recent: [
    {
      entity_id: "jira:PAY-240",
      kind: "ticket",
      source_id: "jira",
      title: "Reconcile mandates",
      updated_at: null,
      synced_at: "2026-08-25T11:58:00Z",
    },
  ],
  sources: [],
  pending_writes: 0,
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
      // cannot drop it silently, and a fixture with no timer to offer says so.
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
 * The 90 ms debounce is not a counterexample. Where a test waits it out, it
 * does so with its own real timer (`setTimeout(…, 120)`) armed *after* the
 * debounce's, and the timer queue fires in due order — so what puts the search
 * before the assertion is the ordering, not the 30 ms of margin. The same
 * reading applies to the other microtask-count `settle()` helpers in this
 * repo (`SourcesView`, `Diagnostics`, `AddSource`, `FirstRun`): none of the
 * components they drive dynamically imports anything either.
 */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
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
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const row = target.querySelector(".res");
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const headings = [...target.querySelectorAll(".lab")].map((el) => el.textContent?.trim());
  expect(headings).toEqual(["Tickets", "Pull requests", "Builds"]);

  // 42 matched, one on the page: §4 wants both numbers or the reader believes
  // there is one ticket.
  const first = target.querySelector(".n")!;
  expect(first.textContent).toContain("42");
  expect(first.textContent).toContain("41 more");

  // And every row carries its provenance, which is what §4 asks for on all of
  // them rather than on the ones that happen to have room.
  const ages = [...target.querySelectorAll(".sy")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const ages = [...target.querySelectorAll(".sy")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const ages = [...target.querySelectorAll(".sy")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  // Three rows from three different sources, only the middle one failing, so
  // a fixture that could not witness a mix-up is not what is being read here.
  const ages = [...target.querySelectorAll(".sy")].map((el) => el.textContent?.trim());
  expect(ages).toEqual(["synced 4 min ago", "gitea · sign in again", "synced 4 min ago"]);
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
  expect(target.querySelector(".c.chg")?.textContent).toContain("3");
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

  const strip = [...target.querySelectorAll(".strip .src")];
  expect(strip.length, "the strip did not render at all").toBe(1);
  expect(strip[0]!.className).not.toContain("fail");
  expect(strip[0]!.textContent).not.toContain("401 from Gitea");
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

  const lines = [...target.querySelectorAll(".src")].map((el) => el.textContent?.replace(/\s+/g, " ").trim());
  // `GI`, not `GT`: the monogram is the first two letters of the id, because
  // §3a forbids a table of adapter names. Written `GT` at first, from the
  // mockup's hand-curated map — the second time this fixture-vs-derivation gap
  // has been caught by a test rather than by reading the code.
  expect(lines).toEqual(["JI jira checked 1 h ago", "GI gitea 401 from Gitea"]);
  // Only the broken one is marked, and `ok` is not "needs attention".
  expect(target.querySelectorAll(".src.fail")).toHaveLength(1);
  expect(target.querySelector(".src.fail")?.textContent).toContain("gitea");
});

test("Enter on a row navigates to its address and closes the overlay", async () => {
  const onnavigate = vi.fn();
  const onclose = vi.fn();
  open({ onnavigate, onclose });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  press("ArrowDown");
  press("Enter");
  await settle();

  // The second row is the PR, and its key carries a `#` and a `/` — both of
  // which have to survive the address or the detail view opens on nothing.
  expect(onnavigate).toHaveBeenCalledWith("#/pr/gitea:acme%2Fsvc%23142");
  expect(onclose).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".dlg")).toBeNull();
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
  expect(target.querySelector(".dlg")).toBeNull();

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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  press("Tab");
  const chain = target.querySelector('[aria-label="Actions on the selected result"]');
  expect(chain?.textContent).toContain("Link to PAY-999");

  press("Enter");
  // The first result of the fixture's first group.
  expect(onlink).toHaveBeenCalledWith("jira:PAY-231", "Retry failed SEPA payouts");
  // The chain closes behind the action rather than staying over a done thing.
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
});

/** The chain acts on the row that was selected when it opened, not on a later one. */
test("moving the selection abandons an open chain", async () => {
  const onlink = vi.fn();
  open({ openEntity: OPEN, onlink });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  press("Tab");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).not.toBeNull();

  // With a chain open the arrows walk *it*, so the selection is moved from a
  // closed one: Escape first.
  press("Escape");
  expect(target.querySelector('[aria-label="Actions on the selected result"]')).toBeNull();
  press("ArrowDown");
  press("Tab");
  press("Enter");

  expect(onlink).toHaveBeenCalledWith("gitea:acme/svc#142", "Backoff");
});

/** An entity cannot be linked to itself, so its own row offers nothing. */
test("the open entity's own row has no chain", async () => {
  const onlink = vi.fn();
  open({ openEntity: { entityId: "jira:PAY-231", label: "PAY-231" }, onlink });
  await settle();
  target.querySelector("input")!.value = "sepa";
  target.querySelector("input")!.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

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
  expect(target.querySelector(".dlg")).toBeNull();

  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }),
  );
  flushSync();
  await settle();
  expect(target.querySelector(".dlg")).not.toBeNull();

  // The shell binds ⌘K too. Two "open" writes on one keystroke must converge,
  // which a toggle would not.
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true, cancelable: true }),
  );
  flushSync();
  await settle();
  expect(target.querySelector(".dlg")).not.toBeNull();
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const greyed = [...target.querySelectorAll(".fchip.off")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const chips = [...target.querySelectorAll(".fchip.on")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["gitea", "jira"]);
  const failing = [...target.querySelectorAll(".fchip.on.fail")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const chips = [...target.querySelectorAll(".fchip.on")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["gitea", "nowhere"]);
  const failing = [...target.querySelectorAll(".fchip.on.fail")].map((el) => el.textContent?.trim());
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const chips = [...target.querySelectorAll(".fchip.on")].map((el) => el.textContent?.trim());
  expect(chips).toEqual(["people", "@me", "jonas"]);
  expect([...target.querySelectorAll(".fchip.off")]).toEqual([]);
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const rows = [...target.querySelectorAll(".pfx")];
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  const gap = target.querySelector(".gap");
  expect(gap?.textContent).toContain("An author search cannot be answered by:");
  expect(gap?.textContent).toContain("Buildserver");
  expect(gap?.textContent).toContain("nothing it has synced names a person");
  // The source that *did* answer is not named: it has nothing to explain, and
  // listing it would turn the explanation into a roll call.
  expect(gap?.textContent).not.toContain("Jira");
  // And the ordinary empty-result line is still there — the gap explains part
  // of the emptiness, it does not replace the answer.
  expect(target.querySelector(".none")).not.toBeNull();
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
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();

  // The results are there, so this is not green because nothing rendered.
  expect(target.textContent).toContain("Retry failed SEPA payouts");
  expect(target.querySelector(".gap")).toBeNull();
});
