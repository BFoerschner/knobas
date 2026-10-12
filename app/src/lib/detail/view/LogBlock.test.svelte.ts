/**
 * The log block (ADR-0020): a build log's tail, drawn monospace as text. A
 * tail is read from its end -- where a build failed -- so the block opens
 * scrolled to the bottom (#665), and says so when the log was longer.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import LogBlock from "./LogBlock.svelte";

const mounted: (() => void)[] = [];
afterEach(() => {
  for (const done of mounted.splice(0)) done();
});

/** jsdom lays nothing out: give every `pre` a content taller than its box. */
function tall() {
  const proto = HTMLElement.prototype;
  const before = Object.getOwnPropertyDescriptor(proto, "scrollHeight");
  Object.defineProperty(proto, "scrollHeight", { configurable: true, get: () => 4000 });
  mounted.push(() => {
    if (before) Object.defineProperty(proto, "scrollHeight", before);
    else delete (proto as { scrollHeight?: number }).scrollHeight;
  });
}

function render(text: string, truncated: boolean) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(LogBlock, { target, props: { text, truncated } });
  flushSync();
  mounted.push(() => {
    unmount(app);
    target.remove();
  });
  return target;
}

test("a log opens scrolled to its end, where the build failed", () => {
  tall();
  const target = render("[19:51:41] step 1\n[19:51:44] Process exited with code 1", true);
  const pre = target.querySelector<HTMLPreElement>("pre")!;
  expect(pre.scrollTop).toBe(4000);
});

test("a cut log says so; a whole one does not; the text is text, never markup", () => {
  const cut = render("<b>not markup</b>", true);
  expect(cut.querySelector(".lb-note")?.textContent).toBe("The end of a longer log.");
  expect(cut.querySelector("pre")!.textContent).toBe("<b>not markup</b>");
  expect(cut.querySelector("b")).toBeNull();

  const whole = render("one line", false);
  expect(whole.querySelector(".lb-note")).toBeNull();
});
