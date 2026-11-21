/**
 * Settings › Appearance › Code colours (`TfThemesSection.svelte`, ADR-0051): the two slots, the editor of one theme,
 * its contrast rows and their fix, the swatch's panel, import, and Delete with Undo.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { plainPort } from "../../render/port";
import { reset as resetToasts, toasts } from "../../shell/toasts.svelte";
import type { KeptStorage } from "../kept-value";
import { APP_SLOT_KEY, createThemes, SLOT_KEY, THEMES_KEY, type Themes, type ThemeRoot } from "../themes.svelte";
import { createTheme } from "../theme.svelte";
import TfThemesSection from "./TfThemesSection.svelte";

function memory(seed: Record<string, string> = {}): KeptStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}
function root(): ThemeRoot & { props: Map<string, string> } {
  const props = new Map<string, string>();
  return { props, style: { setProperty: (k: string, v: string) => void props.set(k, v), removeProperty: (k: string) => (props.delete(k), "") }, setAttribute: () => {}, removeAttribute: () => {} };
}

/** A light theme of one's own whose string falls under 4.5:1. */
const PALE = { id: "own-pale0001", name: "Pale", kind: "light", from: "knobas Light", syntax: { kw: "#7A3EB1", str: "#50A14F" } };

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let storage: ReturnType<typeof memory>;
let html: ReturnType<typeof root>;
let themes: Themes;

function draw(seed: Record<string, string> = {}): void {
  storage = memory(seed);
  html = root();
  themes = createThemes(() => storage, () => html);
  view = mount(TfThemesSection, { target, props: { themes, mode: createTheme(), render: plainPort, prefersDark: () => false } });
  flushSync();
}

const q = <E extends Element = HTMLElement>(selector: string) => target.querySelector<E>(selector);
const button = (text: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) ?? null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  resetToasts();
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

test("choosing in a slot writes the kept value and the variables, and shows that theme in the editor", () => {
  draw();
  const select = q<HTMLSelectElement>('select[aria-label="Theme while dark"]')!;
  select.value = "dracula";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(storage.data.get(SLOT_KEY.dark)).toBe("dracula");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  expect(q(".tf-thm-edh h2")?.textContent).toBe("Dracula");
});

test("the slot the window draws now has the ring", () => {
  draw();
  expect(q('[data-slot="light"]')?.classList.contains("tf-cur")).toBe(true);
  expect(q('[data-slot="dark"]')?.classList.contains("tf-cur")).toBe(false);
});

test("a built-in has no swatch buttons, and Duplicate and edit puts the copy where the original drew: the window's slot, code following", () => {
  draw();
  expect(q(".tf-thm-edh .tf-lab")?.textContent).toBe("comes with knobas");
  expect(target.querySelectorAll("button.tf-thm-sw")).toHaveLength(0);
  button("Duplicate and edit")!.click();
  flushSync();
  expect(q(".tf-thm-edh h2")?.textContent).toBe("knobas Light copy");
  expect(q(".tf-thm-edh .tf-lab")?.textContent).toBe("yours, from knobas Light");
  const copy = themes.own[0]!;
  expect(themes.slot("light")).toBe(copy.id);
  expect(storage.data.get(APP_SLOT_KEY.light)).toBe(copy.id);
  expect(storage.data.has(SLOT_KEY.light)).toBe(false);
  expect(target.querySelectorAll("button.tf-thm-sw")).toHaveLength(17 + 4);
});

test("a row under 4.5:1 names the surface and fixes the colour by lightness, with Undo on the toast", () => {
  draw({ [SLOT_KEY.light]: PALE.id, [THEMES_KEY]: JSON.stringify([PALE]) });
  const row = q('[data-role="str"]')!;
  expect(row.classList.contains("tf-low")).toBe(true);
  expect(row.querySelector(".tf-thm-why")?.textContent).toMatch(/^2\.\d:1 on a removed line\. Text needs 4\.5:1\./);
  [...row.querySelectorAll("button")].find((b) => b.textContent === "Darken to 4.5:1")!.click();
  flushSync();
  expect(q('[data-role="str"]')!.classList.contains("tf-low")).toBe(false);
  expect(themes.find(PALE.id)!.syntax.str).not.toBe("#50A14F");
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("String darkened to 4.5:1.");
  toast.action!.run();
  flushSync();
  expect(themes.find(PALE.id)!.syntax.str).toBe("#50A14F");
});

