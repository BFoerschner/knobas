/**
 * Settings › Appearance › *Code font* and *Ligatures in code* (2026-11-16), and the setting under them
 * (`code-face.svelte.ts`): what is kept across a restart, what lands on <html> for every place code is drawn, and
 * what the field, the list, the specimen and the switch do.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { InstalledFont } from "../../ipc/app";
import { createCodeFace, cssFamily, familyName, FONT_KEY, LIGATURES_KEY, type CodeFace, type RootStyle } from "../code-face.svelte";
import type { KeptStorage } from "../kept-value";
import TfCodeFont, { forgetInstalledFonts } from "./TfCodeFont.svelte";

/** A `localStorage` of its own, kept across "restarts" (a second `createCodeFace` over it). */
function storage(): KeptStorage & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

/** <html>'s inline style, as the setting writes it. */
function rootStyle(): RootStyle & { props: Map<string, string> } {
  const props = new Map<string, string>();
  return {
    props,
    setProperty: (name: string, value: string | null) => void props.set(name, value ?? ""),
    removeProperty: (name: string) => {
      const was = props.get(name) ?? "";
      props.delete(name);
      return was;
    },
  };
}

test("a family is a trimmed name, its CSS quotes dropped, and the bundled face or nothing is no choice", () => {
  expect(familyName("  Fira Code ")).toBe("Fira Code");
  expect(familyName('"Iosevka Term"')).toBe("Iosevka Term");
  expect(familyName("'Menlo'")).toBe("Menlo");
  expect(familyName("jetbrains mono")).toBeNull();
  expect(familyName("   ")).toBeNull();
  expect(familyName(null)).toBeNull();
  expect(familyName("a\nb")).toBe("ab");
  expect(familyName("x".repeat(201))).toBeNull();
  expect(cssFamily('Odd "Name" \\ 2')).toBe('"Odd \\"Name\\" \\\\ 2"');
});

test("nothing kept: the bundled face, no ligatures, and nothing written on <html>", () => {
  const style = rootStyle();
  const face = createCodeFace(() => storage(), () => style);
  expect(face.family).toBeNull();
  expect(face.ligatures).toBe(false);
  expect([...style.props]).toEqual([]);
});

test("a family and the ligatures are kept across a restart and written on <html> at once, before anything draws", () => {
  const kept = storage();
  const style = rootStyle();
  const face = createCodeFace(() => kept, () => style);
  face.setFamily("Fira Code");
  face.setLigatures(true);
  expect(style.props.get("--mono-chosen")).toBe('"Fira Code"');
  expect(style.props.get("--mono-ligatures")).toBe("normal");
  expect(kept.items.get(FONT_KEY)).toBe("Fira Code");
  expect(kept.items.get(LIGATURES_KEY)).toBe("on");

  // The next run: written as the module loads, so the first draw is already in it.
  const after = rootStyle();
  const again = createCodeFace(() => kept, () => after);
  expect(again.family).toBe("Fira Code");
  expect(again.ligatures).toBe(true);
  expect(after.props.get("--mono-chosen")).toBe('"Fira Code"');
  expect(after.props.get("--mono-ligatures")).toBe("normal");

  again.setFamily(null);
  again.setLigatures(false);
  expect([...after.props]).toEqual([]);
  expect([...kept.items]).toEqual([]);
});

test("storage that throws costs the memory, not the choice", () => {
  const broken: KeptStorage = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
    removeItem: () => {
      throw new Error("blocked");
    },
  };
  const style = rootStyle();
  const face = createCodeFace(() => broken, () => style);
  face.setFamily("Menlo");
  expect(face.family).toBe("Menlo");
  expect(style.props.get("--mono-chosen")).toBe('"Menlo"');
});

const FONTS: InstalledFont[] = [
  { family: "Fira Code", monospace: true },
  { family: "Menlo", monospace: true },
  { family: "Arial", monospace: false },
  { family: "Georgia", monospace: false },
];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let face: CodeFace;
let style: ReturnType<typeof rootStyle>;

