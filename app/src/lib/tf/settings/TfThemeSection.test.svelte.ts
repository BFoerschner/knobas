/**
 * Settings › Appearance › Theme (`TfThemeSection.svelte`, ADR-0055): the window slots, the editor's rows, their fixes
 * and *Auto*, the code line, the preview, the import band, and a theme that cannot be read asking first.
 */
import { flushSync, mount, tick, unmount } from "svelte";
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
import { colourBlind } from "../colour-blind.svelte";

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
  // One swatch for each of the 32 row colours; the fold's 19 open on a press.
  expect(target.querySelectorAll("button.tf-thm-sw")).toHaveLength(32);
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

test("a picker drag colours the preview and its swatch each frame; the role list holds until the drag ends, or 300 ms after the last pick", async () => {
  draw({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const list = () => q(".tf-apt-roles")!;
  const hex = () => q('[data-row="paper"] .tf-thm-hex')!.textContent!.slice(0, 7);
  const swatch = () => q<HTMLButtonElement>('[data-row="paper"] button.tf-thm-sw')!;
  const before = derive("light", MINE.app).colours.paper;
  swatch().click();
  flushSync();
  const input = q<HTMLInputElement>('.tf-thm-pop input[type="color"]')!;
  const frame = () => new Promise((done) => requestAnimationFrame(() => done(true)));
  const send = (type: "input" | "change", value: string) => {
    input.value = value;
    input.dispatchEvent(new Event(type, { bubbles: true }));
  };
  send("input", "#d0e0f0");
  await frame();
  flushSync();
  expect(stage().style.getPropertyValue("--paper")).toBe("#D0E0F0");
  expect(swatch().style.getPropertyValue("background")).toMatch(/208, 224, 240|#d0e0f0/i);
  expect(list().style.getPropertyValue("--paper")).toBe(before);
  expect(hex()).toBe(before);
  send("input", "#c0d0e0");
  await frame();
  flushSync();
  expect(stage().style.getPropertyValue("--paper")).toBe("#C0D0E0");
  expect(hex()).toBe(before);
  // The drag's end: the list catches up at once.
  send("change", "#b0c0d0");
  flushSync();
  expect(hex()).toBe("#B0C0D0");
  expect(list().style.getPropertyValue("--paper")).toBe("#B0C0D0");
  expect(stage().style.getPropertyValue("--paper")).toBe("#B0C0D0");
  // A drag whose end never comes (WebKit's panel sends none): the list catches up with the keep, 300 ms after the last pick.
  send("input", "#a0b0c0");
  await frame();
  flushSync();
  expect(hex()).toBe("#B0C0D0");
  await new Promise((done) => setTimeout(done, 350));
  flushSync();
  expect(hex()).toBe("#A0B0C0");
  // A flush (a quit, a backup) brings it too.
  send("input", "#909090");
  await frame();
  flushSync();
  expect(hex()).toBe("#A0B0C0");
  themes.flush();
  flushSync();
  expect(hex()).toBe("#909090");
});

test("a picker drag leaves a row's figure and warning as they were until the colour rests, and a fix pressed meanwhile rests it first", async () => {
  draw({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const row = () => q('[data-row="ink"]')!;
  const figure = () => row().querySelector(".tf-thm-cr")!.textContent;
  const was = figure();
  expect(row().querySelector(".tf-thm-why")).toBeNull();
  row().querySelector<HTMLButtonElement>("button.tf-thm-sw")!.click();
  flushSync();
  const input = q<HTMLInputElement>('.tf-thm-pop input[type="color"]')!;
  // Text the colour of its panel: far under its mark.
  input.value = "#f0f0f0";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((done) => requestAnimationFrame(() => done(true)));
  flushSync();
  expect(stage().style.getPropertyValue("--ink")).toBe("#F0F0F0");
  expect(figure()).toBe(was);
  expect(row().querySelector(".tf-thm-why")).toBeNull();
  await new Promise((done) => setTimeout(done, 350));
  flushSync();
  expect(figure()).not.toBe(was);
  const fix = row().querySelector<HTMLButtonElement>(".tf-thm-why .tf-btn")!;
  expect(fix).not.toBeNull();
  // Moving again, then the fix pressed: it works from the colour now picked, which rests first.
  input.value = "#e8e8e8";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((done) => requestAnimationFrame(() => done(true)));
  flushSync();
  fix.click();
  flushSync();
  const ink = themes.find(MINE.id)!.app!.ink!;
  expect(ink).not.toBe("#E8E8E8");
  expect(failures(derive("light", themes.find(MINE.id)!.app!).colours).map((f) => f.key)).not.toContain("ink");
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
  expect(q<HTMLSelectElement>('select[aria-label="Window theme while light"]')!.value).toBe("knobas-light");
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

test("the Selected row's quick picks: the ink plate, the theme's accent, and the theme's own where its file had one", () => {
  const OFFERED = { ...MINE, id: "own-offered1", app: { sheet: "#FFFFFF", ink: "#202020", primary: "#0B5FFF" }, offers: { selRow: "#DDEBFF", onSelRow: "#0B1B33" } };
  draw({ [APP_SLOT_KEY.light]: OFFERED.id, [THEMES_KEY]: JSON.stringify([OFFERED]) });
  const pick = (words: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-apt-picks button")].find((b) => b.textContent?.trim() === words)!;
  expect(pick("knobas (ink)").getAttribute("aria-pressed")).toBe("true");
  pick("The theme's accent").click();
  flushSync();
  expect(themes.find(OFFERED.id)!.app!.selRow).toBe("#0B5FFF");
  expect(themes.find(OFFERED.id)!.app!.onSelRow).toBeUndefined();
  pick("The theme's own").click();
  flushSync();
  expect(themes.find(OFFERED.id)!.app).toMatchObject({ selRow: "#DDEBFF", onSelRow: "#0B1B33" });
  expect(html.props.get("--sel-row-light")).toBe("#DDEBFF");
  pick("knobas (ink)").click();
  flushSync();
  expect(themes.find(OFFERED.id)!.app!.selRow).toBeUndefined();
  expect(html.props.get("--sel-row-light")).toBe(html.props.get("--fill-light"));
});

test("colour-blind mode names the pairs that read alike under the States, and not while it is off", () => {
  draw();
  expect(target.querySelector(".tf-apt-cb")).toBeNull();
  colourBlind.set(true);
  flushSync();
  const lines = [...target.querySelectorAll(".tf-apt-cb")].map((l) => l.textContent);
  expect(lines).toContain("Attention and Failed read the same under deuteranopia.");
  colourBlind.set(false);
});

test("light or dark chosen while the page is open puts that side's window theme in the editor and its preview (#2516)", () => {
  storage = memory();
  html = root();
  themes = createThemes(() => storage, () => html, { set: setTimeout, clear: clearTimeout, frame: (run) => run() });
  const mode = createTheme();
  views.push(mount(TfThemeSection, { target, props: { themes, mode, prefersDark: () => false } }));
  flushSync();
  expect(q(".tf-thm-edh h2")?.textContent).toBe("knobas Light");
  mode.set("dark");
  flushSync();
  expect(q(".tf-thm-edh h2")?.textContent).toBe("knobas Dark");
  expect(target.querySelector(".tf-thm-side")?.textContent).not.toMatch(/Shown as it draws while the window is/);
  mode.set("light");
  flushSync();
  expect(q(".tf-thm-edh h2")?.textContent).toBe("knobas Light");
});

test("Edit on a slot scrolls smoothly to the editor and focuses its heading, the theme already shown included (#2516)", async () => {
  draw();
  const scrolled: { el: Element; how: unknown }[] = [];
  const was = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = function (this: HTMLElement, how?: unknown) {
    scrolled.push({ el: this, how });
  };
  try {
    button("Edit")!.click();
    flushSync();
    await tick();
    expect(document.activeElement).toBe(q(".tf-thm-edh h2"));
    expect(scrolled.at(-1)).toEqual({ el: q(".tf-thm-ed"), how: { behavior: "smooth", block: "start" } });
  } finally {
    HTMLElement.prototype.scrollIntoView = was;
  }
});
