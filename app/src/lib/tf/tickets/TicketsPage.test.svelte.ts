/**
 * Tickets in the ticket-first shell (#810): the ticket board and the list
 * over `mini_board`, the filter bar sending its query to the server, the
 * yellow selected card and the preview.
 */
import { needles } from "./needles";
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  ContextRow,
  EntityFilter,
  EntityRow,
  LanesAsked,
  MiniBoard,
  MiniBoardCard,
  Reached,
  StateChip,
} from "../../ipc/entity";
import type { QueryTranslation, TicketView, TicketViewDraft, TicketViewRow } from "../../ipc/search";
import { toasts } from "../../shell/toasts.svelte";
import TicketsPage, { forgetTicketsSession, type TicketsPorts } from "./TicketsPage.svelte";
import { sourceRoles } from "../roles.svelte";
import { LANE_FIELDS, laneOf } from "./tickets";
import { until } from "../../shell/test-wait";

// Every command that reaches the bridge past the page's ports, by name: the
// page takes its reads through `ports`, so the list stays empty unless a read
// bypasses them (as `promote_context` would to draw the preview, #1030).
const bridged = vi.hoisted(() => [] as string[]);
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke:(command: string) => {
    bridged.push(command);
    return Promise.reject(new Error("no bridge in this suite"));
  },
}));

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  bridged.length = 0;
  location.hash = "#/tickets";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function card(key: string, title: string, extra: Partial<MiniBoardCard> = {}): MiniBoardCard {
  return {
    entity_id: `jira:${key}`,
    source_id: "jira",
    key,
    title,
    // The harness's board puts the status it stands in on each card.
    status: null,
    priority: null,
    issue_type: null,
    assignee: null,
    updated_at: null,
    path: null,
    parent_title: null,
    mine: false,
    labels: [],
    comment_count: null,
    signals: [],
    ...extra,
  };
}

/** Twelve To Do, one In Progress and five Done tickets; PAY-1 is mine. */
const CORPUS: { status: string; card: MiniBoardCard }[] = [
  ...Array.from({ length: 12 }, (_, i) => ({
    status: "To Do",
    card: card(`PAY-${i + 1}`, i === 0 ? "Retry failed SEPA payouts" : `Ticket ${i + 1}`, {
      mine: i === 0,
      assignee: i === 0 ? "mara.lindqvist" : "jonas.becker",
      issue_type: "Story",
      priority: "High",
      path: i === 0 ? "PAY-200" : null,
      parent_title: i === 0 ? "Payout reliability" : null,
      comment_count: i === 0 ? 3 : null,
      updated_at: `2026-08-${String(22 - i).padStart(2, "0")}T10:00:00Z`,
    }),
  })),
  {
    status: "In Progress",
    card: card("PAY-40", "Payout dashboard latency", {
      mine: true,
      // An epic the mirror does not hold (#894), and a count of none.
      path: "PAY-300",
      comment_count: 0,
      assignee: "mara.lindqvist",
      signals: [
        { kind: "failing-build", text: "build failed", tone: "err" },
        { kind: "open-pr", text: "PR #142 open", tone: "info" },
      ],
    }),
  },
  ...Array.from({ length: 5 }, (_, i) => ({ status: "Done", card: card(`PAY-${50 + i}`, `Done ${i}`) })),
];

interface Harness {
  ports: TicketsPorts;
  /** How often the retired contexts read was made: none (#1502). */
  contextReads: number;
  reads: { perColumn: number | null; query: string | null; lanes?: LanesAsked }[];
  /** The ticket views the views read answers with (#812); the writes change it. */
  views: TicketViewRow[];
  saved: TicketViewDraft[];
  deleted: string[];
  /** Every `hide_saved_view` (#878): the view and whether it was hidden. */
  hidden: [string, boolean][];
  /** Every `open_saved_view` (#878). */
  opened: string[];
  /** Every `translate_ticket_view` (#1208). */
  translated: string[];
  /** The mirror rows a reach or a context can hold. */
  rows: EntityRow[];
  /** What each ticket reaches (`job_reach`), by entity id. */
  reach: Record<string, Reached[]>;
  /** A stored context's members, by context id. */
  members: Record<string, string[]>;
  /** Every `list_entities` filter the page sent. */
  lists: EntityFilter[];
}

function row(entity_id: string, kind: string, tone: StateChip["tone"] | null = null): EntityRow {
  return {
    entity_id,
    kind,
    source_id: kind === "build" ? "teamcity" : "gitea",
    title: entity_id,
    updated_at: null,
    synced_at: "",
    path: null,
    state: tone === null ? null : { text: tone === "err" ? "failed" : "success", tone },
  };
}

const PR = row("gitea:payout-service#142", "pr", "info");
const BUILD_FAILED = row("teamcity:it#1187", "build", "err");
const BUILD_OK = row("teamcity:lint#904", "build", "ok");
/** What the reads above can answer with. */
const AROUND_ROWS = [PR, BUILD_FAILED, BUILD_OK];

function own(of: EntityRow): Reached {
  return { entity_id: of.entity_id, kind: of.kind, step: of.entity_id, origin: { kind: "own" } };
}

/**
 * A whole board in lanes (#939), as `mini_board` answers: each lane's
 * column capped at `perColumn`, lanes biggest first (ties by key here), the
 * first `lanes.first` sent and the rest counted.
 */
function inLanes(whole: MiniBoard, lanes: LanesAsked, perColumn: number | null): MiniBoard {
  const keys = [...new Set(whole.columns.flatMap((column) => column.cards.map((card) => laneOf(card, lanes.by))))];
  const ranked = keys
    .map((key) => {
      const columns = whole.columns
        .map((column) => {
          const ids = column.cards.filter((card) => laneOf(card, lanes.by) === key).map((card) => card.entity_id);
          return { status: column.status, ids: perColumn === null ? ids : ids.slice(0, perColumn), total: ids.length };
        })
        .filter((cell) => cell.total > 0);
      return { key, count: columns.reduce((all, cell) => all + cell.total, 0), columns };
    })
    .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key));
  const sent = ranked.slice(0, lanes.first);
  const kept = new Set(sent.flatMap((lane) => lane.columns.flatMap((cell) => cell.ids)));
  return {
    columns: whole.columns.map((column) => ({ ...column, cards: column.cards.filter((card) => kept.has(card.entity_id)) })),
    sources: whole.sources,
    lanes: sent,
    more_lanes: ranked.length - sent.length,
  };
}

/** Board reads answered over {@link CORPUS}, the query read as words that must all appear. */
function harness(
  contexts: ContextRow[] = [],
  views: TicketViewRow[] = [],
  corpus: { status: string; card: MiniBoardCard }[] = CORPUS,
): Harness {
  const reads: Harness["reads"] = [];
  const h: Harness = {
    reads,
    contextReads: 0,
    views,
    saved: [],
    deleted: [],
    hidden: [],
    opened: [],
    translated: [],
    rows: [...AROUND_ROWS],
    reach: {},
    members: {},
    lists: [],
    ports: {
      views: () => Promise.resolve(h.views.map((row) => ({ ...row }))),
      saveView: (draft) => {
        h.saved.push(draft);
        const id = draft.id ?? draft.name.toLowerCase().replace(/[^a-z0-9]+/g, "-");
        const made: TicketView = {
          ...draft,
          id,
          position: draft.position ?? h.views.length,
        };
        h.views = [...h.views.filter((row) => row.id !== id), { ...made, count: 1, unreadable: null, saved_view_source: null }].sort(
          (a, b) => a.position - b.position,
        );
        return Promise.resolve(made);
      },
      deleteView: (id) => {
        h.deleted.push(id);
        h.views = h.views.filter((row) => row.id !== id);
        return Promise.resolve();
      },
      hideView: (id, hidden) => {
        h.hidden.push([id, hidden]);
        const row = h.views.find((each) => each.id === id);
        if (row?.saved_view_source) row.saved_view_source = { ...row.saved_view_source, hidden };
        return Promise.resolve();
      },
      translateView: (id) => {
        h.translated.push(id);
        return Promise.resolve(null);
      },
      openView: (id) => {
        h.opened.push(id);
        const row = h.views.find((each) => each.id === id);
        if (row?.saved_view_source) row.saved_view_source = { ...row.saved_view_source, new: false };
        return Promise.resolve();
      },
      board: (perColumn, query, _savedView, lanes) => {
        reads.push(lanes ? { perColumn, query, lanes } : { perColumn, query });
        // Grammar keys narrow nothing here; the reads record them. The words are the needles, a phrase whole.
        const words = needles(query ?? "");
        const kept = corpus.filter(({ card }) =>
          words.every((word) => `${card.key} ${card.title}`.toLowerCase().includes(word)),
        );
        const board: MiniBoard = { columns: [], sources: [], lanes: [], more_lanes: 0 };
        for (const status of ["To Do", "In Progress", "Done"]) {
          // Each card carries the status it stands in (#1423).
          const cards = kept.filter((entry) => entry.status === status).map((entry) => ({ ...entry.card, status }));
          if (cards.length === 0) continue;
          board.columns.push({
            status,
            cards: lanes || perColumn === null ? cards : cards.slice(0, perColumn),
            total: cards.length,
          });
        }
        if (lanes) return Promise.resolve(inLanes(board, lanes, perColumn));
        return Promise.resolve(board);
      },
      detail: (id) => Promise.resolve({ body_text: `The description of ${id}.` }),
      // No longer a port: a page that still asked would count here (#1502).
      ...{
        contexts: () => {
          h.contextReads++;
          return Promise.resolve(contexts);
        },
      },
      // The mirror's rows, as the job's section reads ask for them: scoped by
      // what a ticket reaches, or (what no read should ask any more) by a
      // stored context's membership.
      listEntities: (filter) => {
        h.lists.push(filter);
        const ids =
          filter.reach != null
            ? new Set((h.reach[filter.reach] ?? []).map((row) => row.entity_id))
            : filter.context !== null
              ? new Set(h.members[filter.context] ?? [])
              : new Set(h.rows.map((row) => row.entity_id));
        // Asked for CI with no kinds, the mirror reads the run kinds CI's
        // fillers declare (#1295); here, the fixture's builds.
        const roleKinds = filter.role === "ci" ? ["build"] : null;
        const rows = h.rows.filter(
          (row) =>
            ids.has(row.entity_id) &&
            (filter.kinds.length === 0 || filter.kinds.includes(row.kind)) &&
            (roleKinds === null || roleKinds.includes(row.kind)),
        );
        return Promise.resolve({ rows, total: rows.length, tally: null, groups: null });
      },
      jobReach: (id) => Promise.resolve(h.reach[id] ?? []),
      suggestions: () =>
        Promise.resolve({
          fields: [
            { key: "issuetype", value: "Story", detail: "" },
            { key: "issuetype", value: "Bug", detail: "" },
            { key: "assignee", value: "@me", detail: "you" },
          ],
          parents: [],
        }),
    },
  };
  return h;
}

/** Opens the page; the filter's typing pause is none unless a test asks for one. */
async function open(h: Harness, timedTicket: string | null = null, viewId: string | null = null, typingPause = 0) {
  page = mount(TicketsPage, { target, props: { ready: true, timedTicket, ports: h.ports, viewId, typingPause } });
  await settle();
}