beforeEach(() => {
  forgetInstalledFonts();
  style = rootStyle();
  face = createCodeFace(() => storage(), () => style);
  target = document.createElement("div");
  target.className = "tf";
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

async function draw(read: () => Promise<InstalledFont[]> = () => Promise.resolve(FONTS)) {
  const installedFonts = vi.fn(read);
  view = mount(TfCodeFont, { target, props: { installedFonts, face } });
  await settle();
  return installedFonts;
}

const field = () => target.querySelector<HTMLInputElement>('input[role="combobox"]')!;
const options = () => [...target.querySelectorAll('[role="option"]')].map((el) => el.textContent!.trim());
const caption = () => target.querySelector("figcaption")!.textContent!.replace(/\s+/g, " ").trim();
const specimen = () => target.querySelector<HTMLElement>(".tf-cf-spec pre")!;
const key = (name: string) => {
  field().dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  flushSync();
};

test("the list holds the machine's families, monospace first under their headings, and is read once a session", async () => {
  const read = await draw();
  // Focus alone opens nothing: Settings puts the focus in a section's first field as it opens.
  field().dispatchEvent(new FocusEvent("focus"));
  flushSync();
  expect(target.querySelector('[role="listbox"]')).toBeNull();
  field().click();
  flushSync();
  expect(target.querySelector('[role="listbox"]')?.getAttribute("aria-label")).toBe("Installed fonts");
  expect([...target.querySelectorAll(".tf-cf-mh")].map((el) => el.textContent)).toEqual(["Monospace", "Other fonts"]);
  expect(options()).toEqual(["Fira Code", "Menlo", "Arial", "Georgia"]);
  unmount(view!);
  view = null;
  const again = await draw();
  expect(read).toHaveBeenCalledTimes(1);
  expect(again).not.toHaveBeenCalled();
});

test("the option under the keys draws in the specimen before it is chosen; Enter chooses it for the whole app", async () => {
  await draw();
  expect(caption()).toBe("JetBrains Mono");
  key("ArrowDown");
  key("ArrowDown");
  expect(specimen().style.getPropertyValue("font-family")).toBe('"Menlo", var(--mono)');
  expect(caption()).toBe("Menlo");
  expect(face.family).toBeNull();
  expect(style.props.has("--mono-chosen")).toBe(false);
  key("Enter");
  expect(face.family).toBe("Menlo");
  expect(style.props.get("--mono-chosen")).toBe('"Menlo"');
  expect(field().value).toBe("Menlo");
  expect(target.querySelector('[role="listbox"]')).toBeNull();
});

test("Esc puts the chosen family back and keeps the dialog under it open", async () => {
  await draw();
  face.setFamily("Fira Code");
  field().value = "Geo";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(options()).toEqual(["Georgia"]);
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  field().dispatchEvent(event);
  flushSync();
  expect(event.defaultPrevented).toBe(true);
  expect(field().value).toBe("Fira Code");
  expect(face.family).toBe("Fira Code");
});

test("a name typed that is not installed is taken, and the specimen says it draws as JetBrains Mono", async () => {
  await draw();
  field().value = "Iosevka Term";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  key("Enter");
  expect(face.family).toBe("Iosevka Term");
  expect(style.props.get("--mono-chosen")).toBe('"Iosevka Term"');
  expect(caption()).toBe("Iosevka Term is not installed on this machine, so code draws as JetBrains Mono.");
  // Leaving the field chooses what it holds, as Enter does.
  field().value = "Menlo";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  field().dispatchEvent(new FocusEvent("blur"));
  flushSync();
  expect(face.family).toBe("Menlo");
});

test("Use JetBrains Mono goes back to the bundled face", async () => {
  await draw();
  expect([...target.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("Use JetBrains Mono");
  face.setFamily("Menlo");
  flushSync();
  const back = [...target.querySelectorAll("button")].find((b) => b.textContent === "Use JetBrains Mono")!;
  back.click();
  flushSync();
  expect(face.family).toBeNull();
  expect(style.props.has("--mono-chosen")).toBe(false);
  expect(field().value).toBe("");
});

test("the list failing to read leaves a field that still takes a name", async () => {
  await draw(() => Promise.reject(new Error("no fonts")));
  field().click();
  flushSync();
  expect(target.querySelector('[role="listbox"]')).toBeNull();
  field().value = "Menlo";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  key("Enter");
  expect(face.family).toBe("Menlo");
  expect(caption()).toBe("Menlo");
});

test("Ligatures in code is a switch, off until set, that turns the ligatures of all code on and off at once", async () => {
  await draw();
  const sw = target.querySelector<HTMLButtonElement>('[role="switch"]')!;
  expect(sw.getAttribute("aria-label")).toBe("Ligatures in code");
  expect(sw.getAttribute("aria-checked")).toBe("false");
  sw.click();
  flushSync();
  expect(sw.getAttribute("aria-checked")).toBe("true");
  expect(face.ligatures).toBe(true);
  expect(style.props.get("--mono-ligatures")).toBe("normal");
  sw.click();
  flushSync();
  expect(sw.getAttribute("aria-checked")).toBe("false");
  expect(style.props.has("--mono-ligatures")).toBe(false);
});
