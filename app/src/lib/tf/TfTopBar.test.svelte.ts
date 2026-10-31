/**
 * The top bar counts as a header (#2290, design-system spec, Controls): its
 * icon buttons are squares at the header height, `--control-h`, like the
 * default button beside them. jsdom resolves no `var()`, so the computed
 * value is the token the cascade picked.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfTopBar from "./TfTopBar.svelte";

const css = (name: string) => readFileSync(join(process.cwd(), "src/lib/tf/css", name), "utf8");

let target: HTMLElement;
let style: HTMLStyleElement;
let view: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  style = document.createElement("style");
  style.textContent = css("tokens.css") + css("chrome.css");
  document.head.append(style);
  target = document.createElement("div");
  target.className = "tf";
  document.body.append(target);
  view = mount(TfTopBar, {
    target,
    props: {
      current: null,
      job: null,
      inbox: { count: 0 },
      badges: { builds: null, monitors: null, proposals: null },
      timer: { current: null, elapsed: null },
      onsearch: () => {},
      ontimer: undefined,
      ontheme: () => {},
    } as never,
  });
  flushSync();
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
  style.remove();
});

test("the top bar's icon buttons are squares at the header height", () => {
  const icons = [...target.querySelectorAll(".tf-top .tf-iconbtn")];
  expect(icons.map((icon) => icon.getAttribute("aria-label"))).toEqual(["Settings", "Toggle colour theme"]);
  for (const icon of icons) {
    const drawn = getComputedStyle(icon);
    expect([drawn.width, drawn.height], icon.getAttribute("aria-label")!).toEqual(["var(--control-h)", "var(--control-h)"]);
    // A fixed square: padding would push the content box past it.
    expect(drawn.paddingTop).toMatch(/^0(px)?$/);
    expect(drawn.paddingLeft).toMatch(/^0(px)?$/);
  }
});

test("an icon button in a page body's row is a square at the small height", () => {
  const row = document.createElement("div");
  row.innerHTML = `<button class="tf-iconbtn tf-sm" aria-label="Collapse all"></button>`;
  target.append(row);
  const drawn = getComputedStyle(row.firstElementChild!);
  expect([drawn.width, drawn.height]).toEqual(["var(--control-h-sm)", "var(--control-h-sm)"]);
});
