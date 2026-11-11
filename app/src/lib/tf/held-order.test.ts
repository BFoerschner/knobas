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
