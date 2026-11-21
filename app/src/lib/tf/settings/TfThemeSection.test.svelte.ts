/**
 * Settings › Appearance › Theme (`TfThemeSection.svelte`, ADR-0055): the window slots, the editor's rows, their fixes
 * and *Auto*, the code line, the preview, the import band, and a theme that cannot be read asking first.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import type { KeptStorage } from "../kept-value";
import { failures } from "../theme/app-check";
import { derive } from "../theme/app-derive";
import { ROW_KEYS } from "../theme/app-roles";
import { createTheme } from "../theme.svelte";
import { APP_SLOT_KEY, ASIS_KEY, createThemes, KEEP_SECONDS, SLOT_KEY, THEMES_KEY, type Themes, type ThemeRoot } from "../themes.svelte";
import TfThemeKeep from "../TfThemeKeep.svelte";
import TfThemeSection from "./TfThemeSection.svelte";

function memory(seed: Record<string, string> = {}): KeptStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}
function root(): ThemeRoot & { props: Map<string, string> } {
  const props = new Map<string, string>();
  return { props, style: { setProperty: (k: string, v: string) => void props.set(k, v), removeProperty: (k: string) => (props.delete(k), "") }, setAttribute: () => {}, removeAttribute: () => {} };
}

const MINE = { id: "own-mine0001", name: "Mine", kind: "light", from: "knobas Light", app: { sheet: "#FFFFFF", ink: "#202020" }, syntax: {} };
/** Text at 2.2:1 on its panel: under the floor. */
const FAINT = { id: "own-faint001", name: "Faint", kind: "light", app: { sheet: "#FAFAFA", ink: "#AAAAAA" }, syntax: {} };

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];
let storage: ReturnType<typeof memory>;
let html: ReturnType<typeof root>;
let themes: Themes;

function draw(seed: Record<string, string> = {}, dark = false): void {
  storage = memory(seed);
  html = root();
  themes = createThemes(() => storage, () => html, { set: setTimeout, clear: clearTimeout, frame: (run) => run() });
  views.push(mount(TfThemeSection, { target, props: { themes, mode: createTheme(), prefersDark: () => dark } }));
  views.push(mount(TfThemeKeep, { target, props: { themes } }));
  flushSync();
}
const q = <E extends Element = HTMLElement>(selector: string) => target.querySelector<E>(selector);
const button = (text: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) ?? null;
const choose = (slot: "light" | "dark", id: string) => {
  const select = q<HTMLSelectElement>(`select[aria-label="Window theme while ${slot}"]`)!;
  select.value = id;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
};
const stage = () => q(".tf-apt-stage")!;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  resetToasts();
});
afterEach(() => {
  views.forEach((v) => unmount(v));
  views = [];
  target.remove();
  vi.useRealTimers();
});

test("choosing in a slot keeps the id and writes the properties; a built-in has no swatch buttons, and Duplicate and edit puts the copy in that slot", () => {
  draw();
  choose("dark", "dracula");
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe("dracula");
  expect(html.props.get("--sheet-dark")).toBe("#282A36");
  expect(q(".tf-thm-edh h2")?.textContent).toBe("Dracula");
  expect(target.querySelectorAll("button.tf-thm-sw")).toHaveLength(0);
  button("Duplicate and edit")!.click();
  flushSync();
  const copy = themes.own[0]!;
  expect(copy.name).toBe("Dracula copy");
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe(copy.id);
  // One swatch for each of the 30 row colours; the fold's 19 open on a press.
  expect(target.querySelectorAll("button.tf-thm-sw")).toHaveLength(30);
});

