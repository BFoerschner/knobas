/**
 * Settings' nav as a tree (the subtrees ruling, 2026-11-21): Appearance holds Code colours and Fonts; a click opens a
 * parent's page and fold and never closes it, a click on the open parent toggles, the chevron folds alone; the folds
 * are kept; a move to a child's page opens its parent; the Docs tree's keys walk it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { KeptStorage } from "../kept-value";
import { createSettingsFolds, FOLDED_KEY, type SettingsFolds } from "./settings-folds.svelte";
import type { SettingsSection } from "./settings";
import TfSettingsNav from "./TfSettingsNav.svelte";

function memory(seed: Record<string, string> = {}): KeptStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let storage: ReturnType<typeof memory>;
let folds: SettingsFolds;
const props = $state<{ current: SettingsSection | null }>({ current: "sources" });
let onpage: ReturnType<typeof vi.fn<() => void>>;

function draw(current: SettingsSection | null, seed: Record<string, string> = {}): void {
  storage = memory(seed);
  folds = createSettingsFolds(() => storage);
  props.current = current;
  onpage = vi.fn<() => void>();
  view = mount(TfSettingsNav, {
    target,
    props: {
      get current() {
        return props.current;
      },
      folds,
      onpage,
    },
  });
  flushSync();
}

const fold = () => target.querySelector<HTMLDetailsElement>("details.tf-stg-fold")!;
const title = () => target.querySelector<HTMLAnchorElement>("summary > a.tf-stg-pl")!;
/** The chevron: the summary's own room left of the title's link (jsdom lays nothing out, so left of 0). */
const chevron = () => fold().querySelector("summary")!;
const CHEVRON = { clientX: -1 };
const rowNames = () => [...target.querySelectorAll<HTMLAnchorElement>("a.tf-stg-leaf, a.tf-stg-pl")].map((a) => a.textContent?.trim());
function click(el: Element, init: MouseEventInit = {}): MouseEvent {
  const event = new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1, button: 0, ...init });
  el.dispatchEvent(event);
  flushSync();
  return event;
}

beforeEach(() => {
  target = document.createElement("nav");
  document.body.append(target);
  location.hash = "#/settings/sources";
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

test("the rows are the sections in their order, Theme, Code colours and Fonts under Appearance, every fold open at first", () => {
  draw("sources");
  expect(rowNames()).toEqual([
    "Sources",
    "Checkouts and Open in",
    "Time",
    "Desktop notifications",
    "Standup",
    "Monitoring",
    "Docs",
    "Search",
    "Notes and capture",
    "Backup and export",
    "Appearance",
    "Theme",
    "Code colours",
    "Fonts",
    "Performance",
    "Renderers",
    "Keyboard",
    "Logs and database",
    "Updates",
  ]);
  expect(fold().open).toBe(true);
  expect([...fold().querySelectorAll("a.tf-stg-child")].map((a) => a.getAttribute("href"))).toEqual(["#/settings/appearance/theme", "#/settings/appearance/code-colours", "#/settings/appearance/fonts"]);
});

test("a first click on a parent's title opens its page and its fold, and never closes it", () => {
  draw("sources", { [FOLDED_KEY]: "appearance" });
  expect(fold().open).toBe(false);
  const event = click(title());
  expect(event.defaultPrevented).toBe(false);
  expect(fold().open).toBe(true);
  expect(onpage).toHaveBeenCalledTimes(1);
  // Open already: a first click from another page leaves it open.
  props.current = "time";
  flushSync();
  click(title());
  expect(fold().open).toBe(true);
});

test("a click on the open parent's row toggles its fold and opens no page", () => {
  draw("appearance");
  location.hash = "#/settings/appearance";
  click(title());
  expect(fold().open).toBe(false);
  expect(location.hash).toBe("#/settings/appearance");
  expect(storage.data.get(FOLDED_KEY)).toBe("appearance");
  click(title());
  expect(fold().open).toBe(true);
  expect(storage.data.has(FOLDED_KEY)).toBe(false);
  expect(onpage).not.toHaveBeenCalled();
});

test("the chevron folds alone; Enter and a modified click keep the link's own way", () => {
  draw("sources");
  expect(click(chevron(), CHEVRON).defaultPrevented).toBe(true);
  expect(fold().open).toBe(false);
  expect(onpage).not.toHaveBeenCalled();
  // Enter on a link is a click with no count: the page only.
  click(title(), { detail: 0 });
  expect(fold().open).toBe(false);
  click(title(), { metaKey: true });
  expect(fold().open).toBe(false);
});

test("a fold is kept, and a parent added later starts open", () => {
  draw("sources");
  click(chevron(), CHEVRON);
  expect(storage.data.get(FOLDED_KEY)).toBe("appearance");
  const again = createSettingsFolds(() => storage);
  expect(again.isOpen("appearance")).toBe(false);
  expect(again.isOpen("sources")).toBe(true);
});

test("a move to a child's page opens its parent and keeps it; while folded, the parent marks that it holds the open page", () => {
  draw("code-colours", { [FOLDED_KEY]: "appearance" });
  expect(fold().open).toBe(true);
  expect(storage.data.has(FOLDED_KEY)).toBe(false);
  // The chevron may fold it while the child is open: the parent's row then marks that it holds the page.
  click(chevron(), CHEVRON);
  expect(fold().open).toBe(false);
  expect(fold().querySelector("summary")!.classList.contains("tf-holds")).toBe(true);
  expect(fold().querySelector('a[aria-current="page"]')?.textContent?.trim()).toBe("Code colours");
});

test("the tree is one Tab stop, the open page's row, and the arrows walk the rows shown", () => {
  draw("fonts");
  const rows = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-stg-leaf, a.tf-stg-pl")];
  expect(rows.filter((row) => row.tabIndex === 0).map((row) => row.textContent?.trim())).toEqual(["Fonts"]);
  const key = (el: Element, k: string) => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
    flushSync();
  };
  const fonts = rows.find((row) => row.textContent?.trim() === "Fonts")!;
  fonts.focus();
  key(fonts, "ArrowUp");
  expect(document.activeElement?.textContent?.trim()).toBe("Code colours");
  key(document.activeElement!, "ArrowLeft");
  expect(document.activeElement?.textContent?.trim()).toBe("Appearance");
  key(document.activeElement!, "ArrowLeft");
  expect(fold().open).toBe(false);
  key(document.activeElement!, "ArrowDown");
  expect(document.activeElement?.textContent?.trim()).toBe("Performance");
  key(document.activeElement!, "ArrowUp");
  key(document.activeElement!, "ArrowRight");
  expect(fold().open).toBe(true);
});
