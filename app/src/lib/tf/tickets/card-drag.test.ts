/** Drag and drop on the ticket board, pointer and keys (ticket-edit decisions, sections 7 to 10). */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { MiniBoardCard } from "../../ipc/entity";
import { sourceRoles } from "../roles.svelte";
import { createCardDrag, type CardDrag, type CardDragHost } from "./card-drag.svelte";
import { askReach, forgetReach } from "./reach.svelte";

const card = (key: string, status: string): MiniBoardCard => ({
  entity_id: `jira:${key}`,
  source_id: "jira",
  key,
  title: key,
  status,
  priority: null,
  issue_type: "Story",
  assignee: null,
  updated_at: "2026-11-20T10:00:00Z",
  path: null,
  parent_title: null,
  mine: false,
  labels: [],
  comment_count: null,
  signals: [],
});

/** Four columns side by side, 200 px wide, 10 apart; cards 80 px tall, 6 apart, from y 100. */
const COLUMNS = [
  { place: "todo", name: "To do", statuses: ["To Do"], cards: [card("PAY-1", "To Do"), card("PAY-2", "To Do")] },
  { place: "doing", name: "Doing", statuses: ["In Progress", "In Review"], cards: [card("PAY-3", "In Progress"), card("PAY-4", "In Review")] },
  { place: "done", name: "Done", statuses: ["Done"], cards: [card("PAY-5", "Done")] },
  { place: "blocked", name: "Blocked", statuses: ["Blocked"], cards: [] },
];
const rect = (left: number, top: number, width: number, height: number) => ({ left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) }) as DOMRect;

let wrap: HTMLElement;
let floats: HTMLElement;
let drag: CardDrag;
let stop: (() => void) | void;
let dropped: { key: string; place: string; before: string | null; status: string }[];
let chosen: { statuses: string[]; answer: (status: string | null) => void } | null;
let said: string[];

function build() {
  floats = document.createElement("div");
  floats.className = "tf-floats";
  wrap = document.createElement("div");
  wrap.className = "tf-board-wrap";
  const cols = document.createElement("div");
  cols.className = "tf-cols";
  cols.getBoundingClientRect = () => rect(0, 0, 900, 700);
  COLUMNS.forEach((column, at) => {
    const col = document.createElement("div");
    col.className = "tf-col";
    col.dataset.place = column.place;
    col.innerHTML = `<h3>${column.name} <span class="tf-colsts"></span></h3><div class="tf-stack"><div class="tf-cards"></div></div>`;
    const left = at * 210;
    col.querySelector<HTMLElement>(".tf-stack")!.getBoundingClientRect = () => rect(left, 90, 200, 400);
    column.cards.forEach((one, index) => {
      const node = document.createElement("button");
      node.className = "tf-tcard";
      node.dataset.entity = one.entity_id;
      node.innerHTML = `<span class="tf-thead">${one.key}</span>`;
      node.getBoundingClientRect = () => rect(left + 5, 100 + index * 86, 190, 80);
      col.querySelector(".tf-cards")!.append(node);
    });
    cols.append(col);
  });
  wrap.append(cols);
  document.body.append(floats, wrap);
}

function host(over: Partial<CardDragHost> = {}): CardDragHost {
  return {
    cardOf: (id) => {
      for (const column of COLUMNS) {
        const found = column.cards.find((one) => one.entity_id === id);
        if (found) return { card: found, place: column.place, lane: "" };
      }
      return null;
    },
    columns: () =>
      COLUMNS.map((column) => ({
        place: column.place,
        lane: "",
        column: { name: column.name, statuses: column.statuses },
        node: wrap.querySelector<HTMLElement>(`[data-place="${column.place}"]`)!,
      })),
    facts: () => ({ source: "Tidewater Jira", moves: true, refused: null }),
    drop: (one, to) => dropped.push({ key: one.key, ...to }),
    choose: (_card, statuses) => new Promise((answer) => (chosen = { statuses, answer })),
    ask: () => Promise.resolve(null),
    scroller: () => wrap.querySelector<HTMLElement>(".tf-cols")!,
    say: (text) => said.push(text),
    ...over,
  };
}

const pointer = (type: string, x: number, y: number, on: EventTarget = window, pointerType = "mouse") =>
  on.dispatchEvent(new PointerEvent(type, { button: 0, clientX: x, clientY: y, bubbles: true, cancelable: true, pointerId: 1, pointerType, isPrimary: true }));
const key = (name: string) => window.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
const node = (key: string) => wrap.querySelector<HTMLElement>(`[data-entity="jira:${key}"]`)!;
const drops = () => Object.fromEntries(COLUMNS.map((column) => [column.place, wrap.querySelector<HTMLElement>(`[data-place="${column.place}"]`)!.dataset.drop ?? null]));
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Press PAY-1 at its middle and travel `by` px right. */
function lift(by = 10) {
  const box = node("PAY-1").getBoundingClientRect();
  pointer("pointerdown", box.left + 40, box.top + 20, node("PAY-1"));
  pointer("pointermove", box.left + 40 + by, box.top + 20);
}

