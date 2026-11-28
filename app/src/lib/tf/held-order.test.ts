import { expect, test } from "vitest";
import { HeldOrder } from "./held-order.svelte";

type Row = { id: string; group?: string; v?: number };
const row = (id: string, group = "", v = 0): Row => ({ id, group, v });
const ids = (rows: readonly Row[]) => rows.map((r) => r.id);

test("the first answer is drawn as given, and later reads keep each drawn row's place", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  expect(ids(hold.place([row("a"), row("b"), row("c")], "x").rows)).toEqual(["a", "b", "c"]);
  // A later read sorts c first (a push, a fact): the rows stay, and *Sort again* is offered.
  const later = hold.place([row("c"), row("a"), row("b")], "x");
  expect(ids(later.rows)).toEqual(["a", "b", "c"]);
  expect(later).toMatchObject({ waiting: 0, stale: true });
});

test("a late read fills a drawn row in where it stands", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b")], "x");
  const later = hold.place([row("a", "", 1), row("b", "", 2)], "x");
  expect(later.rows.map((r) => r.v)).toEqual([1, 2]);
  expect(later.stale).toBe(false);
});

test("a row that sorts after every drawn row joins the end: a list's next page", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b")], "x");
  const later = hold.place([row("a"), row("b"), row("c"), row("d")], "x");
  expect(ids(later.rows)).toEqual(["a", "b", "c", "d"]);
  expect(later.waiting).toBe(0);
});

test("a row that would sort above a drawn one waits, counted, until the person asks", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b")], "x");
  const later = hold.place([row("new"), row("a"), row("b")], "x");
  expect(ids(later.rows)).toEqual(["a", "b"]);
  expect(later.waiting).toBe(1);
  hold.release();
  expect(ids(hold.place([row("new"), row("a"), row("b")], "x").rows)).toEqual(["new", "a", "b"]);
});

test("a changed basis is the person's act: the list sorts again", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b")], "x");
  expect(ids(hold.place([row("b"), row("a")], "x").rows)).toEqual(["a", "b"]);
  expect(ids(hold.place([row("b"), row("a")], "y").rows)).toEqual(["b", "a"]);
});

test("a row that left stays drawn and offers Sort again; with keepLeft off it goes and keeps its place for a return", () => {
  const kept = new HeldOrder<Row>((r) => r.id);
  kept.place([row("a"), row("b"), row("c")], "x");
  const left = kept.place([row("a"), row("c")], "x");
  expect(ids(left.rows)).toEqual(["a", "b", "c"]);
  expect(left.stale).toBe(true);

  const pressed = new HeldOrder<Row>((r) => r.id, () => "", { keepLeft: false });
  pressed.place([row("a"), row("b"), row("c")], "x");
  expect(ids(pressed.place([row("a"), row("c")], "x").rows)).toEqual(["a", "c"]);
  // *Undo*: the row is back where it stood, not behind *Show N new*.
  const back = pressed.place([row("a"), row("b"), row("c")], "x");
  expect(ids(back.rows)).toEqual(["a", "b", "c"]);
  expect(back.waiting).toBe(0);
});

test("a row the person brings in takes its place where it sorts", () => {
  const hold = new HeldOrder<Row>((r) => r.id, () => "", { keepLeft: false });
  hold.place([row("a"), row("c")], "x");
  hold.admit("b");
  expect(ids(hold.place([row("a"), row("b"), row("c")], "x").rows)).toEqual(["a", "b", "c"]);
});

test("a drawn row keeps the group it drew in until the person acts", () => {
  const hold = new HeldOrder<Row>((r) => r.id, (r) => r.group!);
  hold.place([row("a", "open"), row("b", "settled")], "x");
  const later = hold.place([row("b", "open"), row("a", "settled")], "x");
  expect(later.rows.map((r) => [r.id, hold.tagOf(r)])).toEqual([
    ["a", "open"],
    ["b", "settled"],
  ]);
});

test("nothing is held before a list has a row", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  expect(hold.place([], "x").rows).toEqual([]);
  expect(ids(hold.place([row("b"), row("a")], "x").rows)).toEqual(["b", "a"]);
});

test("groups side by side (byTag): a row new at its own group's foot joins it, wherever the other groups' rows sort (ux18)", () => {
  type Card = { id: string; column: string };
  const at = (column: string, ...ids: string[]) => ids.map((id) => ({ id, column }));
  const board = new HeldOrder<Card>(({ id }) => id, ({ column }) => column, { byTag: true });
  board.place([...at("todo", "a1", "a2"), ...at("done", "d1")], "open");
  // A page of To Do lands: its cards sort after a2 and before d1 in reading order, at the foot of their own column.
  const paged = board.place([...at("todo", "a1", "a2", "a3", "a4"), ...at("done", "d1")], "open");
  expect(paged.waiting).toBe(0);
  expect(paged.rows.filter((row) => row.column === "todo").map(({ id }) => id)).toEqual(["a1", "a2", "a3", "a4"]);
  // One above a drawn card of its column still waits.
  const above = board.place([...at("todo", "a0", "a1", "a2", "a3", "a4"), ...at("done", "d1")], "open");
  expect(above.waiting).toBe(1);

  // Without it, the stacked groups' rule: the new card sorts above d1, so it waits.
  const stacked = new HeldOrder<Card>(({ id }) => id, ({ column }) => column);
  stacked.place([...at("todo", "a1", "a2"), ...at("done", "d1")], "open");
  expect(stacked.place([...at("todo", "a1", "a2", "a3"), ...at("done", "d1")], "open").waiting).toBe(1);
});

