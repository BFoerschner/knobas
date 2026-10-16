/**
 * Tickets in the ticket-first shell (#810): the ticket board and the list
 * over `mini_board`, the filter bar sending its query to the server, the
 * yellow selected card, the preview and *Not a ticket*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ContextRow, MiniBoard, MiniBoardCard } from "../../ipc/entity";
import type { TicketView, TicketViewDraft, TicketViewRow } from "../../ipc/search";
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import TicketsPage, { forgetTicketsSession, type TicketsPorts } from "./TicketsPage.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  location.hash = "#/tickets";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
  forgetFilters();
  forgetTicketsSession();
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
  reads: { perColumn: number | null; query: string | null }[];
  /** The ticket views the views read answers with (#812); the writes change it. */
  views: TicketViewRow[];
  saved: TicketViewDraft[];
  deleted: string[];
}

/** Board reads answered over {@link CORPUS}, the query read as words that must all appear. */
function harness(contexts: ContextRow[] = [], views: TicketViewRow[] = []): Harness {
  const reads: Harness["reads"] = [];
  const h: Harness = {
    reads,
    views,
    saved: [],
    deleted: [],
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
        h.views = [...h.views.filter((row) => row.id !== id), { ...made, count: 1, unreadable: null }].sort(
          (a, b) => a.position - b.position,
        );
        return Promise.resolve(made);
      },
      deleteView: (id) => {
        h.deleted.push(id);
        h.views = h.views.filter((row) => row.id !== id);
        return Promise.resolve();
      },
      board: (perColumn, query) => {
        reads.push({ perColumn, query });
        // Grammar keys narrow nothing here; the reads record them.
        const words = (query ?? "")
          .toLowerCase()
          .split(/\s+/)
          .filter((word) => word !== "" && !word.includes(":"));
        const kept = CORPUS.filter(({ card }) =>
          words.every((word) => `${card.key} ${card.title}`.toLowerCase().includes(word)),
        );
        const board: MiniBoard = { columns: [], sources: [] };
        for (const status of ["To Do", "In Progress", "Done"]) {
          const cards = kept.filter((entry) => entry.status === status).map((entry) => entry.card);
          if (cards.length === 0) continue;
          board.columns.push({
            status,
            cards: perColumn === null ? cards : cards.slice(0, perColumn),
            total: cards.length,
          });
        }
        return Promise.resolve(board);
      },
      detail: (id) => Promise.resolve({ body_text: `The description of ${id}.` }),
      contexts: () => Promise.resolve(contexts),
      tally: () =>
        Promise.resolve([
          { kind: "build", tone: "err" as const, count: 1 },
          { kind: "build", tone: "ok" as const, count: 6 },
          { kind: "page", tone: null, count: 3 },
        ]),
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

async function open(h: Harness, timedTicket: string | null = null, viewId: string | null = null) {
  page = mount(TicketsPage, { target, props: { ready: true, timedTicket, ports: h.ports, viewId } });
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
  expect(button.getAttribute("href")).toBe("#/job/PAY-1");
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

test("typing filters on the server, keeps focus and the caret, shows every match and marks the words", async () => {
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
  expect(h.reads.at(-1)).toEqual({ perColumn: null, query: "sepa" });
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
  expect(h.reads.at(-1)).toEqual({ perColumn: 12, query: null });
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
  expect(h.reads.at(-1)).toEqual({ perColumn: 12, query: null });
  expect(target.querySelector(".tf-list-more")).toBeNull();
  layout("Board");
  await settle();
  expect(columns()[0]).toEqual({ head: "To Do 12", cards: 8, more: "Show 4 more" });
  expect(columns()[2]!.cards).toBe(3);
});

test("Not a ticket lists the ad-hoc contexts, and a ticket's context fills Around this ticket", async () => {
  await open(
    harness([
      { id: "ctx:1", kind: "adhoc", title: "Staging DB configuration", anchor_id: null, created_at: "", archived_at: null },
      { id: "ctx:2", kind: "ticket", title: "PAY-1", anchor_id: "jira:PAY-1", created_at: "", archived_at: null },
    ]),
  );
  const views = target.querySelector(".tf-views")!;
  expect(views.querySelector("h2")!.textContent).toBe("Not a ticket");
  expect([...views.querySelectorAll("a")].map((a) => a.textContent!.trim())).toEqual([
    "All tickets 18",
    "Staging DB configuration",
  ]);
  const lines = [...preview().querySelectorAll(".tf-rollup > div")].map((d) => d.textContent);
  expect(lines).toEqual(["Builds7 builds, 1 failing or down", "Docs3 pages"]);
});

test("Enter presses the preview's button from a card", async () => {
  await open(harness());
  const first = target.querySelector<HTMLButtonElement>(".tf-tcard")!;
  first.focus();
  first.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  expect(location.hash).toBe("#/job/PAY-1");
});

test("Enter on a focused card that is not selected opens that card's ticket", async () => {
  await open(harness());
  const second = target.querySelectorAll<HTMLButtonElement>(".tf-tcard")[1]!;
  const key = second.querySelector(".tf-k")!.textContent;
  expect(second.classList.contains("tf-card")).toBe(false);
  second.focus();
  second.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  expect(location.hash).toBe(`#/job/${key}`);
});

test("the preview reads its ticket once, not again on every board answer that keeps it selected", async () => {
  const h = harness();
  const asked: string[] = [];
  const detail = h.ports.detail;
  h.ports.detail = (id) => {
    asked.push(id);
    return detail(id);
  };
  await open(h);
  type(field(), "r");
  await settle();
  type(field(), "e");
  await settle();
  expect(asked).toEqual(["jira:PAY-1"]);
  expect(preview().textContent).toContain("The description of jira:PAY-1.");
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

test("lanes split the board into bands by size, each with its columns and count, read whole", async () => {
  const h = harness();
  await open(h);
  chooseLanes("assignee");
  await settle();
  expect(h.reads.at(-1)).toEqual({ perColumn: null, query: null });
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
  // One lane's Show N more expands that lane's column only, with no read.
  const reads = h.reads.length;
  target.querySelector<HTMLButtonElement>(".tf-lane .tf-more")!.click();
  await settle();
  expect(h.reads.length).toBe(reads);
  expect(target.querySelectorAll(".tf-lane")[0]!.querySelectorAll(".tf-tcard").length).toBe(11);
  expect(target.querySelectorAll(".tf-lane")[1]!.querySelectorAll(".tf-tcard").length).toBe(3);
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

test("j/k walk the list's rows in their sorted order, and a double click opens the job", async () => {
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
  expect(location.hash).toBe("#/job/PAY-5");
  location.hash = "#/tickets";
  layoutButton("Board").click();
  await settle();
  target.querySelectorAll<HTMLElement>(".tf-tcard")[1]!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  expect(location.hash).toBe("#/job/PAY-2");
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
    jira_ref: null,
    count: 3,
    unreadable: null,
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
  expect(dialog()!.textContent).toContain("Kept in knobas. Jira never sees it.");
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
      jira_ref: null,
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

/** The preview reads a moment after a change. */
async function previewed() {
  await new Promise((resolve) => setTimeout(resolve, 200));
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
  await previewed();
  expect(dialog()!.querySelector("h2")!.textContent).toBe("New view");
  expect(dialog()!.textContent).toContain("Kept in knobas. Jira never sees it.");
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
  await previewed();
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
  await previewed();
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
  await previewed();
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
  await previewed();
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
  await previewed();
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
  await previewed();
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
  await previewed();
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
  await previewed();
  inDialog("Delete view").click();
  await settle();
  expect(dialog()).toBeNull();
  expect(h.deleted).toEqual(["backlog"]);
  expect(toasts.items.at(-1)!.action!.label).toBe("Undo");
});