beforeEach(async () => {
  forgetReach();
  dropped = [];
  chosen = null;
  said = [];
  sourceRoles.take([{ id: "jira", display_name: "Tidewater Jira", roles: [], role_misses: [], write_ops: ["transition"] }]);
  // Where PAY-1 can go from To Do, as its source said: In Progress and In Review, and Blocked.
  await askReach(card("PAY-1", "To Do"), "press", () => Promise.resolve(["In Progress", "In Review", "Blocked"]));
  build();
  drag = createCardDrag(host());
  stop = drag.attach(wrap);
});

afterEach(() => {
  stop?.();
  wrap.remove();
  floats.remove();
  sourceRoles.forget();
});

test("under four pixels nothing is drawn and the click still reaches the card", () => {
  const box = node("PAY-1").getBoundingClientRect();
  pointer("pointerdown", box.left + 40, box.top + 20, node("PAY-1"));
  pointer("pointermove", box.left + 43, box.top + 20);
  expect(floats.children).toHaveLength(0);
  expect(node("PAY-1").classList.contains("tf-held")).toBe(false);
  pointer("pointerup", box.left + 43, box.top + 20);
  const clicked = vi.fn();
  node("PAY-1").addEventListener("click", clicked);
  node("PAY-1").click();
  expect(clicked).toHaveBeenCalledOnce();
});

test("past them the copy is in the float host, the card's box is emptied, and each column says whether it takes it", () => {
  lift();
  expect(floats.querySelector(".tf-tghost")?.textContent).toBe("PAY-1");
  expect(floats.querySelector(".tf-tghost")?.hasAttribute("data-entity")).toBe(false);
  expect(node("PAY-1").classList.contains("tf-held")).toBe(true);
  expect(wrap.hasAttribute("data-held")).toBe(true);
  expect(drops()).toEqual({ todo: null, doing: "yes", done: "no", blocked: "yes" });
  // A column of several reachable statuses says them in its head.
  expect(wrap.querySelector('[data-place="doing"] .tf-colsts')!.textContent).toBe("In Progress or In Review");
  expect(drag.holding()).toBe(true);
});

test("Esc puts it back: nothing drawn stays, and nothing is dropped", () => {
  lift();
  key("Escape");
  expect(floats.children).toHaveLength(0);
  expect(node("PAY-1").classList.contains("tf-held")).toBe(false);
  expect(wrap.hasAttribute("data-held")).toBe(false);
  expect(drops()).toEqual({ todo: null, doing: null, done: null, blocked: null });
  expect(wrap.querySelector('[data-place="doing"] .tf-colsts')!.textContent).toBe("");
  expect(dropped).toEqual([]);
  expect(said.at(-1)).toBe("PAY-1 put back.");
});

test("over a column that will not take it the reason stands under the copy, and a release there drops nothing", () => {
  lift();
  pointer("pointermove", 420 + 100, 200);
  const why = floats.querySelector<HTMLElement>(".tf-twhy")!;
  expect(why.hidden).toBe(false);
  expect(why.textContent).toBe("Tidewater Jira offers no move from To Do to Done.");
  expect(wrap.querySelector<HTMLElement>('[data-place="done"]')!.hasAttribute("data-over")).toBe(true);
  pointer("pointerup", 420 + 100, 200);
  expect(dropped).toEqual([]);
  expect(floats.children).toHaveLength(0);
});

test("a release over a one-status column drops it between the two cards the pointer was between, and the click it makes is swallowed", () => {
  lift();
  pointer("pointermove", 630 + 100, 120);
  expect(floats.querySelector<HTMLElement>(".tf-tline")!.hidden).toBe(false);
  pointer("pointerup", 630 + 100, 120);
  expect(dropped).toEqual([{ key: "PAY-1", place: "blocked", before: null, status: "Blocked" }]);
  expect(floats.children).toHaveLength(0);
  const clicked = vi.fn();
  node("PAY-1").addEventListener("click", clicked);
  node("PAY-1").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  expect(clicked).not.toHaveBeenCalled();
});

test("a column of several statuses asks which; a pick drops with it, before the card under the line", async () => {
  lift();
  // Between PAY-3 (100 to 180) and PAY-4 (186 to 266): below PAY-3's middle, above PAY-4's.
  pointer("pointermove", 210 + 100, 200);
  pointer("pointerup", 210 + 100, 200);
  expect(dropped).toEqual([]);
  expect(chosen?.statuses).toEqual(["In Progress", "In Review"]);
  // The copy waits while the menu stands; nothing was written.
  expect(floats.querySelector(".tf-tghost")).not.toBeNull();
  chosen!.answer("In Review");
  await flush();
  expect(dropped).toEqual([{ key: "PAY-1", place: "doing", before: "jira:PAY-4", status: "In Review" }]);
  expect(floats.children).toHaveLength(0);
});

