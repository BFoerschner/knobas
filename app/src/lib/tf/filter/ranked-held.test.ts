/**
 * A list held once and ranked by its id (ux31, the API explorer's filter, section 6): its texts cross once, a key
 * sends only its text, and a list the app dropped is held again once.
 */
import { expect, test, vi } from "vitest";

/** What crossed to the app, by command, and the bytes of each. */
const sent: { cmd: string; bytes: number }[] = [];
const held = new Map<string, string[]>();
vi.mock("../../ipc/search", () => {
  const log = (cmd: string, args: unknown) => sent.push({ cmd, bytes: JSON.stringify(args).length });
  return {
    fuzzyRank: async () => {
      throw new Error("not this one");
    },
    fuzzyHold: async (id: string, haystacks: readonly string[]) => {
      log("fuzzy_hold", { id, haystacks });
      held.set(id, [...haystacks]);
    },
    fuzzyRankHeld: async (id: string, text: string) => {
      log("fuzzy_rank_held", { id, text });
      const texts = held.get(id);
      if (texts === undefined) throw { code: "not_found", message: "gone" };
      return texts.flatMap((t, index) => (t.includes(text) ? [{ index, loose: false, marks: [t.indexOf(text), t.indexOf(text) + text.length] }] : []));
    },
    fuzzyDrop: async (id: string) => {
      log("fuzzy_drop", { id });
      held.delete(id);
    },
  };
});

const { FuzzyOrder } = await import("./ranked.svelte");
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const rows = Array.from({ length: 2000 }, (_, i) => `get /resource${i}/{id} summary of resource number ${i}`);
const same = (row: string) => row;

test("a held list sends its texts once; each key after sends only its text, and answers by the list's rows", async () => {
  const order = FuzzyOrder.held<string>("spec-1");
  order.ask("resource1", rows, same);
  await flush();
  expect(sent.map((s) => s.cmd)).toEqual(["fuzzy_hold", "fuzzy_rank_held"]);
  expect(sent[0]!.bytes).toBeGreaterThan(80_000);
  sent.length = 0;
  for (const text of ["resource12", "resource123", "resource1234"]) {
    order.ask(text, rows, same);
    await flush();
  }
  expect(sent.map((s) => s.cmd)).toEqual(["fuzzy_rank_held", "fuzzy_rank_held", "fuzzy_rank_held"]);
  expect(Math.max(...sent.map((s) => s.bytes))).toBeLessThan(100);
  expect(order.rows("resource1234", rows, same)).toEqual([rows[1234]]);
  expect(order.marks(rows[1234]!)).toEqual([5, 17]);
  sent.length = 0;
});

test("a list the app dropped is held again once, and the key is answered", async () => {
  const order = FuzzyOrder.held<string>("spec-2");
  order.ask("resource7", rows, same);
  await flush();
  held.delete("spec-2");
  sent.length = 0;
  order.ask("resource77", rows, same);
  await flush();
  expect(sent.map((s) => s.cmd)).toEqual(["fuzzy_rank_held", "fuzzy_hold", "fuzzy_rank_held"]);
  expect(order.rows("resource77", rows, same)).toEqual(rows.filter((r) => r.includes("resource77")));
  sent.length = 0;
  order.drop();
  expect(sent.map((s) => s.cmd)).toEqual(["fuzzy_drop"]);
  sent.length = 0;
});

test("a key out when the page drops its list holds nothing again (#2532)", async () => {
  const order = FuzzyOrder.held<string>("spec-gone");
  sent.length = 0;
  order.ask("resource9", rows, same);
  // The page closes while the key's ranking is out: the app answers it `not_found`.
  order.drop();
  await flush();
  await flush();
  expect(sent.map((s) => s.cmd)).toEqual(["fuzzy_hold", "fuzzy_drop", "fuzzy_rank_held"]);
  expect(held.has("spec-gone")).toBe(false);
  sent.length = 0;
});

test("new rows are held anew; an older text's answer is never drawn", async () => {
  const order = FuzzyOrder.held<string>("spec-3");
  order.ask("resource5", rows, same);
  const other = rows.slice(0, 10);
  order.ask("resource5", other, same);
  await flush();
  expect(sent.filter((s) => s.cmd === "fuzzy_hold").length).toBe(2);
  expect(order.rows("resource5", other, same)).toEqual([other[5]]);
  sent.length = 0;
});

test("a held order exposes sliced UTF-16 marks and clears stale marks when the query empties", async () => {
  const emojiRows = ["🚀 get /resource5/items", "get /health"];
  const order = FuzzyOrder.held<string>("spec-marks");
  order.ask("resource5", emojiRows, same);
  await flush();
  // "🚀 get /" is 2 + 6 = 8 UTF-16 code units; "resource5" is [8, 17].
  expect(order.marks(emojiRows[0]!)).toEqual([8, 17]);
  expect(order.marks(emojiRows[0]!, 7, 16)).toEqual([1, 10]);

  order.ask("", emojiRows, same);
  expect(order.marks(emojiRows[0]!)).toEqual([]);
  expect(order.marks(emojiRows[0]!, 7, 16)).toEqual([]);
  sent.length = 0;
});
