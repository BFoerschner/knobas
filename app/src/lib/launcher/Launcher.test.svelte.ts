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

/** Let the injected promises settle and the DOM catch up. */
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

test("Tab is reserved and steals no focus", async () => {
  open();
  await settle();
  const input = target.querySelector("input");
  const event = press("Tab");
  expect(event.defaultPrevented, "Tab is M2's action chain, not a focus move").toBe(true);
  expect(document.activeElement).toBe(input);
});

test("⌘K opens the overlay from anywhere, and opening twice is not closing", async () => {
  app = mount(Launcher, {
    target,
    props: {
      open: false,
      onnavigate: () => {},
      onclose: () => {},
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
 * the same source, so they are drawn from one rule (`format.ts::isActionable`)
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