test("closing the menu without a pick puts the card back and drops nothing", async () => {
  lift();
  pointer("pointermove", 310, 120);
  pointer("pointerup", 310, 120);
  chosen!.answer(null);
  await flush();
  expect(dropped).toEqual([]);
  expect(floats.children).toHaveLength(0);
  expect(node("PAY-1").classList.contains("tf-held")).toBe(false);
});

test("a finger starts no drag: it scrolls the board", () => {
  const box = node("PAY-1").getBoundingClientRect();
  pointer("pointerdown", box.left + 40, box.top + 20, node("PAY-1"), "touch");
  pointer("pointermove", box.left + 80, box.top + 60, window, "touch");
  expect(floats.children).toHaveLength(0);
});

test("the page going while a card is held leaves the float host empty", () => {
  lift();
  stop?.();
  stop = undefined;
  expect(floats.children).toHaveLength(0);
});

test("m carries the card by key: the first step skips a column that will not take it, Enter drops at the column's top", () => {
  expect(drag.carry("jira:PAY-1")).toBe(true);
  expect(said.at(-1)).toBe("PAY-1 picked up. PAY-1 over Doing (In Progress or In Review). Enter moves it, Esc puts it back. It cannot go to Done.");
  key("ArrowRight");
  // Done takes nothing, so → goes on to Blocked.
  expect(said.at(-1)).toBe("PAY-1 over Blocked. Enter moves it, Esc puts it back.");
  key("ArrowRight");
  expect(said.at(-1)).toBe("PAY-1 over Blocked. Enter moves it, Esc puts it back.");
  key("ArrowLeft");
  key("ArrowLeft");
  expect(said.at(-1)).toBe("PAY-1 is over To do, where it stands. Enter puts it back.");
  key("ArrowRight");
  key("ArrowRight");
  key("Enter");
  expect(dropped).toEqual([{ key: "PAY-1", place: "blocked", before: null, status: "Blocked" }]);
});

test("while a card is carried every other key is swallowed, and Esc drops nothing", () => {
  const heard = vi.fn();
  window.addEventListener("keydown", heard);
  drag.carry("jira:PAY-1");
  key("j");
  expect(heard).not.toHaveBeenCalled();
  key("Escape");
  expect(dropped).toEqual([]);
  expect(floats.children).toHaveLength(0);
  window.removeEventListener("keydown", heard);
});

test("Enter on a carried card over a column with cards drops it above the first", async () => {
  // PAY-5 in Done can go to To Do: the carry lands at the top of a column that has cards.
  await askReach(card("PAY-5", "Done"), "press", () => Promise.resolve(["To Do"]));
  drag.carry("jira:PAY-5");
  key("Enter");
  expect(dropped).toEqual([{ key: "PAY-5", place: "todo", before: "jira:PAY-1", status: "To Do" }]);
});

test("a card whose source answers later lights from what the window holds and follows the answer, boxes unchanged", async () => {
  let land: (statuses: string[]) => void = () => {};
  stop?.();
  drag = createCardDrag(host({ ask: (one, when) => (when === "pickup" ? askReach({ ...one, updated_at: "later" }, "pickup", () => new Promise((resolve) => (land = resolve))) : Promise.resolve(null)) }));
  stop = drag.attach(wrap);
  const late = card("PAY-2", "To Do");
  COLUMNS[0]!.cards[1] = { ...late, updated_at: "later" };
  try {
    const box = node("PAY-2").getBoundingClientRect();
    pointer("pointerdown", box.left + 40, box.top + 20, node("PAY-2"));
    pointer("pointermove", box.left + 50, box.top + 20);
    // A like answer (PAY-1's, same workflow) lights it at once.
    expect(drops()).toEqual({ todo: null, doing: "yes", done: "no", blocked: "yes" });
    await flush();
    land(["Done"]);
    await flush();
    await flush();
    expect(drops()).toEqual({ todo: null, doing: "no", done: "yes", blocked: "no" });
    key("Escape");
  } finally {
    COLUMNS[0]!.cards[1] = late;
  }
});

test("a capture lost with the button still down keeps the card held; lost with none down puts it back", () => {
  lift();
  const lost = (buttons: number) => wrap.dispatchEvent(new PointerEvent("lostpointercapture", { pointerId: 1, pointerType: "mouse", buttons, bubbles: true }));
  lost(1);
  expect(drag.holding()).toBe(true);
  lost(0);
  expect(drag.holding()).toBe(false);
  expect(floats.children).toHaveLength(0);
});
