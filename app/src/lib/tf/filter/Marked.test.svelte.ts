/**
 * `<Marked>` with `FuzzyOrder` UTF-16 highlight pairs (#2520): paints matched
 * characters from `frizbee` pairs (including non-contiguous fuzzy hits, emoji
 * surrogate pairs, and `from` field offsets), and returns to a single plain
 * text node once the filter clears.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

const pending: { text: string; resolve: (ranked: { index: number; loose: boolean; marks: number[] }[]) => void }[] = [];
vi.mock("../../ipc/search", () => ({
  fuzzyRank: (text: string) =>
    new Promise((resolve) => {
      pending.push({ text, resolve });
    }),
  fuzzyHold: async () => {},
  fuzzyRankHeld: async () => [],
  fuzzyDrop: async () => {},
}));

import Marked from "./Marked.svelte";
const { FuzzyOrder } = await import("./ranked.svelte");

function render(props: { text: string; typed?: string; pairs?: readonly number[]; from?: number }) {
  const target = document.createElement("div");
  document.body.append(target);
  const state = $state(props);
  const app = mount(Marked, { target, props: state });
  flushSync();
  return {
    target,
    state,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("paints non-contiguous UTF-16 pairs and honours field offsets without creating markup", () => {
  const { target, state, done } = render({
    text: "🚀 ADProxy",
    typed: "apr",
    // "🚀 " = 3 UTF-16 units; "A" = [3, 4], "Pr" = [5, 7]
    pairs: [3, 4, 5, 7],
  });
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["A", "Pr"]);
  expect(target.textContent).toBe("🚀 ADProxy");

  // Field offset `from`: pairs from `"acme/ADProxy"` applied to `"ADProxy"` (from = 5)
  state.text = "ADProxy";
  state.pairs = [0, 2, 7, 12];
  state.from = 5;
  flushSync();
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["Proxy"]);

  // Cleared filter (empty pairs + empty typed): single plain text node, zero <mark> elements.
  state.typed = "";
  state.pairs = [];
  state.from = 0;
  flushSync();
  expect(target.querySelectorAll("mark")).toHaveLength(0);
  expect(target.textContent).toBe("ADProxy");
  done();
});

test("Marked subscribing to FuzzyOrder.marks while starting cleared updates reactively when an answer lands (#2520)", async () => {
  const rows = ["🚀 acme/ADProxy", "acme/routing"];
  const same = (row: string) => row;
  const order = new FuzzyOrder<string>();
  // Page mounts with an empty filter: `rows("")` marks the order cleared before `<Marked>` reads `marks(row)`.
  order.rows("", rows, same);

  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Marked, {
    target,
    props: {
      text: "ADProxy",
      get pairs() {
        return order.marks(rows[0]!, 8, 7);
      },
    },
  });
  flushSync();
  expect(target.querySelectorAll("mark")).toHaveLength(0);

  // User types a query (no `typed` prop on `<Marked>`, so reactivity depends solely on `order.marks`).
  order.rows("proxy", rows, same);
  order.ask("proxy", rows, same);
  pending[0]!.resolve([{ index: 0, loose: false, marks: [10, 15] }]);
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["Proxy"]);

  // Clearing the query removes marks again, and a subsequent query paints fresh marks.
  order.rows("", rows, same);
  order.ask("", rows, same);
  flushSync();
  expect(target.querySelectorAll("mark")).toHaveLength(0);

  order.rows("ad", rows, same);
  order.ask("ad", rows, same);
  pending[1]!.resolve([{ index: 0, loose: false, marks: [8, 10] }]);
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  expect([...target.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["AD"]);

  unmount(app);
  target.remove();
  pending.length = 0;
});
