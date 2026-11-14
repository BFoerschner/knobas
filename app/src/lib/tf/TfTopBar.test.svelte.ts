/**
 * The top bar counts as a header (#2290, design-system spec, Controls): its
 * icon buttons are squares at the header height, `--control-h`, like the
 * default button beside them. jsdom resolves no `var()`, so the computed
 * value is the token the cascade picked.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

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
      badges: { builds: null, monitors: null },
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

// The Windows pass (2026-11-14): the search box said ⌘/ in knobas's WebView2 window on Windows.
test("the search box, the timer and the gear name the platform's modifier: Ctrl on Windows, ⌘ on the Mac", () => {
  const kbd = () => target.querySelector(".tf-search kbd")!.textContent;
  expect(kbd()).toBe("⌘/");
  const agent = vi.spyOn(navigator, "userAgent", "get").mockReturnValue(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0",
  );
  unmount(view!);
  view = mount(TfTopBar, {
    target,
    props: {
      current: null,
      job: "PAY-231",
      inbox: { count: 0 },
      badges: { builds: null, monitors: null },
      timer: { current: null, elapsed: null },
      onsearch: () => {},
      ontimer: () => {},
      ontheme: () => {},
    } as never,
  });
  flushSync();
  expect(kbd()).toBe("Ctrl+/");
  expect(target.querySelector(".tf-timer")!.getAttribute("title")).toBe("Start the timer on PAY-231 (Ctrl+T)");
  expect(target.querySelector('a[aria-label="Settings"]')!.getAttribute("title")).toBe("Settings (Ctrl+,)");
  agent.mockRestore();
});