test("byTag: a row of a group with no drawn row waits, counted, as a new board column's card does (ux18)", () => {
  type Card = { id: string; column: string };
  const at = (column: string, ...ids: string[]) => ids.map((id) => ({ id, column }));
  const board = new HeldOrder<Card>(({ id }) => id, ({ column }) => column, { byTag: true });
  board.place([...at("todo", "a1", "a2"), ...at("done", "d1")], "open");
  // A sync brings a status of its own, between the drawn columns: its card is counted behind *Show N new*.
  const synced = board.place([...at("todo", "a1", "a2"), ...at("new", "n1"), ...at("done", "d1")], "open");
  expect(synced.waiting).toBe(1);
  expect(synced.rows.map(({ id }) => id)).toEqual(["a1", "a2", "d1"]);
  // At the end of the board it joins, as the stacked rule lets it.
  const last = board.place([...at("todo", "a1", "a2"), ...at("done", "d1"), ...at("later", "l1")], "open");
  expect(last.waiting).toBe(0);
});

test("move: the person's move stands at once between two rows, at a group's start or its end, and a later read keeps it (ADR-0054)", () => {
  const hold = new HeldOrder<Row>((r) => r.id, (r) => r.group ?? "", { byTag: true });
  const read = [row("a", "todo"), row("b", "todo"), row("c", "doing"), row("d", "doing")];
  hold.place(read, "x");
  const group = (rows: readonly Row[], tag: string) => ids(rows.filter((r) => hold.tagOf(r) === tag));
  // Between c and d.
  expect(hold.move("a", "doing", "d")).toEqual({ tag: "todo", rank: 0 });
  let placed = hold.place(read, "x");
  expect(group(placed.rows, "doing")).toEqual(["c", "a", "d"]);
  expect(group(placed.rows, "todo")).toEqual(["b"]);
  // At the start, and at the end.
  hold.move("b", "doing", "c");
  placed = hold.place(read, "x");
  expect(group(placed.rows, "doing")).toEqual(["b", "c", "a", "d"]);
  hold.move("c", "todo", null);
  expect(group(hold.place(read, "x").rows, "todo")).toEqual(["c"]);
  // The read's old order does not take it back: the drawn order differs, so *Sort again* is offered.
  expect(hold.place(read, "x").stale).toBe(true);
  // A key that is not held moves nothing.
  expect(hold.move("zz", "doing", null)).toBeNull();
});

test("restore puts a moved row back where it stood, with nothing else changed; a new basis sorts as read", () => {
  const hold = new HeldOrder<Row>((r) => r.id, (r) => r.group ?? "", { byTag: true });
  const read = [row("a", "todo"), row("b", "todo"), row("c", "doing")];
  hold.place(read, "x");
  const was = hold.move("a", "doing", "c")!;
  hold.restore("a", was);
  const placed = hold.place(read, "x");
  expect(ids(placed.rows)).toEqual(["a", "b", "c"]);
  expect(placed.rows.map((r) => hold.tagOf(r))).toEqual(["todo", "todo", "doing"]);
  hold.move("a", "doing", null);
  const fresh = hold.place(read, "y");
  expect(fresh.rows.map((r) => hold.tagOf(r))).toEqual(["todo", "todo", "doing"]);
});

test("settle: the waiting rows take their place where they sort and the rows that left go, while the drawn rows keep their order (#2574)", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b"), row("c"), row("gone")], "x");
  // b now sorts first (a queued run that started) and gone left: still drawn as they were, new waiting above.
  const read = [row("new"), row("b"), row("a"), row("c")];
  const before = hold.place(read, "x");
  expect(ids(before.rows)).toEqual(["a", "b", "c", "gone"]);
  expect(before).toMatchObject({ waiting: 1, left: 1 });
  hold.settle();
  const after = hold.place(read, "x");
  expect(ids(after.rows)).toEqual(["new", "a", "b", "c"]);
  expect(after).toMatchObject({ waiting: 0, left: 0 });
  // Settled once: a later read keeps that order as any drawn order is kept.
  expect(ids(hold.place([row("b"), row("new"), row("a"), row("c")], "x").rows)).toEqual(["new", "a", "b", "c"]);
});

test("settle is for the list as it is: a new basis or a release before the next read drops it, and it lets in no later row (#2574)", () => {
  const hold = new HeldOrder<Row>((r) => r.id);
  hold.place([row("a"), row("b")], "x");
  hold.settle();
  // The person changed the filter before the next read: that sorts, and the settle is spent with it.
  hold.place([row("a"), row("b")], "y");
  expect(hold.place([row("new"), row("a"), row("b")], "y")).toMatchObject({ waiting: 1 });
  hold.settle();
  hold.release();
  hold.place([row("a"), row("b")], "y");
  expect(hold.place([row("new2"), row("a"), row("b")], "y")).toMatchObject({ waiting: 1 });
  // A row a settle let in at the end, which leaves and comes back while the pointer rests, waits like any other.
  hold.settle();
  hold.place([row("a"), row("b"), row("c")], "y");
  hold.settle();
  hold.place([row("a"), row("b")], "y");
  expect(hold.place([row("c"), row("a"), row("b")], "y")).toMatchObject({ waiting: 1 });
});