test("a row's swatch sets its key; the preview and the figure follow; Auto takes it out again and says auto", () => {
  draw({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const row = () => q('[data-row="ok"]')!;
  expect(row().querySelector(".tf-apt-auto")).not.toBeNull();
  row().querySelector<HTMLButtonElement>("button.tf-thm-sw")!.click();
  flushSync();
  const field = q<HTMLInputElement>(".tf-thm-pop .tf-stg-in")!;
  field.value = "#2E7D32";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(stage().style.getPropertyValue("--ok")).toBe("#2E7D32");
  expect(row().querySelector(".tf-apt-auto")).toBeNull();
  const want = derive("light", { ...MINE.app, ok: "#2E7D32" }).colours;
  expect(row().querySelector(".tf-thm-cr")?.textContent).toMatch(/^\d+\.\d:1$/);
  expect(want.ok).toBe("#2E7D32");
  button("Auto")!.click();
  flushSync();
  expect(themes.find(MINE.id)!.app!.ok).toBeUndefined();
  expect(row().querySelector(".tf-apt-auto")).not.toBeNull();
  expect(stage().style.getPropertyValue("--ok")).toBe(derive("light", MINE.app).colours.ok);
});

test("a row under its mark says the pair and fixes it by lightness, with Undo on the toast", () => {
  const low = { ...MINE, app: { ...MINE.app, ok: "#7BC67E" } };
  draw({ [APP_SLOT_KEY.light]: low.id, [THEMES_KEY]: JSON.stringify([low]) });
  const row = q('[data-row="ok"]')!;
  expect(row.classList.contains("tf-low")).toBe(true);
  expect(row.querySelector(".tf-thm-why")?.textContent).toMatch(/^\d\.\d:1 on (the page|the panel|the inset|its tint)\. Text needs 4\.5:1\./);
  row.querySelector<HTMLButtonElement>(".tf-thm-why .tf-btn")!.click();
  flushSync();
  expect(failures(derive("light", themes.find(low.id)!.app).colours).filter((f) => f.key === "ok")).toEqual([]);
  toasts.items.at(-1)!.action!.run();
  flushSync();
  expect(themes.find(low.id)!.app!.ok).toBe("#7BC67E");
});

test("the code line: Set apart and Follow this theme swap, and the code slot's kept value with them", () => {
  draw();
  const slot = q('.tf-thm-slot[data-slot="light"]')!;
  expect(slot.querySelector(".tf-apt-code span")?.textContent).toBe("Code colours follow this theme.");
  button("Set apart")!.click();
  flushSync();
  expect(storage.data.get(SLOT_KEY.light)).toBe("knobas-light");
  expect(slot.querySelector(".tf-apt-code span")?.textContent).toBe("Code colours are knobas Light.");
  button("Follow this theme")!.click();
  flushSync();
  expect(storage.data.has(SLOT_KEY.light)).toBe(false);
});

test("a dark theme edited while the window is light is drawn on its own page in the preview, and the line says so", () => {
  draw();
  choose("dark", "dracula");
  expect(stage().style.getPropertyValue("--paper")).toBe("#21222C");
  expect(html.props.get("--paper-light")).toBeUndefined();
  expect(q(".tf-apt-pvh .tf-dim")?.textContent).toBe("Shown as it draws while the window is dark.");
});

test("every one of the 30 row colours is named by some part of the preview", () => {
  draw();
  const named = new Set([...target.querySelectorAll("[data-keys]")].flatMap((el) => el.getAttribute("data-keys")!.split(" ")));
  expect(ROW_KEYS.filter((key) => !named.has(key))).toEqual([]);
});

test("the band: The app only while code follows sets code apart first, so code's properties never change", async () => {
  draw();
  const before = html.props.get("--hl-kw-dark");
  const file = new File([JSON.stringify({ name: "Night", type: "dark", colors: { "editor.background": "#1A1B26", "editor.foreground": "#A9B1D6" }, tokenColors: [{ scope: "keyword", settings: { foreground: "#BB9AF7" } }] })], "night.json");
  const input = q<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(input, "files", { value: [file] });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  for (let i = 0; i < 20 && !button("The app only"); i += 1) await new Promise((r) => setTimeout(r, 5)), flushSync();
  expect(q(".tf-apt-use span")?.textContent).toBe("Night has colours for the window and for code. Use it while dark for");
  // Nothing is in a slot yet.
  expect(themes.appDrawn("dark").id).toBe("knobas-dark");
  const sets: string[] = [];
  const set = html.style.setProperty;
  html.style.setProperty = (k: string, v: string) => (k === "--hl-kw-dark" && sets.push(v), set(k, v));
  button("The app only")!.click();
  flushSync();
  expect(themes.appDrawn("dark").name).toBe("Night");
  expect(themes.follows("dark")).toBe(false);
  expect(storage.data.get(SLOT_KEY.dark)).toBe("knobas-dark");
  expect(sets.every((v) => v !== "#BB9AF7")).toBe(true);
  expect(html.props.get("--hl-kw-dark")).not.toBe("#BB9AF7");
  expect(before).toBeUndefined();
  expect(q(".tf-apt-use")).toBeNull();
});

test("a theme that cannot be read asks first: Cancel leaves the slot, Fix and use draws it fixed", () => {
  draw({ [THEMES_KEY]: JSON.stringify([FAINT]) });
  choose("light", FAINT.id);
  expect(themes.appDrawn("light").id).toBe("knobas-light");
  expect(q('[role="dialog"] h2')?.textContent).toBe("Faint would be hard to read");
  button("Cancel")!.click();
  flushSync();
  expect(storage.data.has(APP_SLOT_KEY.light)).toBe(false);
  choose("light", FAINT.id);
  button("Fix and use")!.click();
  flushSync();
  expect(themes.appDrawn("light").id).toBe(FAINT.id);
  expect(failures(derive("light", themes.find(FAINT.id)!.app).colours)).toEqual([]);
});

test("Use as it is draws it, in a plate of knobas Light's own colours; the time running out goes back, Keep it keeps it", () => {
  vi.useFakeTimers();
  draw({ [THEMES_KEY]: JSON.stringify([FAINT]) });
  choose("light", FAINT.id);
  button("Use as it is")!.click();
  flushSync();
  expect(themes.appDrawn("light").id).toBe(FAINT.id);
  const plate = q('[role="alertdialog"]')!;
  expect(plate.textContent).toContain("Keep Faint? Going back to knobas Light in");
  expect(plate.style.getPropertyValue("--sheet")).toBe("#F7F8F9");
  expect(plate.style.getPropertyValue("--ink")).toBe("#17212B");
  vi.advanceTimersByTime(KEEP_SECONDS * 1000);
  flushSync();
  expect(themes.appDrawn("light").id).toBe("knobas-light");
  expect(storage.data.has(APP_SLOT_KEY.light)).toBe(false);
  expect(html.props.get("--sheet-light")).toBeUndefined();
  expect(q('[role="alertdialog"]')).toBeNull();
  choose("light", FAINT.id);
  button("Use as it is")!.click();
  flushSync();
  button("Keep it")!.click();
  flushSync();
  vi.advanceTimersByTime(KEEP_SECONDS * 1000);
  expect(storage.data.get(ASIS_KEY)).toBe(JSON.stringify([FAINT.id]));
  expect(themes.appDrawn("light").id).toBe(FAINT.id);
});

test("an edit that takes the drawn theme under the floor starts the plate, and the time running out undoes it; an edit back over it ends the plate", () => {
  vi.useFakeTimers();
  draw({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  themes.update(MINE.id, (t) => ({ ...t, app: { ...t.app, ink: "#F0F0F0" } }));
  flushSync();
  expect(q('[role="alertdialog"]')?.textContent).toContain("Undoing that change in");
  expect(themes.appDrawn("light").id).toBe(MINE.id);
  vi.advanceTimersByTime(KEEP_SECONDS * 1000);
  flushSync();
  expect(themes.find(MINE.id)!.app!.ink).toBe("#202020");
  themes.update(MINE.id, (t) => ({ ...t, app: { ...t.app, ink: "#F0F0F0" } }));
  flushSync();
  themes.update(MINE.id, (t) => ({ ...t, app: { ...t.app, ink: "#101010" } }));
  flushSync();
  expect(q('[role="alertdialog"]')).toBeNull();
  expect(themes.find(MINE.id)!.app!.ink).toBe("#101010");
});

test("pick mode: a press on the Attention chip lists its colours nearest first; a pick opens that swatch and ends the mode", async () => {
  draw({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const toggle = button("Pick from the preview")!;
  toggle.click();
  flushSync();
  expect(toggle.getAttribute("aria-pressed")).toBe("true");
  expect(stage().classList.contains("tf-apt-picking")).toBe(true);
  const chip = stage().querySelector('.tf-chip.tf-warn[data-keys^="warn "]')!;
  chip.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 40, clientY: 40 }));
  flushSync();
  await Promise.resolve();
  flushSync();
  const items = [...document.querySelectorAll('[role="menu"] [role="menuitem"]')].map((b) => b.querySelector("span")?.textContent);
  expect(items.slice(0, 3)).toEqual(["Attention", "Attention's tint", "Panel"]);
  expect(items.at(-1)).toBe("Page");
  (document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role="menuitem"]')[1]!).click();
  for (let i = 0; i < 5; i += 1) {
    flushSync();
    await Promise.resolve();
  }
  flushSync();
  const where = document.activeElement;
  expect(where?.outerHTML.slice(0, 120)).toContain("tf-thm-sw");
  expect(toggle.getAttribute("aria-pressed")).toBe("false");
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Change Attention's tint");
  expect(q('.tf-thm-pop[aria-label="Attention\'s tint"]')).not.toBeNull();
  // The toggle starts it again, and a second press ends it.
  toggle.click();
  flushSync();
  expect(stage().classList.contains("tf-apt-picking")).toBe(true);
  toggle.click();
  flushSync();
  expect(stage().classList.contains("tf-apt-picking")).toBe(false);
});
