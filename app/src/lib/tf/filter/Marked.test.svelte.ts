/**
 * `<Marked>` with `FuzzyOrder` UTF-16 highlight pairs (#2520): paints matched
 * characters from `frizbee` pairs (including non-contiguous fuzzy hits, emoji
 * surrogate pairs, and `from` field offsets), and returns to a single plain
 * text node once the filter clears.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import Marked from "./Marked.svelte";

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