const columns = () =>
  [...target.querySelectorAll(".tf-col")].map((col) => ({
    head: col.querySelector("h3")!.textContent!.replace(/\s+/g, " ").trim(),
    cards: col.querySelectorAll(".tf-tcard").length,
    more: col.querySelector(".tf-more")?.textContent ?? null,
  }));
const count = () => target.querySelector(".tf-fcount")!.textContent;
const preview = () => target.querySelector<HTMLElement>(".tf-preview")!;
const field = () => target.querySelector<HTMLInputElement>("input[data-filter]")!;

function type(input: HTMLInputElement, char: string) {
  const at = input.selectionStart ?? input.value.length;
  input.value = input.value.slice(0, at) + char + input.value.slice(at);
  input.setSelectionRange(at + 1, at + 1);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

test("every synced ticket on the board: a column per status, eight cards then Show N more, Done three", async () => {
  const h = harness();
  await open(h);
  expect(h.reads[0]).toEqual({ perColumn: 8, query: null });
  expect(columns()).toEqual([
    { head: "To Do 12", cards: 8, more: "Show 4 more" },
    { head: "In Progress 1", cards: 1, more: null },
    { head: "Done 5", cards: 3, more: "Show 2 more done" },
  ]);
  expect(count()).toBe("18 tickets");
  expect(target.querySelector(".tf-views a[aria-current]")!.textContent).toContain("All tickets");
  expect(target.querySelector(".tf-views .tf-count")!.textContent).toBe("18");
});

test("the first card is selected: yellow, and the preview shows it with Start work", async () => {
  await open(harness());
  const first = target.querySelector<HTMLElement>(".tf-tcard")!;
  expect(first.classList.contains("tf-card")).toBe(true);
  expect(first.getAttribute("aria-current")).toBe("true");
  expect(target.querySelectorAll(".tf-tcard.tf-card")).toHaveLength(1);
  expect(first.textContent).toContain("Story, High");
  expect(first.querySelector(".tf-av")!.textContent).toBe("ML");
  expect(first.querySelector(".tf-cep")!.textContent).toBe("PAY-200 Payout reliability");

  expect(preview().querySelector("h2")!.textContent).toBe("Retry failed SEPA payouts");
  expect(preview().textContent).toContain("The description of jira:PAY-1.");
  const button = preview().querySelector<HTMLAnchorElement>(".tf-go .tf-primary")!;
  expect(button.textContent).toBe("Start work");
  expect(button.getAttribute("href")).toBe("#/job/jira%3APAY-1");
});

test("selecting another card moves the yellow and the preview's button follows its state", async () => {
  await open(harness(), "PAY-40");
  const pick = (key: string) => target.querySelector<HTMLButtonElement>(`.tf-tcard[data-entity="jira:${key}"]`)!;
  pick("PAY-40").click();
  await settle();
  expect(pick("PAY-40").classList.contains("tf-card")).toBe(true);
  expect(pick("PAY-1").classList.contains("tf-card")).toBe(false);
  expect(preview().querySelector(".tf-go .tf-primary")!.textContent).toBe("Resume work");
  expect(preview().textContent).toContain("Your timer is running on this ticket.");

  pick("PAY-2").click();
  await settle();
  expect(preview().querySelector(".tf-go .tf-primary")!.textContent).toBe("Open job");
  expect(preview().textContent).toContain("Assigned to jonas.becker.");
});

test("typing filters on the server, keeps focus and the caret, shows the matches and marks the words", async () => {
  const h = harness();
  await open(h);
  const input = field();
  input.focus();
  for (const char of "sepa") {
    type(input, char);
    await settle();
    expect(document.activeElement).toBe(input);
  }
  expect(input.selectionStart).toBe(4);
  expect(h.reads.at(-1)).toEqual({ perColumn: 50, query: "sepa" });
  expect(columns()).toEqual([{ head: "To Do 1", cards: 1, more: null }]);
  expect(count()).toBe("1 of 18 tickets");
  expect(target.querySelector(".tf-tcard mark")!.textContent).toBe("SEPA");

  // Into the middle of what is typed, with the results re-rendering under it.
  input.setSelectionRange(1, 1);
  type(input, " ");
  await settle();
  expect(document.activeElement).toBe(input);
  expect(input.selectionStart).toBe(2);
  expect(input.value).toBe("s epa");
});

test("a typed filter reads and draws fifty a column under its total, and Show N more reads the rest (#1024)", async () => {
  const bulk = (status: string, n: number, word: string) =>
    Array.from({ length: n }, (_, i) => ({ status, card: card(`BLK-${status[0]}${i + 1}`, `${word} ${i + 1}`) }));
  const h = harness([], [], [...CORPUS, ...bulk("To Do", 60, "Bulk"), ...bulk("Done", 55, "Bulk done")]);
  await open(h);
  for (const char of "bulk") type(field(), char);
  await settle();
  expect(columns()).toEqual([
    { head: "To Do 60", cards: 50, more: "Show 10 more" },
    { head: "Done 55", cards: 50, more: "Show 5 more done" },
  ]);
  expect(count()).toBe("115 of 133 tickets");

  target.querySelector<HTMLButtonElement>(".tf-col .tf-more")!.click();
  await settle();
  expect(columns()).toEqual([
    { head: "To Do 60", cards: 60, more: null },
    { head: "Done 55", cards: 50, more: "Show 5 more done" },
  ]);
});

test("no match says so and offers to clear the filter", async () => {
  await open(harness());
  type(field(), "q");
  type(field(), "z");
  await settle();
  expect(target.querySelector(".tf-empty")!.textContent).toContain("No tickets match.");
  target.querySelector<HTMLButtonElement>(".tf-empty button")!.click();
  await settle();
  expect(field().value).toBe("");
  expect(columns()).toHaveLength(3);
});

test("a chip and a dropdown are sent as the grammar's keys", async () => {
  const h = harness();
  await open(h);
  const chip = [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Type"] button')].find(
    (b) => b.textContent === "Bug",
  )!;
  chip.click();
  await settle();
  expect(h.reads.at(-1)!.query).toBe("issuetype:Bug");
  const select = target.querySelector<HTMLSelectElement>(".tf-fsel select")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["Any", "Me"]);
  select.value = "@me";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(h.reads.at(-1)).toEqual({ perColumn: 8, query: "issuetype:Bug assignee:@me" });
});

test("Show N more reads the column whole and draws all of it", async () => {
  const h = harness();
  await open(h);
  target.querySelector<HTMLButtonElement>(".tf-col .tf-more")!.click();
  await settle();
  expect(columns()[0]).toEqual({ head: "To Do 12", cards: 12, more: null });
  expect(columns()[2]!.cards).toBe(3);
});

