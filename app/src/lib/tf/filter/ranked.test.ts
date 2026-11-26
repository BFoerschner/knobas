/**
 * A page's list ranked by `fuzzy_rank` (the fuzzy-search pass, 2026-11-19): the latest answer only, what was drawn
 * kept until it lands, the filter as it was where the ask fails.
 */
import { expect, test, vi } from "vitest";

const pending: { text: string; resolve: (ranked: unknown) => void; reject: (error: unknown) => void }[] = [];
vi.mock("../../ipc/search", () => ({
  fuzzyRank: (text: string) =>
    new Promise((resolve, reject) => {
      pending.push({ text, resolve, reject });
    }),
}));

const { FuzzyOrder, inOrder } = await import("./ranked.svelte");

const rows = ["acme/auth-proxy", "acme/ADProxy", "acme/routing"];
const same = (row: string) => row;
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the answer's rows are drawn once it lands, what was drawn until then, and a late answer is dropped", async () => {
  const order = new FuzzyOrder<string>();
  expect(order.rows("", rows, same)).toEqual(rows);
  order.ask("prox", rows, same);
  order.ask("proxy", rows, same);
  // Nothing answered: what was drawn stays.
  expect(order.rows("proxy", rows, same)).toEqual(rows);
  expect(order.settled("proxy", rows)).toBe(false);
  // The answer to the text the page moved past is dropped.
  pending[0]!.resolve([{ index: 2, loose: false, marks: [] }]);
  await flush();
  expect(order.rows("proxy", rows, same)).toEqual(rows);
  pending[1]!.resolve([
    { index: 1, loose: false, marks: [7, 12] },
    { index: 0, loose: false, marks: [10, 15] },
  ]);
  await flush();
  expect(order.settled("proxy", rows)).toBe(true);
  expect(order.rows("proxy", rows, same)).toEqual(["acme/ADProxy", "acme/auth-proxy"]);
  expect(order.marks("acme/ADProxy")).toEqual([7, 12]);
  // No text: the rows as given.
  expect(order.rows("", rows, same)).toEqual(rows);
  pending.length = 0;
});

test("a list drawn for the first time with words kept draws the filter as it was, and a failed ask is that filter, marked", async () => {
  const order = new FuzzyOrder<string>();
  expect(order.rows("routing", rows, same)).toEqual(["acme/routing"]);
  order.ask("auth", rows, same);
  pending[0]!.reject(new Error("no such command"));
  await flush();
  expect(order.rows("auth", rows, same)).toEqual(["acme/auth-proxy"]);
  // Each typed word is marked where it stands (#2532), so a field marks what the filter kept it by.
  expect(order.marks("acme/auth-proxy")).toEqual([5, 9]);
  pending.length = 0;
});

test("`typed` is the text while its answer is out and nothing once it lands, for a field's words to mark (#2532)", async () => {
  const order = new FuzzyOrder<string>();
  expect(order.typed("", rows)).toBe("");
  order.ask("prox", rows, same);
  expect(order.typed("prox", rows)).toBe("prox");
  pending[0]!.resolve([{ index: 1, loose: false, marks: [7, 11] }]);
  await flush();
  expect(order.typed("prox", rows)).toBe("");
  // An answer for other rows is not theirs: the words mark until theirs lands.
  expect(order.typed("prox", rows.slice(1))).toBe("prox");
  pending.length = 0;
});

test("`ready` waits for a fresh list's first answer, then holds while what it drew stands (#2532)", async () => {
  const order = new FuzzyOrder<string>();
  order.ask("prox", rows, same);
  // Nothing drawn for these rows and no answer: a page keeps what it drew.
  expect(order.ready("prox", rows)).toBe(false);
  pending[0]!.resolve([{ index: 0, loose: false, marks: [] }]);
  await flush();
  expect(order.ready("prox", rows)).toBe(true);
  expect(order.rows("prox", rows, same)).toEqual(["acme/auth-proxy"]);
  // The next key: its answer is out, and the rows drawn for these rows stand meanwhile.
  order.ask("proxy", rows, same);
  expect(order.ready("proxy", rows)).toBe(true);
  expect(order.rows("proxy", rows, same)).toEqual(["acme/auth-proxy"]);
  // Other rows are a fresh list again.
  expect(order.ready("proxy", rows.slice(1))).toBe(false);
  pending.length = 0;
});

test("inOrder keeps the rows the answer kept in the page's order, the same list where it kept every one (#2532)", () => {
  expect(inOrder(rows, ["acme/routing", "acme/auth-proxy"])).toEqual(["acme/auth-proxy", "acme/routing"]);
  expect(inOrder(rows, [...rows].reverse())).toBe(rows);
  expect(inOrder(rows, [])).toEqual([]);
});

test("an older text's answer that lands after a newer one's never replaces it", async () => {
  const order = new FuzzyOrder<string>();
  order.ask("proxy", rows, same);
  order.ask("prox", rows, same);
  pending[1]!.resolve([{ index: 1, loose: false, marks: [] }]);
  await flush();
  pending[0]!.resolve([{ index: 0, loose: false, marks: [] }]);
  await flush();
  expect(order.settled("prox", rows)).toBe(true);
  expect(order.rows("prox", rows, same)).toEqual(["acme/ADProxy"]);
  pending.length = 0;
});

test("clearing the query drops stale highlight marks immediately, and marks slices multi-field UTF-16 ranges", async () => {
  const emojiRows = ["🚀 acme/ADProxy", "acme/routing"];
  const order = new FuzzyOrder<string>();
  order.ask("proxy", emojiRows, same);
  // "🚀 acme/AD" is 2 + 1 + 7 = 10 UTF-16 code units; "Proxy" is [10, 15].
  pending[0]!.resolve([{ index: 0, loose: false, marks: [10, 15] }]);
  await flush();
  expect(order.marks(emojiRows[0]!)).toEqual([10, 15]);
  // Field slice starting after "🚀 acme/" (8 UTF-16 units), length 7 ("ADProxy"):
  expect(order.marks(emojiRows[0]!, 8, 7)).toEqual([2, 7]);
  // Field slice before the match ("🚀 acme/", 0..8) has no marks:
  expect(order.marks(emojiRows[0]!, 0, 8)).toEqual([]);

  // Clearing via rows("") immediately hides stale marks even before ask("") runs (first render frame).
  expect(order.rows("", emojiRows, same)).toEqual(emojiRows);
  expect(order.marks(emojiRows[0]!)).toEqual([]);

  // Clearing via ask("") clears `#answer` permanently.
  order.ask("   ", emojiRows, same);
  expect(order.marks(emojiRows[0]!)).toEqual([]);
  // Even after rows() sees a new non-empty query before its answer lands (keeping what was drawn),
  // the cleared answer's old marks never return.
  expect(order.rows("routing", emojiRows, same)).toEqual(emojiRows);
  expect(order.marks(emojiRows[0]!)).toEqual([]);
  pending.length = 0;
});