test("Delete… acts at once, the slot goes back to knobas's, and Undo restores the theme and its slot", () => {
  draw({ [SLOT_KEY.light]: PALE.id, [THEMES_KEY]: JSON.stringify([PALE]) });
  button("Delete…")!.click();
  flushSync();
  expect(themes.own).toHaveLength(0);
  expect(themes.slot("light")).toBe("knobas-light");
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Deleted Pale.");
  toast.action!.run();
  flushSync();
  expect(themes.own.map((t) => t.id)).toEqual([PALE.id]);
  expect(themes.slot("light")).toBe(PALE.id);
  expect(q(".tf-thm-edh h2")?.textContent).toBe("Pale");
});

test("a swatch's panel writes a typed colour at once, and Esc closes it leaving focus on the swatch", async () => {
  draw({ [SLOT_KEY.light]: PALE.id, [THEMES_KEY]: JSON.stringify([PALE]) });
  const swatch = q<HTMLButtonElement>('[data-role="kw"] button.tf-thm-sw')!;
  swatch.click();
  flushSync();
  const field = q<HTMLInputElement>(".tf-thm-pop .tf-stg-in")!;
  field.value = "#123456";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  // Drawn by the next frame (ADR-0055: an edit is applied once a frame at most).
  themes.flush();
  expect(html.props.get("--hl-kw-light")).toBe("#123456");
  field.focus();
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  await tick();
  expect(q(".tf-thm-pop")).toBeNull();
  expect(document.activeElement).toBe(swatch);
});

test("importing a VS Code theme shows it in the editor and offers to fix what is under 4.5:1", async () => {
  draw();
  const input = q<HTMLInputElement>('input[type="file"]')!;
  const text = JSON.stringify({ name: "Sunny Light", type: "light", tokenColors: [{ scope: "string", settings: { foreground: "#E5C07B" } }] });
  Object.defineProperty(input, "files", { value: [new File([text], "sunny.json")], configurable: true });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  for (let i = 0; i < 20 && q(".tf-thm-edh h2")?.textContent !== "Sunny Light"; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 5));
    flushSync();
  }
  expect(q(".tf-thm-edh h2")?.textContent).toBe("Sunny Light");
  // Shown, not put in a slot. Its one `string` rule colours escapes and links too: three colours under 4.5:1.
  expect(themes.slot("light")).toBe("knobas-light");
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Imported Sunny Light as a light theme. 3 colours are under 4.5:1.");
  toast.action!.run();
  flushSync();
  expect(q('[data-role="str"]')!.classList.contains("tf-low")).toBe(false);
});

test("a file that is not a theme is refused in one sentence", async () => {
  draw();
  const input = q<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(input, "files", { value: [new File(['{"name":"x"}'], "x.json")], configurable: true });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  for (let i = 0; i < 20 && toasts.items.length === 0; i += 1) await new Promise((resolve) => setTimeout(resolve, 5));
  expect(toasts.items.at(-1)?.text).toBe("That file is not a theme knobas can read. It has no `knobasTheme`, `tokenColors` or `syntax`.");
});

test("Same as the app theme follows the window's theme, and a choice sets the slot apart", () => {
  draw();
  const select = q<HTMLSelectElement>('select[aria-label="Theme while dark"]')!;
  expect(select.value).toBe("");
  expect(select.options[0]!.textContent).toBe("Same as the app theme");
  themes.chooseApp("dracula");
  flushSync();
  expect(themes.drawn("dark").id).toBe("dracula");
  select.value = "one-dark";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(storage.data.get(SLOT_KEY.dark)).toBe("one-dark");
  select.value = "";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(storage.data.has(SLOT_KEY.dark)).toBe(false);
  expect(themes.drawn("dark").id).toBe("dracula");
});

test("a theme's contrast is read on the window theme drawn for its side, and its small print names it", () => {
  draw({ [APP_SLOT_KEY.light]: "github-light", [SLOT_KEY.light]: "knobas-light" });
  expect(q(".tf-thm-grp small")?.textContent).toBe("lowest contrast on any surface of GitHub Light");
  // knobas Light's string on GitHub Light's added line, not knobas's own.
  expect(q('[data-role="str"] .tf-thm-cr')?.textContent).toMatch(/^\d+\.\d:1$/);
});

test("a theme with window colours that the window does not draw offers Use for the app too; one without says so", () => {
  draw({ [SLOT_KEY.dark]: "dracula" });
  const select = q<HTMLSelectElement>('select[aria-label="Theme while dark"]')!;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  themes.choose("dracula");
  flushSync();
  q<HTMLButtonElement>('.tf-thm-slot[data-slot="dark"] .tf-btn')!.click();
  flushSync();
  button("Use for the app too")!.click();
  flushSync();
  expect(themes.appDrawn("dark").id).toBe("dracula");
  expect(themes.follows("dark")).toBe(true);
  expect(button("Use for the app too")).toBeNull();
});
