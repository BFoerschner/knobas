/**
 * A fixed row (the XML and JSON views', `rowHeight`): its height is given at code's size of 100 % and follows code's
 * size, on the screen's pixels, while the lines are drawn too. A fixed 22 px row did not, so at 130 % the text stood
 * taller than its row (the orchestrator, 2026-11-24).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import TfLines from "./TfLines.svelte";
import { fonts } from "../fonts.svelte";
import type { RenderPort } from "../../render/port";

const quiet: RenderPort = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
let view: ReturnType<typeof mount> | null = null;
let target: HTMLElement | null = null;

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target?.remove();
});

function draw(props: Record<string, unknown>) {
  target = document.createElement("div");
  document.body.appendChild(target);
  view = mount(TfLines, { target, props: { text: "<a>\n  <b/>\n</a>\n", path: "a.xml", render: quiet, scrolls: true, find: "overlay", ...props } });
  flushSync();
  return () => target!.querySelector<HTMLElement>(".tf-code")!.style.getPropertyValue("--tf-row");
}

test("a fixed row is its height at 100 % times code's size, in whole pixels at a scale of 1, and follows a change", () => {
  fonts.setSize("code", 130);
  // As the XML view draws it: a fixed row, its long lines clipped.
  const row = draw({ rowHeight: 22, clip: { over: 400, keep: 320 } });
  expect(row()).toBe("29px");
  fonts.setSize("code", 100);
  flushSync();
  expect(row()).toBe("22px");
  fonts.setSize("code", 85);
  flushSync();
  expect(row()).toBe("19px");
});

test("Source's rows set no fixed row", () => {
  fonts.setSize("code", 130);
  expect(draw({})()).toBe("");
});