test("the list draws the same tickets newest first, the selected row yellow", async () => {
  await open(harness());
  [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')]
    .find((b) => b.textContent === "List")!
    .click();
  await settle();
  const head = [...target.querySelectorAll(".tf-tbl th")].map((th) => th.textContent);
  expect(head).toEqual(["Key", "Summary", "Status", "Priority", "Signals", "Assignee", "Updated"]);
  const rows = [...target.querySelectorAll<HTMLElement>(".tf-tbl tbody tr")];
  expect(rows[0]!.querySelector("td")!.textContent).toBe("PAY-1");
  expect(rows[0]!.classList.contains("tf-card")).toBe(true);
  expect(rows[0]!.textContent).toContain("PAY-200");
  expect(target.querySelector(".tf-list-more")!.textContent).toBe("Show 4 more");
});

test("the list's Show N more reads every ticket, and the board keeps its caps", async () => {
  const h = harness();
  await open(h);
  const layout = (name: string) =>
    [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')]
      .find((b) => b.textContent === name)!
      .click();
  layout("List");
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-list-more")!.click();
  await settle();
  expect(target.querySelector(".tf-list-more")).toBeNull();
  layout("Board");
  await settle();
  expect(columns()[0]).toEqual({ head: "To Do 12", cards: 8, more: "Show 4 more" });
  expect(columns()[2]!.cards).toBe(3);
});

test("an ad-hoc context draws no Not a ticket group and is never read for", async () => {
  const h = harness([
    { id: "ctx:1", kind: "adhoc", title: "Staging DB configuration", anchor_id: null, created_at: "", archived_at: null },
  ]);
  await open(h);
  const views = target.querySelector(".tf-views")!;
  expect(views.querySelector("h2")).toBeNull();
  expect(views.textContent).not.toContain("Not a ticket");
  expect(views.textContent).not.toContain("Staging DB configuration");
  expect(h.contextReads).toBe(0);
});

const around = () => [...preview().querySelectorAll(".tf-rollup > div")].map((d) => d.textContent);

// #1030: the preview counts what the job's sections show, from the job-reach
// read, whether or not anybody has opened the ticket's job yet.
test("Around this ticket counts what the ticket reaches, with no context stored for it", async () => {
  const h = harness();
  h.reach["jira:PAY-1"] = [own(PR), own(BUILD_FAILED)];
  await open(h);
  expect(around()).toEqual(["Code1 pull request", "Builds1 build, 1 failing or down"]);
  // Builds is counted as the job reads it: asked for the CI role, naming no kinds (#1295).
  const builds = h.lists.filter((filter) => filter.order === "latest_per_parent");
  expect(builds).toHaveLength(1);
  expect(builds[0]).toMatchObject({ role: "ci", kinds: [], reach: "jira:PAY-1" });
});

test("Around this ticket counts a parent's link the reach brings in and the context's membership does not", async () => {
  const h = harness([{ id: "ctx:2", kind: "ticket", title: "PAY-1", anchor_id: "jira:PAY-1", created_at: "", archived_at: null }]);
  // The job's Builds section draws both builds: one the ticket's own, one its parent's (R-7.3).
  h.reach["jira:PAY-1"] = [
    own(PR),
    own(BUILD_OK),
    { entity_id: BUILD_FAILED.entity_id, kind: "build", step: "jira:PAY-200", origin: { kind: "parent", ticket: "jira:PAY-200" } },
  ];
  h.members["ctx:2"] = [PR.entity_id, BUILD_OK.entity_id];
  await open(h);
  expect(around()).toEqual(["Code1 pull request", "Builds2 builds, 1 failing or down"]);
});

test("Around this ticket says Nothing connected yet only when the reach is empty", async () => {
  const h = harness();
  h.reach["jira:PAY-2"] = [own(PR)];
  await open(h);
  expect(around()).toEqual([]);
  expect(preview().textContent).toContain("Nothing connected yet.");
  target.querySelector<HTMLButtonElement>('.tf-tcard[data-entity="jira:PAY-2"]')!.click();
  await settle();
  expect(around()).toEqual(["Code1 pull request"]);
});

test("selecting a card never creates a context", async () => {
  const h = harness();
  h.reach["jira:PAY-1"] = [own(PR)];
  await open(h);
  for (const key of ["PAY-2", "PAY-40", "PAY-1"]) {
    target.querySelector<HTMLButtonElement>(`.tf-tcard[data-entity="jira:${key}"]`)!.click();
    await settle();
  }
  expect(around()).toEqual(["Code1 pull request"]);
  expect(bridged).not.toContain("promote_context");
  expect(h.lists.every((filter) => filter.context === null)).toBe(true);
});

test("Enter on a card opens its ticket page, not the job (spec #2227)", async () => {
  await open(harness());
  const first = target.querySelector<HTMLButtonElement>(".tf-tcard")!;
  first.focus();
  first.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  expect(location.hash).toBe("#/ticket/jira%3APAY-1");
});

test("Enter on a focused card that is not selected opens that card's ticket", async () => {
  await open(harness());
  const second = target.querySelectorAll<HTMLButtonElement>(".tf-tcard")[1]!;
  const key = second.querySelector(".tf-k")!.textContent;
  expect(second.classList.contains("tf-card")).toBe(false);
  second.focus();
  second.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  expect(location.hash).toBe(`#/ticket/jira%3A${key}`);
});

const layoutButton = (name: string) =>
  [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')].find((b) => b.textContent === name)!;
const lanesSelect = () => target.querySelector<HTMLSelectElement>('select[aria-label="Lanes"]');
function chooseLanes(value: string) {
  const select = lanesSelect()!;
  select.value = value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
}
function press(key: string, init: KeyboardEventInit = {}) {
  const at = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
  at.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  flushSync();
}
const selectedKey = () =>
  target.querySelector('[aria-current="true"][data-entity]')?.getAttribute("data-entity")?.replace("jira:", "") ?? null;

test("lanes split the board into bands by size, each with its columns and count, read in lanes", async () => {
  const h = harness();
  await open(h);
  chooseLanes("assignee");
  await settle();
  // Grouped and capped on the server (#939): eight a lane's column, ten lanes.
  expect(h.reads.at(-1)).toEqual({ perColumn: 8, query: null, lanes: { by: "assignee", first: 10 } });
  const lanes = [...target.querySelectorAll(".tf-lane")].map((lane) => ({
    head: lane.querySelector("h2")!.textContent!.replace(/\s+/g, " ").trim(),
    columns: [...lane.querySelectorAll(".tf-col")].map((col) => ({
      head: col.querySelector("h3")!.textContent!.replace(/\s+/g, " ").trim(),
      cards: col.querySelectorAll(".tf-tcard").length,
      more: col.querySelector(".tf-more")?.textContent ?? null,
    })),
  }));
  expect(lanes).toEqual([
    {
      head: "jonas.becker 11",
      columns: [
        { head: "To Do 11", cards: 8, more: "Show 3 more" },
        { head: "In Progress 0", cards: 0, more: null },
        { head: "Done 0", cards: 0, more: null },
      ],
    },
    {
      head: "Unassigned 5",
      columns: [
        { head: "To Do 0", cards: 0, more: null },
        { head: "In Progress 0", cards: 0, more: null },
        { head: "Done 5", cards: 3, more: "Show 2 more done" },
      ],
    },
    {
      head: "mara.lindqvist 2",
      columns: [
        { head: "To Do 1", cards: 1, more: null },
        { head: "In Progress 1", cards: 1, more: null },
        { head: "Done 0", cards: 0, more: null },
      ],
    },
  ]);
  // One lane's Show N more reads that column's count and expands it only.
  target.querySelector<HTMLButtonElement>(".tf-lane .tf-more")!.click();
  await settle();
  expect(target.querySelectorAll(".tf-lane")[0]!.querySelectorAll(".tf-tcard").length).toBe(11);
  expect(target.querySelectorAll(".tf-lane")[1]!.querySelectorAll(".tf-tcard").length).toBe(3);
  // Another field is another read of lanes.
  chooseLanes("priority");
  await settle();
  expect(h.reads.at(-1)!.lanes).toEqual({ by: "priority", first: 10 });
});

test("a laned board draws its first ten lanes and Show N more lanes reads ten more (#939)", async () => {
  const labelled = Array.from({ length: 12 }, (_, i) => ({
    status: "To Do",
    card: card(`PAY-${i + 1}`, `Ticket ${i + 1}`, {
      labels: [`label-${String(i + 1).padStart(2, "0")}`],
      updated_at: `2026-08-${String(22 - i).padStart(2, "0")}T10:00:00Z`,
    }),
  }));
  const h = harness([], [], labelled);
  await open(h);
  chooseLanes("label");
  await settle();
  const heads = () => [...target.querySelectorAll(".tf-lane h2")].map((head) => head.textContent!.trim());
  expect(heads()).toHaveLength(10);
  expect(heads()[0]).toBe("label-01 1");
  const more = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Show 2 more lanes")!;
  expect(more).toBeDefined();
  more.click();
  await settle();
  expect(heads()).toHaveLength(12);
  expect([...target.querySelectorAll("button")].some((b) => b.textContent?.includes("more lanes"))).toBe(false);
});

test("j/k and the arrows move the selection through the visible cards in reading order across lanes", async () => {
  await open(harness());
  chooseLanes("assignee");
  await settle();
  const reading = [
    ...["PAY-2", "PAY-3", "PAY-4", "PAY-5", "PAY-6", "PAY-7", "PAY-8", "PAY-9"],
    ...["PAY-50", "PAY-51", "PAY-52"],
    ...["PAY-1", "PAY-40"],
  ];
  expect(selectedKey()).toBe(reading[0]);
  const walked = [selectedKey()];
  for (let step = 1; step < reading.length; step += 1) {
    press(step % 2 === 0 ? "j" : "ArrowDown");
    walked.push(selectedKey());
  }
  expect(walked).toEqual(reading);
  // The selected card has the focus, so the next key starts from it.
  expect(document.activeElement?.getAttribute("data-entity")).toBe("jira:PAY-40");
  press("j");
  expect(selectedKey()).toBe("PAY-40");
  press("k");
  expect(selectedKey()).toBe("PAY-1");
  press("ArrowUp");
  expect(selectedKey()).toBe("PAY-52");
  // A modifier, or a key typed in the filter, moves nothing.
  press("j", { metaKey: true });
  expect(selectedKey()).toBe("PAY-52");
  field().focus();
  press("j");
  expect(selectedKey()).toBe("PAY-52");
  expect(preview().textContent).toContain("PAY-52");
});

test("j/k walk the list's rows in their sorted order, and a double click opens the ticket page", async () => {
  await open(harness());
  layoutButton("List").click();
  await settle();
  const keyHeader = [...target.querySelectorAll<HTMLButtonElement>(".tf-tbl th button")].find(
    (b) => b.textContent === "Key",
  )!;
  keyHeader.click();
  await settle();
  const first = [...target.querySelectorAll(".tf-tbl tbody tr")].map((row) => row.querySelector("td")!.textContent);
  expect(first.slice(0, 3)).toEqual(["PAY-1", "PAY-2", "PAY-3"]);
  press("j");
  press("j");
  expect(selectedKey()).toBe("PAY-3");
  target.querySelectorAll<HTMLElement>(".tf-tbl tbody tr")[4]!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  expect(location.hash).toBe("#/ticket/jira%3APAY-5");
  location.hash = "#/tickets";
  layoutButton("Board").click();
  await settle();
  target.querySelectorAll<HTMLElement>(".tf-tcard")[1]!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  expect(location.hash).toBe("#/ticket/jira%3APAY-2");
});

test("list headers sort on click and show the direction; Key and Status never wrap", async () => {
  await open(harness());
  layoutButton("List").click();
  await settle();
  const header = (name: string) =>
    [...target.querySelectorAll<HTMLElement>(".tf-tbl th")].find((th) => th.textContent === name)!;
  const keys = () => [...target.querySelectorAll(".tf-tbl tbody tr")].map((row) => row.querySelector("td")!.textContent);
  expect(header("Updated").getAttribute("aria-sort")).toBe("descending");
  expect(header("Key").getAttribute("aria-sort")).toBeNull();
  expect(header("Signals").querySelector("button")).toBeNull();
  header("Key").querySelector("button")!.click();
  await settle();
  expect(header("Key").getAttribute("aria-sort")).toBe("ascending");
  expect(header("Updated").getAttribute("aria-sort")).toBeNull();
  expect(keys().slice(0, 2)).toEqual(["PAY-1", "PAY-2"]);
  header("Key").querySelector("button")!.click();
  await settle();
  expect(header("Key").getAttribute("aria-sort")).toBe("descending");
  expect(keys()[0]).toBe("PAY-54");
  const row = target.querySelector(".tf-tbl tbody tr")!;
  const cells = [...row.querySelectorAll("td")];
  expect(cells[0]!.classList.contains("tf-nowrap")).toBe(true);
  expect(cells[2]!.classList.contains("tf-nowrap")).toBe(true);
});

test("the Board / List and Lanes switches hold for the session only: nothing is saved, a restart forgets them", async () => {
  const h = harness();
  const written: string[] = [];
  const setItem = Storage.prototype.setItem;
  Storage.prototype.setItem = function (key: string, value: string) {
    written.push(key);
    return setItem.call(this, key, value);
  };
  try {
    await open(h);
    chooseLanes("epic");
    await settle();
    expect(lanesSelect()!.value).toBe("epic");
    layoutButton("List").click();
    await settle();
    expect(lanesSelect()).toBeNull();
    // Away and back within the session: both switches held.
    unmount(page!);
    page = null;
    await open(h);
    expect(layoutButton("List").getAttribute("aria-pressed")).toBe("true");
    layoutButton("Board").click();
    await settle();
    expect(lanesSelect()!.value).toBe("epic");
    expect(written).toEqual([]);
  } finally {
    Storage.prototype.setItem = setItem;
  }
  // A restart: the session is new, and the board opens as it always does.
  unmount(page!);
  page = null;
  forgetTicketsSession();
  await open(h);
  expect(layoutButton("Board").getAttribute("aria-pressed")).toBe("true");
  expect(lanesSelect()!.value).toBe("none");
});

test("signals are drawn on the card and in the list's Signals column, each a tone dot and its words", async () => {
  await open(harness());
  const signals = (root: Element) =>
    [...root.querySelectorAll(".tf-signal")].map((s) => [s.querySelector(".tf-dot")!.className, s.textContent]);
  const cardOf = target.querySelector('.tf-tcard[data-entity="jira:PAY-40"]')!;
  expect(signals(cardOf)).toEqual([
    ["tf-dot tf-err", "build failed"],
    ["tf-dot tf-info", "PR #142 open"],
  ]);
  layoutButton("List").click();
  await settle();
  const row = target.querySelector('.tf-tbl tr[data-entity="jira:PAY-40"]')!;
  expect(signals(row.querySelectorAll("td")[4]!)).toEqual([
    ["tf-dot tf-err", "build failed"],
    ["tf-dot tf-info", "PR #142 open"],
  ]);
});

test("a card counts its comments above zero and names its epic, as the preview's Epic row does", async () => {
  await open(harness());
  const mine = target.querySelector('.tf-tcard[data-entity="jira:PAY-1"]')!;
  expect(mine.querySelector(".tf-comments")!.textContent).toBe("3 comments");
  const epicRow = () => [...preview().querySelectorAll("dt")].find((dt) => dt.textContent === "Epic")!;
  expect(epicRow().nextElementSibling!.textContent).toBe("PAY-200 Payout reliability");

  // A count of none draws nothing, nor does a count that never answered,
  // and an epic the mirror does not hold is its key alone (ADR-0007).
  const other = target.querySelector<HTMLElement>('.tf-tcard[data-entity="jira:PAY-40"]')!;
  expect(other.querySelector(".tf-comments")).toBeNull();
  expect(target.querySelector('.tf-tcard[data-entity="jira:PAY-2"] .tf-comments')).toBeNull();
  expect(other.querySelector(".tf-cep")!.textContent).toBe("PAY-300");
  other.click();
  await settle();
  expect(epicRow().nextElementSibling!.textContent).toBe("PAY-300");
});

test("a view whose cards leave the comment count out draws none", async () => {
  await open(harness([], [view("v", "No talk", "Mine", 0, { card_fields: ["type", "epic"] })]), null, "v");
  const mine = target.querySelector('.tf-tcard[data-entity="jira:PAY-1"]')!;
  expect(mine.querySelector(".tf-comments")).toBeNull();
  expect(mine.querySelector(".tf-cep")!.textContent).toBe("PAY-200 Payout reliability");
});

// ---- ticket views (#812) ----

function view(id: string, name: string, group: string, position: number, extra: Partial<TicketViewRow> = {}): TicketViewRow {
  return {
    id,
    name,
    group,
    position,
    query: "",
    layout: "board",
    columns: [],
    lanes: "none",
    card_fields: ["type", "signals", "comments", "assignee"],
    list_columns: ["key", "summary", "status", "priority", "signals", "assignee", "updated"],
    list_sort: null,
    saved_view: null,
    count: 3,
    unreadable: null,
    saved_view_source: null,
    ...extra,
  };
}

const VIEWS: TicketViewRow[] = [
  view("ops-board", "OPS board", "Ops", 0, { query: "project:OPS", count: 0 }),
  view("backlog", "Payments backlog", "Team", 1, { query: "project:PAY status:\"To Do\"", count: 12 }),
  view("my-work", "My work", "Mine", 2, { query: "assignee:@me", count: 2 }),
  view("payouts", "Payouts", "Epics", 3, { query: "under:PAY-200 payout", count: 1 }),
  view("pay", "Payments (PAY)", "Projects", 4, { query: "project:PAY", count: 18 }),
];

const pane = () =>
  [...target.querySelectorAll(".tf-vlist > h2, .tf-vlist .tf-vw-row a")].map((el) =>
    el.tagName === "H2" ? `# ${el.textContent}` : el.textContent!.replace(/\s+/g, " ").trim(),
  );

function menuOf(name: string): HTMLButtonElement {
  return target.querySelector<HTMLButtonElement>(`button[aria-label="More for ${name}"]`)!;
}

function menuItem(label: string): HTMLButtonElement {
  return [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].find((b) => b.textContent === label)!;
}

const dialog = () => document.querySelector<HTMLElement>("[role=dialog]");

test("the views pane: Mine, Team, Epics and Projects first, then typed groups, each view with its count above zero", async () => {
  await open(harness([], VIEWS));
  expect(pane()).toEqual([
    "# Mine",
    "My work 2",
    "# Team",
    "Payments backlog 12",
    "# Epics",
    "Payouts 1",
    "# Projects",
    "Payments (PAY) 18",
    "# Ops",
    "OPS board",
  ]);
});

test("the marks are what the server read: a phrase whole, never an or, a -word or a key (#1903)", async () => {
  const h = harness();
  await open(h);
  const input = field();
  type(input, 'or "failed sepa" -blocked project:PAY');
  await settle();
  const marked = [...target.querySelectorAll(".tf-tcard mark")].map((m) => m.textContent);
  expect(marked).toEqual(["failed SEPA"]);
});

test("Find a view narrows the pane by name and by query, and marks the name", async () => {
  await open(harness([], VIEWS));
  const find = target.querySelector<HTMLInputElement>('input[aria-label="Find a view"]')!;
  find.value = "pay";
  find.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  // "Payments backlog" and "Payments (PAY)" by name, "Payouts" by name, "OPS board" not at all.
  expect(pane()).toEqual(["# Team", "Payments backlog 12", "# Epics", "Payouts 1", "# Projects", "Payments (PAY) 18"]);
  expect(target.querySelector(".tf-vw-row mark")!.textContent).toBe("Pay");
  find.value = "assignee:@me";
  find.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(pane()).toEqual(["# Mine", "My work 2"]);
  expect(target.querySelector(".tf-vlist")!.textContent).not.toContain("All tickets");
});

test("the board head is the canonical header: title left, one group right with the switch, the actions at default size, Sync now last (#2293)", async () => {
  const h = harness([], [view("mine", "My open bugs", "Mine", 0, { query: "assignee:@me issuetype:Bug" })]);
  const sync = createRawSnippet(() => ({ render: () => '<button type="button" class="tf-btn tf-syncnow">Sync now</button>' }));
  page = mount(TicketsPage, { target, props: { ready: true, timedTicket: null, ports: h.ports, viewId: "mine", typingPause: 0, sync } });
  await settle();
  const head = target.querySelector<HTMLElement>(".tf-board-head")!;
  expect(head.matches("header.tf-fhead")).toBe(true);
  const [title, acts, ...rest] = [...head.children];
  expect(rest).toEqual([]);
  expect(title!.querySelector("h1")!.textContent).toBe("My open bugs");
  expect(title!.querySelector(".tf-q")!.textContent).toBe("assignee:@me issuetype:Bug");
  expect(acts!.matches(".tf-acts")).toBe(true);
  expect(acts!.firstElementChild!.matches('.tf-seg[aria-label="Layout"]')).toBe(true);
  const buttons = [...acts!.querySelectorAll<HTMLButtonElement>(":scope > button, :scope > * > button")].filter((b) => !b.closest(".tf-seg"));
  expect(buttons.map((b) => b.textContent!.trim())).toEqual(["Edit view", "⋯", "Sync now"]);
  for (const b of buttons) {
    expect(b.classList.contains("tf-btn")).toBe(true);
    expect(b.classList.contains("tf-sm")).toBe(false);
  }
  expect(acts!.lastElementChild!.textContent).toBe("Sync now");
});

test("the preview's go button is the large decision (#2293)", async () => {
  await open(harness());
  const go = preview().querySelector(".tf-go .tf-primary")!;
  expect(go.classList.contains("tf-btn")).toBe(true);
  expect(go.classList.contains("tf-lg")).toBe(true);
});

test("a view opens with its text as the filter bar's base, its name and text in the header, and its own layout", async () => {
  const h = harness([], [view("mine", "My open bugs", "Mine", 0, { query: "assignee:@me issuetype:Bug", layout: "list" })]);
  await open(h, null, "mine");
  expect(h.reads[0]).toEqual({ perColumn: 8, query: "assignee:@me issuetype:Bug" });
  expect(target.querySelector(".tf-board-head h1")!.textContent).toBe("My open bugs");
  expect(target.querySelector(".tf-q")!.textContent).toBe("assignee:@me issuetype:Bug");
  expect(target.querySelector('.tf-views a[aria-current="true"]')!.textContent).toContain("My open bugs");
  expect(target.querySelector(".tf-tbl")).not.toBeNull();
  // The filter's words go after the view's text.
  type(field(), "r");
  await settle();
  expect(h.reads.at(-1)!.query).toBe("assignee:@me issuetype:Bug r");
});

test("a view's switches hold for the session over what it was saved with, and each view keeps its own", async () => {
  const h = harness([], [view("a", "A", "Mine", 0, { layout: "list" }), view("b", "B", "Mine", 1, { lanes: "assignee" })]);
  await open(h, null, "a");
  target.querySelector<HTMLButtonElement>('[aria-label="Layout"] button')!.click();
  await settle();
  expect(target.querySelector(".tf-tbl")).toBeNull();
  unmount(page!);
  await open(h, null, "b");
  expect(target.querySelector<HTMLSelectElement>('select[aria-label="Lanes"]')!.value).toBe("assignee");
  unmount(page!);
  await open(h, null, "a");
  expect(target.querySelector(".tf-tbl")).toBeNull();
  expect(h.saved).toEqual([]);
});

test("a view whose text the grammar cannot read opens as text with a line naming the part, and reads no tickets", async () => {
  const h = harness([], [
    view("next", "Next sprint", "Mine", 0, { query: "sprint:next retry", count: null, unreadable: "sprint:next" }),
  ]);
  await open(h, null, "next");
  expect(h.reads).toEqual([]);
  expect(target.querySelector(".tf-q")!.textContent).toBe("sprint:next retry");
  expect(target.querySelector(".tf-vw-warn")!.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "The search cannot read sprint:next in this view, so it shows no tickets.",
  );
  expect(target.querySelector("input[data-filter]")).toBeNull();
  expect(target.querySelector(".tf-views .tf-vw-row .tf-count")).toBeNull();
});

test("Save as a view saves the view's text plus the chips, dropdowns and words, with how the tickets show", async () => {
  const h = harness([], [view("pay", "Payments (PAY)", "Projects", 0, { query: "project:PAY" })]);
  await open(h, null, "pay");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Story")!.click();
  type(field(), "r");
  target.querySelector<HTMLButtonElement>('[aria-label="Layout"] button:last-child')!.click();
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save as a view")!.click();
  await settle();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(dialog()!.querySelector("code")!.textContent).toBe("project:PAY issuetype:Story r");
  expect(dialog()!.textContent).toContain("Kept in knobas. No source sees it.");
  const create = [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Create view")!;
  expect(create.disabled).toBe(true);
  const name = dialog()!.querySelector<HTMLInputElement>("input")!;
  name.value = "PAY stories";
  name.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(create.disabled).toBe(false);
  create.click();
  await settle();
  expect(h.saved).toEqual([
    expect.objectContaining({
      id: null,
      name: "PAY stories",
      group: "Projects",
      query: "project:PAY issuetype:Story r",
      layout: "list",
      saved_view: null,
    }),
  ]);
  expect(dialog()).toBeNull();
  expect(location.hash).toBe("#/tickets/pay-stories");
  expect(toasts.items.at(-1)!.text).toBe("Created PAY stories");
});

test("Delete from a row's menu goes, and the toast's Undo puts it back where it was", async () => {
  const h = harness([], VIEWS);
  await open(h, null, "backlog");
  menuOf("Payments backlog").click();
  await settle();
  menuItem("Delete").click();
  await settle();
  expect(h.deleted).toEqual(["backlog"]);
  expect(location.hash).toBe("#/tickets");
  expect(pane()).not.toContain("Payments backlog 12");
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Deleted Payments backlog");
  toast.action!.run();
  await settle();
  expect(h.saved.at(-1)).toEqual(expect.objectContaining({ id: "backlog", position: 1, query: 'project:PAY status:"To Do"' }));
  expect(pane()).toContain("Payments backlog 1");
});

test("Duplicate opens New view on a copy called Copy of …, with the view's text", async () => {
  await open(harness([], VIEWS));
  menuOf("My work").click();
  await settle();
  menuItem("Duplicate").click();
  await settle();
  expect(dialog()!.querySelector<HTMLInputElement>("input")!.value).toBe("Copy of My work");
  expect(dialog()!.querySelector("code")!.textContent).toBe("assignee:@me");
});

test("a view's own columns gather statuses, count them, and show an over-limit count as N of M in err tone", async () => {
  const h = harness([], [
    view("flow", "Flow", "Team", 0, {
      columns: [
        { name: "Open", statuses: ["To Do", "In Progress"], limit: 10 },
        { name: "Done", statuses: ["Done"], limit: null },
      ],
    }),
  ]);
  await open(h, null, "flow");
  expect(columns()).toEqual([
    { head: "Open 13 of 10", cards: 8, more: "Show 5 more" },
    { head: "Done 5", cards: 3, more: "Show 2 more done" },
  ]);
  expect(target.querySelector(".tf-col .tf-count")!.classList.contains("tf-err")).toBe(true);
});

test("a view's list columns and card fields are what it was saved with", async () => {
  const h = harness([], [
    view("lean", "Lean", "Mine", 0, { list_columns: ["key", "summary", "epic"], card_fields: ["signals"] }),
  ]);
  await open(h, null, "lean");
  const first = target.querySelector(".tf-tcard")!;
  expect(first.querySelector(".tf-tkind")).toBeNull();
  expect(first.querySelector(".tf-av")).toBeNull();
  expect(first.querySelector(".tf-cep")).toBeNull();
  target.querySelector<HTMLButtonElement>('[aria-label="Layout"] button:last-child')!.click();
  await settle();
  expect([...target.querySelectorAll(".tf-tbl th")].map((th) => th.textContent)).toEqual(["Key", "Summary", "Epic"]);
});

test("an address naming no view says so and offers All tickets", async () => {
  const h = harness([], VIEWS);
  await open(h, null, "gone");
  expect(target.querySelector(".tf-empty")!.textContent).toContain("No view is called gone.");
  expect(h.reads).toEqual([]);
});

// ---- the view editor (#814) ----

/**
 * The preview reads a moment after a change (`ViewEditor`'s debounce): wait for that read
 * to reach the harness, then let its answer draw. A fixed 200 ms sleep here lost the race
 * on a two-core runner (#2048). The preview asks for nine cards a column, or whole lanes
 * (`null`); the editor's statuses read asks for one and the page's own reads for 8 or 50.
 */
async function previewed(h: Harness) {
  const sent = h.reads.length;
  const preview = (read: Harness["reads"][number]) => read.perColumn === 9 || read.perColumn === null;
  await until(() => h.reads.slice(sent).some(preview), "the preview's debounced read");
  await settle();
}

const inDialog = (label: string) =>
  [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === label)!;
const searchLine = () => dialog()!.querySelector(".tf-vw-q code")!;
const previewCols = () =>
  [...dialog()!.querySelectorAll(".tf-vw-mcol")].map((col) => col.querySelector(".tf-vw-mh")!.textContent!.trim());

function input(element: HTMLInputElement | HTMLSelectElement, value: string) {
  element.value = value;
  element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
}

test("+ New view opens the editor: conditions write the search line, the preview counts and draws, Create view saves it", async () => {
  const h = harness();
  await open(h);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-vw-acts button")].find((b) => b.textContent === "+ New view")!.click();
  await previewed(h);
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(dialog()!.textContent).toContain("Kept in knobas. No source sees it.");
  expect(dialog()!.textContent).toContain("No conditions yet, so it holds every synced ticket.");
  expect(searchLine().textContent).toBe("every synced ticket");
  // Every ticket, one column per status of the board.
  expect(dialog()!.querySelector(".tf-vw-n b")!.textContent).toBe("18");
  expect(previewCols()).toEqual(["To Do12", "In Progress1", "Done5"]);
  expect(inDialog("Create view").disabled).toBe(true);

  inDialog("+ Add a condition").click();
  await settle();
  // The first field is Project; pick Issue type, Bug, then is not.
  input(dialog()!.querySelector<HTMLSelectElement>('select[aria-label="Field"]')!, "issuetype");
  await settle();
  inDialog("Bug").click();
  inDialog("is not").click();
  input(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Words in the key or summary"]')!, "payout");
  await previewed(h);
  expect(searchLine().textContent).toBe("-issuetype:Bug payout");
  expect(h.reads.at(-1)!.query).toBe("-issuetype:Bug payout");

  input(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Name"]')!, "Payout work");
  await settle();
  expect(inDialog("Create view").disabled).toBe(false);
  inDialog("Create view").click();
  await settle();
  expect(h.saved).toEqual([
    expect.objectContaining({ id: null, name: "Payout work", group: "Mine", query: "-issuetype:Bug payout", columns: [] }),
  ]);
  expect(dialog()).toBeNull();
  expect(toasts.items.at(-1)!.text).toBe("Created Payout work");
  expect(location.hash).toBe("#/tickets/payout-work");
});

test("Edit view opens the text as conditions, saves over the view under its id, and keeps a text left alone as typed", async () => {
  const h = harness([], [view("bugs", "Bugs", "Mine", 0, { query: 'issuetype:Bug   assignee:me "sepa retry"' })]);
  await open(h, null, "bugs");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "Edit view")!.click();
  await previewed(h);
  expect(dialog()!.querySelector("h2")!.textContent).toBe("Edit Bugs");
  const fields = [...dialog()!.querySelectorAll<HTMLSelectElement>('select[aria-label="Field"]')].map((s) => s.value);
  expect(fields).toEqual(["issuetype", "assignee"]);
  // `me` is the Me the suggestions offer as `@me`.
  expect(inDialog("Me").getAttribute("aria-pressed")).toBe("true");
  expect(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Words in the key or summary"]')!.value).toBe('"sepa retry"');
  inDialog("Save view").click();
  await settle();
  expect(h.saved.at(-1)).toEqual(
    expect.objectContaining({ id: "bugs", position: 0, query: 'issuetype:Bug   assignee:me "sepa retry"' }),
  );
  expect(toasts.items.at(-1)!.text).toBe("Saved Bugs");

  // Changed, it writes new text.
  menuOf("this view").click();
  await settle();
  menuItem("Edit view").click();
  await previewed(h);
  inDialog("Story").click();
  await settle();
  inDialog("Save view").click();
  await settle();
  expect(h.saved.at(-1)!.query).toBe('issuetype:Bug issuetype:Story assignee:me "sepa retry"');
});

test("each status sits on one column, an over-limit column is err, tickets on no column are called out, and One column per status resets", async () => {
  const h = harness();
  await open(h);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-vw-acts button")].find((b) => b.textContent === "+ New view")!.click();
  await previewed(h);
  const cols = () => [...dialog()!.querySelectorAll<HTMLElement>(".tf-vw-col")];
  const pressed = (col: HTMLElement) =>
    [...col.querySelectorAll<HTMLButtonElement>(".tf-vw-val")].filter((b) => b.getAttribute("aria-pressed") === "true").map((b) => b.textContent);
  // In Progress onto the first column leaves its own.
  [...cols()[0]!.querySelectorAll<HTMLButtonElement>(".tf-vw-val")].find((b) => b.textContent === "In Progress")!.click();
  await settle();
  expect(cols().map(pressed)).toEqual([["To Do", "In Progress"], [], ["Done"]]);
  input(cols()[0]!.querySelector<HTMLInputElement>('input[aria-label="At most"]')!, "10");
  // Done off its column: on no column.
  [...cols()[2]!.querySelectorAll<HTMLButtonElement>(".tf-vw-val")].find((b) => b.textContent === "Done")!.click();
  // Columns and limits redraw the answer already read; only the query reads again.
  await settle();
  expect(dialog()!.textContent).toContain("Done is on no column, so those tickets do not show on the board.");
  expect(previewCols()).toEqual(["To Do13 of 10", "In Progress0", "Done0"]);
  expect(dialog()!.querySelector(".tf-vw-mcol")!.classList.contains("tf-vw-over")).toBe(true);
  expect(dialog()!.textContent).toContain("5 of them sit in a status on no column.");
  inDialog("One column per status").click();
  await settle();
  expect(cols().map(pressed)).toEqual([["To Do"], ["In Progress"], ["Done"]]);
  expect(dialog()!.textContent).not.toContain("on no column");
});

test("the search line underlines what the grammar does not read, and Signal is a field only once the signals answer", async () => {
  const h = harness([], [view("odd", "Odd", "Mine", 0, { query: "sprint:next -under:PAY-200 label:sepa" })]);
  await open(h, null, "odd");
  menuOf("this view").click();
  await settle();
  menuItem("Edit view").click();
  await previewed(h);
  expect([...searchLine().querySelectorAll(".tf-vw-newq")].map((s) => s.textContent)).toEqual(["sprint:next", "-under:PAY-200"]);
  expect(searchLine().textContent).toBe("sprint:next -under:PAY-200 label:sepa");
  const options = () => [...dialog()!.querySelector('select[aria-label="Field"]')!.querySelectorAll("option")].map((o) => o.textContent);
  // The harness's suggestions name no `has:` values.
  expect(options()).not.toContain("Signal");
  inDialog("Cancel").click();
  await settle();

  const ports = h.ports;
  h.ports = {
    ...ports,
    suggestions: () =>
      Promise.resolve({ fields: [{ key: "has", value: "blocker", detail: "something blocks it" }], parents: [] }),
  };
  unmount(page!);
  await open(h, null, "odd");
  menuOf("this view").click();
  await settle();
  menuItem("Edit view").click();
  await previewed(h);
  expect(options()).toContain("Signal");
  input(dialog()!.querySelector<HTMLSelectElement>('select[aria-label="Field"]')!, "signal");
  await settle();
  inDialog("blocked").click();
  await settle();
  expect(searchLine().textContent).toBe("has:blocker -under:PAY-200 label:sepa");
});

test("Delete view in the editor deletes it, with Undo in the toast", async () => {
  const h = harness([], VIEWS);
  await open(h, null, "backlog");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "Edit view")!.click();
  await previewed(h);
  // #2289: an extra action that starts a destructive one, drawn by the Controls classes alone.
  expect(inDialog("Delete view").className).toBe("tf-btn tf-ghost tf-danger");
  inDialog("Delete view").click();
  await settle();
  expect(dialog()).toBeNull();
  expect(h.deleted).toEqual(["backlog"]);
  expect(toasts.items.at(-1)!.action!.label).toBe("Undo");
});

// ---- Jira views (#877) ----

const FLOW_COLUMNS = [
  { name: "Ready", statuses: ["To Do"], limit: null },
  { name: "In flight", statuses: ["In Progress", "In Review"], limit: 6 },
  { name: "Shipped", statuses: ["Done"], limit: null },
];

/** What the Jira adapter answers for *Payments and Ops flow*'s query (its own tests hold the reading, #1208). */
const FLOW_COPY: QueryTranslation = {
  conditions: [{ key: "project", negated: false, values: ["PAY", "OPS"] }],
  kept: 1,
  left_out: [{ clause: "fixVersion is EMPTY", reason: "knobas does not sync fix versions" }],
};

/** A Jira board view, and a harness whose board answers it under Jira's columns, recording each read. */

function jiraHarness(over: Partial<NonNullable<TicketViewRow["saved_view_source"]>> = {}): {
  h: Harness;
  jiraReads: (string | null)[];
} {
  const h = harness([], [
    view("my-work", "My work", "Mine", 0, { query: "assignee:@me", count: 2 }),
    view("jira-board-21", "Payments and Ops flow", "Jira boards", 1, {
      columns: FLOW_COLUMNS,
      saved_view: "jira:board-21",
      count: 18,
      saved_view_source: {
        kind: "board",
        system: "Jira",
        board_type: "kanban",
        source_query: "project in (PAY, OPS) ORDER BY Rank ASC",
        sub_query: "fixVersion is EMPTY",
        sprint: null,
        last_read: "2026-10-17T09:56:00Z",
        web_url: "https://jira.example/secure/RapidBoard.jspa?rapidView=21",
        hidden: false,
        new: false,
        unsynced: [],
        ...over,
      },
    }),
  ]);
  // The Jira source claims `translate_query`, and answers as the adapter does (#1208).
  sourceRoles.take([{ id: "jira", display_name: "jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions", "translate_query"] }], role_misses: [] }]);
  h.ports.translateView = (id) => {
    h.translated.push(id);
    return Promise.resolve(FLOW_COPY);
  };
  const jiraReads: (string | null)[] = [];
  const plain = h.ports.board;
  h.ports.board = async (perColumn, query, savedView) => {
    jiraReads.push(savedView ?? null);
    const whole = await plain(perColumn, query);
    if (!savedView) return whole;
    // As `saved_view_board` answers: each Jira column by its name.
    return {
      sources: whole.sources,
      lanes: [],
      more_lanes: 0,
      columns: FLOW_COLUMNS.map((own) => {
        const held = whole.columns.filter((column) => own.statuses.includes(column.status ?? ""));
        return {
          status: own.name,
          cards: held.flatMap((column) => column.cards),
          total: held.reduce((all, column) => all + column.total, 0),
        };
      }),
    };
  };
  return { h, jiraReads };
}

test("Jira boards come last, said to be from the Jira source, and the list ends with where they come from", async () => {
  const { h } = jiraHarness();
  await open(h);
  expect(pane()).toEqual(["# Mine", "My work 2", "# Jira boards from the Jira source", "Payments and Ops flow 18"]);
  const foot = target.querySelector(".tf-vw-foot")!;
  expect(foot.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "Boards and favourite filters come in with the Jira source at every sync. Choose which",
  );
  expect(foot.querySelector("a")!.getAttribute("href")).toBe("#/settings/source/jira");
});

test("a Jira view reads its board from Jira, draws Jira's columns and limits, and offers View settings only", async () => {
  const { h, jiraReads } = jiraHarness();
  await open(h, null, "jira-board-21");
  expect(jiraReads.every((ref) => ref === "jira:board-21")).toBe(true);
  expect(jiraReads.length).toBeGreaterThan(0);
  expect(columns()).toEqual([
    { head: "Ready 12", cards: 8, more: "Show 4 more" },
    { head: "In flight 1 of 6", cards: 1, more: null },
    // Done by the status it lists, not by its name (#1423): three, like *Done*.
    { head: "Shipped 5", cards: 3, more: "Show 2 more done" },
  ]);
  expect(target.querySelector(".tf-board-head .tf-q")).toBeNull();
  const settings = [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find(
    (b) => b.textContent === "View settings",
  );
  expect(settings).toBeDefined();

  menuOf("Payments and Ops flow").click();
  flushSync();
  const items = [...target.querySelectorAll("[role=menuitem]")].map((b) => b.textContent!.replace(/\s+/g, " ").trim());
  expect(items).toEqual([
    "View settingslanes, cards and list columns",
    "Make an editable copyyour own view, starting from this board",
    "Hide from the listit keeps syncing, so you can show it again",
  ]);
});

test("on a Jira view the preview and the list read the ticket's status, the column its view's name (#1423)", async () => {
  const { h } = jiraHarness();
  await open(h, null, "jira-board-21");
  // PAY-1 is To Do and mine, under *Ready*.
  expect(columns()[0]!.head).toBe("Ready 12");
  const facts = [...preview().querySelectorAll(".tf-facts dt")].map((dt) => [dt.textContent, dt.nextElementSibling!.textContent]);
  expect(facts).toContainEqual(["Status", "To Do"]);
  expect(preview().querySelector(".tf-go .tf-primary")!.textContent).toBe("Start work");

  [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')].find((b) => b.textContent === "List")!.click();
  await settle();
  const statusAt = [...target.querySelectorAll(".tf-tbl th")].findIndex((th) => th.textContent === "Status");
  const status = (key: string) =>
    [...target.querySelectorAll(".tf-tbl tbody tr")].find((tr) => tr.querySelector("td")!.textContent === key)!.children[statusAt]!
      .textContent;
  expect([status("PAY-1"), status("PAY-40"), status("PAY-50")]).toEqual(["To Do", "In Progress", "Done"]);
});

test("two sources holding PAY-1: each card and row names its source and opens by it; a unique key names none (#1476)", async () => {
  const twin = { status: "To Do", card: card("PAY-1", "Retry failed SEPA payouts", { entity_id: "gitea:PAY-1", source_id: "gitea" }) };
  sourceRoles.take([
    { id: "jira", display_name: "Tidewater Jira", roles: [], role_misses: [] },
    { id: "gitea", display_name: "Tidewater Gitea", roles: [], role_misses: [] },
  ]);
  try {
    // Counted over the cards read: the twin is among the first eight.
    await open(harness([], [], [twin, ...CORPUS]));
    const marks = () =>
      [...target.querySelectorAll<HTMLElement>(".tf-tcard")]
        .filter((el) => el.querySelector(".tf-k")!.textContent === "PAY-1" || el.querySelector(".tf-k")!.textContent === "PAY-2")
        .map((el) => [el.dataset.entity, el.querySelector(".tf-tsrc")?.textContent ?? null]);
    expect(marks()).toEqual(
      expect.arrayContaining([
        ["jira:PAY-1", "Tidewater Jira"],
        ["gitea:PAY-1", "Tidewater Gitea"],
        ["jira:PAY-2", null],
      ]),
    );
    target.querySelector<HTMLElement>('.tf-tcard[data-entity="gitea:PAY-1"]')!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    expect(location.hash).toBe("#/ticket/gitea%3APAY-1");

    [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')].find((b) => b.textContent === "List")!.click();
    await settle();
    const keyCell = (id: string) => target.querySelector(`.tf-tbl tr[data-entity="${id}"] td`)!;
    expect(keyCell("gitea:PAY-1").textContent).toBe("PAY-1Tidewater Gitea");
    expect(keyCell("jira:PAY-1").querySelector(".tf-sub")!.textContent).toBe("Tidewater Jira");
    expect(keyCell("jira:PAY-2").querySelector(".tf-sub")).toBeNull();
  } finally {
    sourceRoles.forget();
  }
});

// ---- Make an editable copy (#880) ----

const fromLine = () => dialog()!.querySelector(".tf-vw-from")!.textContent!.replace(/\s+/g, " ").trim();
const COPIED =
  "Copied from the Jira board Payments and Ops flow. It no longer follows Jira. Left out, because knobas cannot test it: fixVersion is EMPTY (knobas does not sync fix versions).";

test("Make an editable copy opens New view on your own copy: the JQL it could test as conditions, what it left out and why", async () => {
  const { h, jiraReads } = jiraHarness();
  await open(h, null, "jira-board-21");
  menuOf("Payments and Ops flow").click();
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].find((b) => b.textContent!.startsWith("Make an editable copy"))!.click();
  await settle();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Name"]')!.value).toBe("Payments and Ops flow");
  expect(fromLine()).toBe(COPIED);
  expect(searchLine().textContent).toBe("project:PAY project:OPS");
  expect(h.translated).toEqual(["jira-board-21"]);
  expect(dialog()!.textContent).toContain("Kept in knobas. Jira never sees it.");
  // Jira's columns come along, now the reader's to change.
  expect([...dialog()!.querySelectorAll<HTMLInputElement>('input[aria-label="Column name"]')].map((i) => [i.value, i.disabled])).toEqual([
    ["Ready", false],
    ["In flight", false],
    ["Shipped", false],
  ]);
  // The preview reads the copy's own text, not the Jira view's board.
  jiraReads.length = 0;
  await until(() => jiraReads.length > 0, "the copy's preview read");
  await settle();
  expect(jiraReads.every((ref) => ref === null)).toBe(true);

  [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Create view")!.click();
  await settle();
  expect(h.saved.at(-1)).toEqual(
    expect.objectContaining({
      id: null,
      name: "Payments and Ops flow",
      group: "Mine",
      query: "project:PAY project:OPS",
      columns: FLOW_COLUMNS,
      saved_view: null,
    }),
  );
});

test("View settings on a Jira view says how much a copy keeps and offers Make an editable copy", async () => {
  const { h } = jiraHarness();
  await open(h, null, "jira-board-21");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  expect(dialog()!.textContent!.replace(/\s+/g, " ")).toContain(
    "To change which tickets it holds, change it in Jira, or make your own copy. A copy keeps 1 of 2 parts of the query.",
  );
  [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Make an editable copy")!.click();
  await settle();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(fromLine()).toBe(COPIED);
});

test("a saved view from a source without translate_query offers no editable copy, and its view still shows (#1208)", async () => {
  const { h } = jiraHarness();
  sourceRoles.take([{ id: "jira", display_name: "jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions"] }], role_misses: [] }]);
  await open(h, null, "jira-board-21");
  expect(columns().map((column) => column.head)).toEqual(["Ready 12", "In flight 1 of 6", "Shipped 5"]);

  menuOf("Payments and Ops flow").click();
  flushSync();
  const items = [...target.querySelectorAll("[role=menuitem]")].map((b) => b.textContent!.replace(/\s+/g, " ").trim());
  expect(items).toEqual([
    "View settingslanes, cards and list columns",
    "Hide from the listit keeps syncing, so you can show it again",
  ]);
  menuOf("Payments and Ops flow").click();
  flushSync();
  expect([...target.querySelectorAll("button")].some((b) => b.textContent === "Save as a view")).toBe(false);

  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  const text = dialog()!.textContent!.replace(/\s+/g, " ");
  expect(text).toContain("To change which tickets it holds, change it in Jira.");
  expect(text).not.toContain("copy");
  expect([...dialog()!.querySelectorAll("button")].some((b) => b.textContent === "Make an editable copy")).toBe(false);
  expect(h.translated).toEqual([]);
});

test("Make an editable copy asks the view's source to read its query, and a source that answers none opens nothing (#1208)", async () => {
  const { h } = jiraHarness();
  h.ports.translateView = (id) => {
    h.translated.push(id);
    return Promise.resolve(null);
  };
  await open(h, null, "jira-board-21");
  menuOf("Payments and Ops flow").click();
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].find((b) => b.textContent!.startsWith("Make an editable copy"))!.click();
  await settle();
  expect(h.translated).toEqual(["jira-board-21"]);
  expect(dialog()).toBeNull();
  expect(toasts.items.at(-1)!.text).toBe("Payments and Ops flow cannot be copied: its source does not translate its query");
});

test("Save as a view on a Jira view is a copy too: the JQL it could test, then the chips and words, no name yet", async () => {
  const { h } = jiraHarness();
  await open(h, null, "jira-board-21");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent === "Story")!.click();
  type(field(), "r");
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save as a view")!.click();
  await settle();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(dialog()!.querySelector<HTMLInputElement>('input[aria-label="Name"]')!.value).toBe("");
  expect(fromLine()).toBe(COPIED);
  expect(searchLine().textContent).toBe("project:PAY project:OPS issuetype:Story r");
});

// ---- Jira filters, new and hidden (#878) ----

/** {@link jiraHarness} plus a favourite filter, the board marked as `board` says. */
function filtersHarness(board: Partial<NonNullable<TicketViewRow["saved_view_source"]>> = {}) {
  const { h, jiraReads } = jiraHarness();
  const flow = h.views[1]!;
  flow.saved_view_source = { ...flow.saved_view_source!, ...board };
  h.views.push(
    view("jira-filter-10300", "PAY without a fix version", "Jira filters", 2, {
      layout: "list",
      saved_view: "jira:filter-10300",
      count: 4,
      saved_view_source: {
        ...flow.saved_view_source!,
        kind: "filter",
        board_type: null,
        sub_query: null,
        source_query: "project = PAY AND fixVersion is EMPTY",
        new: false,
      },
    }),
  );
  return { h, jiraReads };
}

test("favourite filters come in under Jira filters, after the boards, and a new board says new", async () => {
  const { h } = filtersHarness({ new: true });
  await open(h);
  expect(pane()).toEqual([
    "# Mine",
    "My work 2",
    "# Jira boards from the Jira source",
    // Prototype-6 sets *new* right after the name, spaced by its margin.
    "Payments and Ops flownew 18",
    "# Jira filters from the Jira source",
    "PAY without a fix version 4",
  ]);
  expect(h.opened).toEqual([]);
});

test("opening a new Jira view takes its new away, once", async () => {
  const { h } = filtersHarness({ new: true });
  await open(h, null, "jira-board-21");
  expect(h.opened).toEqual(["jira-board-21"]);
  expect(pane()).toContain("Payments and Ops flow 18");
});

test("Hide from the list moves a Jira view to one hidden line, with Undo, and Show brings it back", async () => {
  const { h } = filtersHarness();
  await open(h);
  menuOf("PAY without a fix version").click();
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")].find((b) => b.textContent!.startsWith("Hide"))!.click();
  await settle();
  expect(h.hidden).toEqual([["jira-filter-10300", true]]);
  expect(pane()).not.toContain("PAY without a fix version 4");
  expect(toasts.items.at(-1)).toMatchObject({ text: "Hid PAY without a fix version", action: { label: "Undo" } });

  const line = target.querySelector<HTMLButtonElement>(".tf-vw-hidden")!;
  expect(line.textContent).toBe("1 hidden");
  line.click();
  flushSync();
  target.querySelector<HTMLButtonElement>('button[aria-label="Show PAY without a fix version again"]')!.click();
  await settle();
  expect(h.hidden.at(-1)).toEqual(["jira-filter-10300", false]);
  expect(pane()).toContain("PAY without a fix version 4");
  expect(target.querySelector(".tf-vw-hidden")).toBeNull();
});

test("View settings on a Jira view says Jira decides, locks its columns, and saves only how it shows", async () => {
  const { h } = jiraHarness();
  await open(h, null, "jira-board-21");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  const box = target.querySelector<HTMLElement>(".tf-vw-dialog")!;
  expect(box.querySelector("h2")!.textContent).toBe("Settings for Payments and Ops flow");
  expect(box.querySelector(".tf-vw-source-query")!.textContent).toBe("project in (PAY, OPS) ORDER BY Rank ASC\nfixVersion is EMPTY");
  expect(box.querySelector(".tf-vw-sec p")!.textContent).toMatch(/^Jira decides, from the Kanban board Payments and Ops flow\. /);
  expect(box.querySelector<HTMLInputElement>('input[aria-label="Name"]')).toBeNull();
  expect([...box.querySelectorAll<HTMLInputElement>('input[aria-label="Column name"]')].map((i) => [i.value, i.disabled])).toEqual([
    ["Ready", true],
    ["In flight", true],
    ["Shipped", true],
  ]);
  expect([...box.querySelectorAll("button")].some((b) => b.textContent!.trim() === "Delete view")).toBe(false);
  expect(box.textContent).toContain("Only how it looks here. Jira still decides which tickets.");

  const lanes = box.querySelector<HTMLSelectElement>('select[aria-label="Lanes"]')!;
  lanes.value = "assignee";
  lanes.dispatchEvent(new Event("change"));
  flushSync();
  [...box.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save view")!.click();
  await settle();
  expect(h.saved.at(-1)).toMatchObject({ id: "jira-board-21", lanes: "assignee", columns: FLOW_COLUMNS, saved_view: "jira:board-21" });
});

const srcLines = () => [...target.querySelectorAll(".tf-vw-srcline")].map((p) => p.textContent!.replace(/\s+/g, " ").trim());

test("a Jira view's header says where it comes from: board and type, JQL, last read, and Open in Jira (#879)", async () => {
  const opened: string[] = [];
  const { h } = jiraHarness({ last_read: new Date(Date.now() - 4 * 60_000).toISOString() });
  h.ports.openExternal = (url) => {
    opened.push(url);
    return Promise.resolve();
  };
  await open(h, null, "jira-board-21");
  expect(srcLines()).toEqual([
    "Jira decides which tickets, from the Kanban board Payments and Ops flow: project in (PAY, OPS) ORDER BY Rank ASC and fixVersion is EMPTY. Read again at every sync, last 4 min ago. Open in Jira",
  ]);
  [...target.querySelectorAll<HTMLAnchorElement>(".tf-vw-srcline a")].find((a) => a.textContent === "Open in Jira")!.click();
  expect(opened).toEqual(["https://jira.example/secure/RapidBoard.jspa?rapidView=21"]);
});

test("a Scrum view names its active sprint and when it ends", async () => {
  const { h } = jiraHarness({
    board_type: "scrum",
    sub_query: null,
    source_query: "project = PAY ORDER BY Rank ASC",
    sprint: { id: 7, name: "Sprint 34", end_date: "2026-10-22T16:00:00Z" },
  });
  await open(h, null, "jira-board-21");
  expect(srcLines()[0]).toMatch(
    /^Jira decides which tickets, from the Scrum board Payments and Ops flow, active sprint Sprint 34 \(ends 22 Oct\): project = PAY ORDER BY Rank ASC\. /,
  );
});

test("tickets Jira put on a view in a project the source does not sync get a warn line and Sync WEB too", async () => {
  const { h } = jiraHarness({ unsynced: [{ project: "WEB", count: 9 }] });
  await open(h, null, "jira-board-21");
  const warn = target.querySelector<HTMLElement>(".tf-vw-srcline.tf-vw-warn")!;
  expect(warn.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "9 tickets on it are in WEB, which the Jira source does not sync, so they are missing here. Sync WEB too",
  );
  [...warn.querySelectorAll("button")].find((b) => b.textContent === "Sync WEB too")!.click();
  expect(location.hash).toBe("#/settings/source/jira/add/projects/WEB");
});

test("a view somebody made has no source line", async () => {
  const { h } = jiraHarness();
  await open(h, null, "my-work");
  expect(srcLines()).toEqual([]);
});

// ---- A second ticket system's saved views (#2032) ----

/** {@link jiraHarness} with its board coming from a ticket system that is not Jira: Kestrel, whose source is `kestrel`. */
function kestrelHarness(): Harness {
  const { h } = jiraHarness();
  const jira = h.views[1]!;
  h.views[1] = {
    ...jira,
    id: "kestrel-board-7",
    name: "Customs flow",
    group: "Kestrel boards",
    saved_view: "kestrel:board-7",
    saved_view_source: {
      ...jira.saved_view_source!,
      system: "Kestrel",
      board_type: "scrum",
      source_query: "project = CUS",
      sub_query: null,
      sprint: { id: 3, name: "Sprint 9", end_date: null },
      last_read: new Date(Date.now() - 4 * 60_000).toISOString(),
      web_url: "https://kestrel.example/boards/7",
      unsynced: [{ project: "WEB", count: 2 }],
    },
  };
  sourceRoles.take([{ id: "kestrel", display_name: "Tidewater Kestrel", roles: [{ role: "ticket-system", reads: ["translate_query"] }], role_misses: [] }]);
  return h;
}

const noJira = (text: string | null | undefined) => expect(text ?? "").not.toMatch(/jira/i);

test("a second ticket system's views are listed after your own, from its source, and the list ends naming it", async () => {
  const h = kestrelHarness();
  await open(h);
  expect(pane()).toEqual(["# Mine", "My work 2", "# Kestrel boards from the Kestrel source", "Customs flow 18"]);
  const foot = target.querySelector(".tf-vw-foot")!;
  expect(foot.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "Boards and favourite filters come in with the Kestrel source at every sync. Choose which",
  );
  expect(foot.querySelector("a")!.getAttribute("href")).toBe("#/settings/source/kestrel");
  noJira(target.querySelector(".tf-views")!.textContent);
});

test("a second ticket system's view says where it comes from, what decides it, and where a copy came from, never Jira", async () => {
  const h = kestrelHarness();
  await open(h, null, "kestrel-board-7");
  expect(srcLines()).toEqual([
    "Kestrel decides which tickets, from the Scrum board Customs flow, active sprint Sprint 9: project = CUS. Read again at every sync, last 4 min ago. Open in Kestrel",
    "2 tickets on it are in WEB, which the Kestrel source does not sync, so they are missing here. Sync WEB too",
  ]);
  // The tickets are the corpus's, mirrored from `jira`; the words about the view are what may not say so.
  noJira(target.querySelector(".tf-board-head")!.textContent);
  noJira(srcLines().join(" "));

  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  const settings = dialog()!.textContent!.replace(/\s+/g, " ");
  expect(settings).toContain(
    "Kestrel decides, from the Scrum board Customs flow. knobas runs its query at every sync and keeps Kestrel's order. Like the board in Kestrel, it shows the active sprint, now Sprint 9,",
  );
  expect(settings).toContain("To change which tickets it holds, change it in Kestrel, or make your own copy.");
  expect(settings).toContain("The columns come from the board in Kestrel and change when someone changes them there.");
  expect(settings).toContain("Only how it looks here. Kestrel still decides which tickets.");
  noJira(settings);

  [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Make an editable copy")!.click();
  await settle();
  expect(fromLine()).toBe(
    "Copied from the Kestrel board Customs flow. It no longer follows Kestrel. Left out, because knobas cannot test it: fixVersion is EMPTY (knobas does not sync fix versions).",
  );
  expect(dialog()!.textContent).toContain("Kept in knobas. Kestrel never sees it.");
  noJira(dialog()!.textContent);
});

// #2050: the query is what the source's kind declares, and a view whose record carries none has none.
test("a saved view with no query says what decides it with no dangling query, in its source line and View settings", async () => {
  const h = kestrelHarness();
  h.views[1]!.saved_view_source = { ...h.views[1]!.saved_view_source!, source_query: "", unsynced: [] };
  await open(h, null, "kestrel-board-7");
  expect(srcLines()).toEqual([
    "Kestrel decides which tickets, from the Scrum board Customs flow, active sprint Sprint 9. Read again at every sync, last 4 min ago. Open in Kestrel",
  ]);
  expect(target.querySelector(".tf-vw-srcline code")).toBeNull();

  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  const settings = dialog()!.textContent!.replace(/\s+/g, " ");
  expect(settings).toContain(
    "Kestrel decides, from the Scrum board Customs flow. knobas reads it again at every sync and keeps Kestrel's order. Like the board in Kestrel,",
  );
  expect(dialog()!.querySelector(".tf-vw-source-query")).toBeNull();
});

test("a saved view's own query and sub-query show in its source line and View settings", async () => {
  const h = kestrelHarness();
  h.views[1]!.saved_view_source = { ...h.views[1]!.saved_view_source!, source_query: "is:open milestone:train", sub_query: "label:ops", sprint: null, unsynced: [] };
  await open(h, null, "kestrel-board-7");
  expect(srcLines()).toEqual([
    "Kestrel decides which tickets, from the Scrum board Customs flow: is:open milestone:train and label:ops. Read again at every sync, last 4 min ago. Open in Kestrel",
  ]);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-board-head button")].find((b) => b.textContent === "View settings")!.click();
  await settle();
  expect(dialog()!.querySelector(".tf-vw-source-query")!.textContent).toBe("is:open milestone:train\nlabel:ops");
  expect(dialog()!.textContent!.replace(/\s+/g, " ")).toContain("knobas runs its query at every sync and keeps Kestrel's order.");
});

test("with two ticket systems the list's foot and a view of your own name both", async () => {
  const { h } = jiraHarness();
  h.views.push({ ...kestrelHarness().views[1]!, position: 2 });
  await open(h);
  expect(target.querySelector(".tf-vw-foot")!.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "Boards and favourite filters come in with the Jira and Kestrel sources at every sync. Choose which",
  );
  [...target.querySelectorAll<HTMLButtonElement>(".tf-vw-acts button")].find((b) => b.textContent === "+ New view")!.click();
  await settle();
  expect(dialog()!.textContent).toContain("Kept in knobas. Jira and Kestrel never see it.");
});

test("a count of one reads in the singular: the filter bar's ticket and the editor's lane (#1097)", async () => {
  const h = harness([], [], [CORPUS[0]!]);
  await open(h);
  expect(count()).toBe("1 ticket");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-vw-acts button")].find((b) => b.textContent === "+ New view")!.click();
  await previewed(h);
  input(dialog()!.querySelector<HTMLSelectElement>('select[aria-label="Lanes"]')!, "priority");
  await previewed(h);
  expect(dialog()!.querySelector(".tf-vw-pnote")!.textContent!.trim()).toBe("In 1 lane by priority.");
});

/**
 * Two sources besides Jira (#1200): `tracker`, of an adapter kind the shell
 * has never seen, fills the ticket system; `notes` fills only the wiki,
 * though it has ticket-kind items. TRK-2 is a ticket whose source declares
 * none of the lane and column paths.
 */
const TRACKER_CORPUS: { status: string; card: MiniBoardCard }[] = [
  {
    status: "To Do",
    card: card("TRK-1", "Rotate the relay keys", {
      entity_id: "tracker:TRK-1",
      source_id: "tracker",
      path: "TRK-9",
      parent_title: "Relay hardening",
      assignee: "ada.byron",
      priority: "P1",
      issue_type: "Chore",
      labels: ["infra"],
      updated_at: "2026-08-21T10:00:00Z",
    }),
  },
  {
    status: "In Progress",
    card: card("TRK-2", "Undeclared fields", { entity_id: "tracker:TRK-2", source_id: "tracker" }),
  },
  { status: "To Do", card: card("NOTE-1", "A page's to-do", { entity_id: "notes:NOTE-1", source_id: "notes" }) },
];

/**
 * {@link harness} over {@link TRACKER_CORPUS}, its board asked for a role
 * narrowed to `fillers`, as `mini_board` narrows to the sources the mirror
 * says fill it (#1299). `roles` is every role a board read asked for.
 */
function trackerHarness(fillers: string[] = ["tracker"]) {
  const h = harness([], [], TRACKER_CORPUS);
  const whole = h.ports.board;
  const roles: (string | undefined)[] = [];
  h.ports.board = async (perColumn, query, savedView, lanes, role) => {
    roles.push(role);
    const board = await whole(perColumn, query, savedView, lanes);
    if (role === undefined) return board;
    const kept = (column: MiniBoard["columns"][number]) => column.cards.filter((c) => fillers.includes(c.source_id));
    const columns = board.columns
      .map((column) => ({ ...column, cards: kept(column), total: kept(column).length }))
      .filter((column) => column.total > 0);
    const ids = new Set(columns.flatMap((column) => column.cards.map((c) => c.entity_id)));
    const laned = board.lanes
      .map((lane) => ({ ...lane, columns: lane.columns.map((cell) => ({ ...cell, ids: cell.ids.filter((id) => ids.has(id)) })) }))
      .map((lane) => ({ ...lane, columns: lane.columns.map((cell) => ({ ...cell, total: cell.ids.length })) }))
      .map((lane) => ({ ...lane, count: lane.columns.reduce((all, cell) => all + cell.total, 0) }))
      .filter((lane) => lane.count > 0);
    return { ...board, columns, lanes: laned };
  };
  return { h, roles };
}

const keysOnBoard = () => [...target.querySelectorAll(".tf-tcard")].map((el) => el.textContent!.match(/[A-Z]+-\d+/)![0]);

test("a source of any adapter kind that fills the ticket system has its tickets on the board, the list and every lane (#1200, #1299)", async () => {
  const { h, roles } = trackerHarness();
  await open(h);

  expect(roles[0]).toBe("ticket-system");
  expect(keysOnBoard().sort()).toEqual(["TRK-1", "TRK-2"]);
  expect(count()).toBe("2 tickets");

  // Every lane option: TRK-1 under its value, TRK-2, which declares none of them, under none.
  const expected: Record<string, string[]> = {
    epic: ["No epic 1", "TRK-9 1"],
    assignee: ["Unassigned 1", "ada.byron 1"],
    priority: ["No priority 1", "P1 1"],
    issuetype: ["Chore 1", "No type 1"],
    label: ["infra 1", "No label 1"],
  };
  for (const [field] of LANE_FIELDS.filter(([field]) => field !== "none")) {
    chooseLanes(field);
    await settle();
    expect(roles.at(-1)).toBe("ticket-system");
    expect(target.querySelector(".tf-empty")).toBeNull();
    const heads = [...target.querySelectorAll(".tf-lane h2")].map((h2) => h2.textContent!.replace(/\s+/g, " ").trim());
    expect(heads.sort()).toEqual(expected[field]!.slice().sort());
    expect(keysOnBoard().sort()).toEqual(["TRK-1", "TRK-2"]);
  }
  chooseLanes("none");
  await settle();

  [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')].find((b) => b.textContent === "List")!.click();
  await settle();
  const listed = [...target.querySelectorAll<HTMLElement>(".tf-tbl tbody tr")].map((tr) =>
    [...tr.querySelectorAll("td")].map((td) => td.textContent!.trim()),
  );
  expect(listed.map((cells) => cells[0]).sort()).toEqual(["TRK-1", "TRK-2"]);
  const bare = listed.find((cells) => cells[0] === "TRK-2")!;
  // Key, Summary, Status, Priority, Signals, Assignee, Updated: nothing declared, nothing drawn.
  expect(bare).toEqual(["TRK-2", "Undeclared fields", "In Progress", "", "", "Unassigned", "never"]);
});

test("every board read asks for the ticket system, and the shell's source list answering reads nothing again (#1299)", async () => {
  const { h, roles } = trackerHarness();
  await open(h);
  expect(roles.length).toBeGreaterThan(0);
  expect(new Set(roles)).toEqual(new Set(["ticket-system"]));
  expect(keysOnBoard()).not.toContain("NOTE-1");

  // The shell's list of claims is no scope of the board's any more.
  const asked = roles.length;
  sourceRoles.take([
    { id: "tracker", display_name: "tracker", roles: [{ role: "ticket-system", reads: [] }], role_misses: [] },
    { id: "notes", display_name: "notes", roles: [{ role: "ticket-system", reads: [] }], role_misses: [] },
  ]);
  await settle();
  expect(roles).toHaveLength(asked);
  expect(keysOnBoard()).not.toContain("NOTE-1");
});

test("with no source filling the ticket system the board is read, empty, and says how to add one (#1200, #1299)", async () => {
  const { h, roles } = trackerHarness([]);
  await open(h);
  expect(roles[0]).toBe("ticket-system");
  expect(target.querySelector(".tf-empty")!.textContent).toBe(
    "No tickets synced yet. Add a ticket system in Settings and they appear here.",
  );
});

test("a long list draws the rows near the viewport, and j/k walk past them, drawing and focusing each row they reach (#2209)", async () => {
  const many = Array.from({ length: 1000 }, (_, i) => ({
    status: "To Do",
    card: card(`PAY-${String(i + 1).padStart(4, "0")}`, `Ticket ${i + 1}`),
  }));
  await open(harness([], [], many));
  [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')].find((b) => b.textContent === "List")!.click();
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-list-more")!.click();
  await settle();
  const drawn = () => [...target.querySelectorAll(".tf-tbl tbody tr[data-entity]")].map((row) => row.getAttribute("data-entity"));
  const gaps = () => [...target.querySelectorAll<HTMLElement>(".tf-tbl tbody tr.tf-wgap td")].map((td) => td.style.height);
  const first = drawn();
  expect(first.length).toBeGreaterThan(5);
  expect(first.length).toBeLessThan(100);
  expect(first[0]).toBe("jira:PAY-0001");
  // Only room below, every undrawn row at its 52 px estimate.
  expect(gaps()).toEqual([`${(1000 - first.length) * 52}px`]);

  // Past the last drawn row: the next is drawn, selected and focused.
  for (let step = 0; step < first.length + 20; step += 1) press("j");
  const at = `PAY-${String(first.length + 21).padStart(4, "0")}`;
  expect(selectedKey()).toBe(at);
  expect(document.activeElement?.getAttribute("data-entity")).toBe(`jira:${at}`);
  expect(drawn()).not.toContain("jira:PAY-0001");
  expect(gaps()).toHaveLength(2);
  // The preview follows the selection to a row that was not drawn.
  await settle();
  expect(preview().textContent).toContain(at);

  // And back to the top.
  for (let step = 0; step < first.length + 20; step += 1) press("k");
  expect(selectedKey()).toBe("PAY-0001");
  expect(document.activeElement?.getAttribute("data-entity")).toBe("jira:PAY-0001");
  expect(drawn()[0]).toBe("jira:PAY-0001");
});
