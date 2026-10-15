/**
 * Tickets in the ticket-first shell (#810): the ticket board and the list
 * over `mini_board`, the filter bar sending its query to the server, the
 * yellow selected card, the preview and *Not a ticket*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { ContextRow, MiniBoard, MiniBoardCard } from "../../ipc/entity";
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
    mine: false,
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
      updated_at: `2026-08-${String(22 - i).padStart(2, "0")}T10:00:00Z`,
    }),
  })),
  { status: "In Progress", card: card("PAY-40", "Payout dashboard latency", { mine: true, assignee: "mara.lindqvist" }) },
  ...Array.from({ length: 5 }, (_, i) => ({ status: "Done", card: card(`PAY-${50 + i}`, `Done ${i}`) })),
];

interface Harness {
  ports: TicketsPorts;
  reads: { perColumn: number | null; query: string | null }[];
}

/** Board reads answered over {@link CORPUS}, the query read as words that must all appear. */
function harness(contexts: ContextRow[] = []): Harness {
  const reads: Harness["reads"] = [];
  return {
    reads,
    ports: {
      board: (perColumn, query) => {
        reads.push({ perColumn, query });
        const words = (query ?? "").toLowerCase().split(/\s+/).filter(Boolean);
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
}

async function open(h: Harness, timedTicket: string | null = null) {
  page = mount(TicketsPage, { target, props: { ready: true, timedTicket, ports: h.ports } });
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
  expect(first.querySelector(".tf-cep")!.textContent).toBe("PAY-200");

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
  expect(head).toEqual(["Key", "Summary", "Status", "Priority", "Assignee", "Updated"]);
  const rows = [...target.querySelectorAll<HTMLElement>(".tf-tbl tbody tr")];
  expect(rows[0]!.querySelector("td")!.textContent).toBe("PAY-1");
  expect(rows[0]!.classList.contains("tf-card")).toBe(true);
  expect(rows[0]!.textContent).toContain("PAY-200");
  expect(target.querySelector(".tf-list-more")!.textContent).toBe("Show 4 more");
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
