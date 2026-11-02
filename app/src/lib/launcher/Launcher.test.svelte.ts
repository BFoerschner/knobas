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
import type { StateChip } from "../ipc/entity";
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
  state?: StateChip | null;
}) {
  return {
    entity_id: over.entity_id,
    kind: over.kind,
    source_id: over.source_id ?? "jira",
    updated_at: null,
    synced_at: over.synced_at ?? "2026-08-25T11:56:00Z",
    path: over.path ?? null,
    state: over.state ?? null,
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
            unknown_tokens: ["env:prod", "source:cf"],
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
  expect(chips).toEqual(["repositories", "lists"]);
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

test("the placeholder names the # letters and no / source", async () => {
  open();
  await settle();
  const placeholder = target.querySelector("input")!.placeholder;
  expect(placeholder).not.toContain("/");
  expect(placeholder).toContain("#t");
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

/** A recent item on the board. */
function recent(entity_id: string, title: string): LauncherHome["recent"][number] {
  return { entity_id, kind: "ticket", source_id: "jira", title, updated_at: null, synced_at: "2026-08-25T11:58:00Z", path: null, state: null };
}

/**
 * The board with three recent items, each with a two-row chain: *Link to* the
 * open job and *Start timer*.
 */
function openSaved() {
  open({
    openEntity: OPEN,
    onlink: vi.fn(),
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
      launcherHome: async () => ({ ...HOME, smart_lists: [], recent: [] }),
      search: async () => response(),
    },
  });
  await settle();
  expect(box().hasAttribute("aria-activedescendant")).toBe(false);
  expect(box().getAttribute("aria-controls")).toBe(listbox("Results").id);
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

test("a result row is three cells, the key cell following the kind's rule", async () => {
  open({ ports: { search: async () => response(EVERY_KIND), launcherHome: async () => HOME } });
  await settle();
  await search("sepa");

  expect(cells()).toEqual([
    ["TK", "PAY-231", "Retry failed SEPA payouts"],
    ["PR", "payout-service", "Backoff on SEPA retries"],
    // The owner is the cell, so the repository's own name is the title.
    ["RP", "tidewater", "payout-service"],
    ["PG", "Engineering", "SEPA retry design"],
    ["PL", "Payout service", "Build"],
    ["MO", "http", "payout health"],
    ["VW", "Payments", "My open SEPA work"],
    // Nothing better known: the kind's word.
    ["NT", "note", "Ask Jonas about the cut-off"],
  ]);
  // Only a ticket's key is an identifier, drawn in the mono face.
  const mono = [...target.querySelectorAll(".tf-ln-res .tf-ln-k")].map((cell) => cell.classList.contains("tf-ln-mono"));
  expect(mono).toEqual([true, false, false, false, false, false, false, false]);
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
  expect(monograms[0]!.getAttribute("aria-label")).toBe("Ticket: In Progress");
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
    ["TK", "PAY-240", "Reconcile mandates"],
    ["RP", "tidewater", "ledger"],
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
