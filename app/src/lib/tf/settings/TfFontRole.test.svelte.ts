/**
 * Settings › Fonts, one role's row (`TfFontRole.svelte`, 2026-11-18; *Code font* and *Ligatures in code* since
 * 2026-11-16): the field, its list with the role's kind first, the specimen that previews, the size and its marked
 * step, and the switch under Code.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { InstalledFont } from "../../ipc/app";
import { createFonts, type Fonts, type Role, type RootStyle } from "../fonts.svelte";
import type { KeptStorage } from "../kept-value";
import TfFontRole, { forgetInstalledFonts } from "./TfFontRole.svelte";
import TfFontsSection from "./TfFontsSection.svelte";

function storage(): KeptStorage {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

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

const font = (family: string, kind: Partial<InstalledFont> = {}): InstalledFont => ({
  family,
  monospace: false,
  serif: false,
  narrows: false,
  x_height: null,
  ...kind,
});

const FONTS: InstalledFont[] = [
  font("Fira Code", { monospace: true, x_height: 0.527 }),
  font("Menlo", { monospace: true, x_height: 0.547 }),
  font("Arial", { x_height: 0.519 }),
  font("Avenir Next Condensed", { narrows: true, x_height: 0.468 }),
  font("Georgia", { serif: true, x_height: 0.481 }),
];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let fonts: Fonts;
let style: ReturnType<typeof rootStyle>;

beforeEach(() => {
  forgetInstalledFonts();
  style = rootStyle();
  fonts = createFonts(() => storage(), () => style);
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

async function draw(role: Role = "code", read: () => Promise<InstalledFont[]> = () => Promise.resolve(FONTS)) {
  const installedFonts = vi.fn(read);
  view = mount(TfFontRole, { target, props: { role, installedFonts, fonts } });
  await settle();
  return installedFonts;
}

const field = () => target.querySelector<HTMLInputElement>('input[role="combobox"]')!;
const options = () => [...target.querySelectorAll('[role="option"]')].map((el) => el.textContent!.trim());
const heads = () => [...target.querySelectorAll(".tf-cf-mh")].map((el) => el.textContent);
const caption = () => target.querySelector(".tf-fr-spec figcaption")!.textContent!.replace(/\s+/g, " ").trim();
const specimen = () => target.querySelector<HTMLElement>(".tf-fr-spec > :first-child")!;
const slider = () => target.querySelector<HTMLElement>('[role="slider"]')!;
const button = (text: string) => [...target.querySelectorAll("button")].find((b) => b.textContent === text);
const key = (name: string, on: HTMLElement = field()) => {
  on.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  flushSync();
};
const open = () => {
  field().click();
  flushSync();
};

test("the list holds the machine's families, monospace first under their headings, and is read once a session", async () => {
  const read = await draw();
  // Focus alone opens nothing: Settings puts the focus in a section's first field as it opens.
  field().dispatchEvent(new FocusEvent("focus"));
  flushSync();
  expect(target.querySelector('[role="listbox"]')).toBeNull();
  open();
  expect(target.querySelector('[role="listbox"]')?.getAttribute("aria-label")).toBe("Installed fonts");
  expect(heads()).toEqual(["Monospace", "Other fonts"]);
  expect(options()).toEqual(["Fira Code", "Menlo", "Arial", "Avenir Next Condensed", "Georgia"]);
  unmount(view!);
  view = null;
  const again = await draw();
  expect(read).toHaveBeenCalledTimes(1);
  expect(again).not.toHaveBeenCalled();
});

test.each([
  ["ui", ["Sans serif", "Other fonts"], ["Arial", "Avenir Next Condensed", "Fira Code", "Georgia", "Menlo"]],
  ["head", ["Draws narrow", "Normal width only"], ["Avenir Next Condensed", "Arial", "Fira Code", "Georgia", "Menlo"]],
  ["read", ["Serif", "Other fonts"], ["Georgia", "Arial", "Avenir Next Condensed", "Fira Code", "Menlo"]],
] as const)("%s lists its own kind first, each group by name whatever order the machine gave", async (role, wantHeads, wantOptions) => {
  await draw(role);
  open();
  expect(heads()).toEqual(wantHeads);
  expect(options()).toEqual(wantOptions);
});

test("the option under the keys draws in the specimen before it is chosen; Enter chooses it for the whole app", async () => {
  await draw();
  expect(caption()).toBe("Cascadia Mono, 100 %");
  key("ArrowDown");
  key("ArrowDown");
  expect(specimen().style.getPropertyValue("--mono")).toMatch(/^"Menlo"/);
  expect(caption()).toBe("Menlo, 100 %");
  expect(fonts.family("code")).toBeNull();
  expect(style.props.has("--mono-chosen")).toBe(false);
  key("Enter");
  expect(fonts.family("code")).toBe("Menlo");
  expect(style.props.get("--mono-chosen")).toBe('"Menlo"');
  expect(field().value).toBe("Menlo");
  expect(target.querySelector('[role="listbox"]')).toBeNull();
  // The specimen draws the chosen face again once the list is closed.
  expect(specimen().style.getPropertyValue("--mono")).toMatch(/^"Menlo"/);
});

test.each([
  ["ui", "--sans", "Arial", '"Inter"'],
  ["head", "--head", "Avenir Next Condensed", '"Inter"'],
  ["read", "--serif", "Georgia", '"Inter"'],
] as const)("%s's specimen previews through %s, and shows the bundled face with nothing chosen", async (role, property, pick, bundled) => {
  await draw(role);
  expect(specimen().style.getPropertyValue(property)).toMatch(new RegExp(`^${bundled}`));
  open();
  key("ArrowDown");
  expect(specimen().style.getPropertyValue(property)).toMatch(new RegExp(`^"${pick}"`));
});

test("Esc puts the chosen family back and keeps the dialog under it open", async () => {
  await draw();
  fonts.setFamily("code", "Fira Code");
  field().value = "Geo";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(options()).toEqual(["Georgia"]);
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  field().dispatchEvent(event);
  flushSync();
  expect(event.defaultPrevented).toBe(true);
  expect(field().value).toBe("Fira Code");
  expect(fonts.family("code")).toBe("Fira Code");
});

test("a name typed that is not installed is taken, and the specimen says it draws as the bundled face", async () => {
  await draw("read");
  field().value = "Iowan Old Style";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  key("Enter");
  expect(fonts.family("read")).toBe("Iowan Old Style");
  expect(style.props.get("--read-chosen")).toBe('"Iowan Old Style"');
  expect(caption()).toBe("Iowan Old Style is not installed on this machine, so this draws as Inter.");
  // Leaving the field chooses what it holds, as Enter does.
  field().value = "Georgia";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  field().dispatchEvent(new FocusEvent("blur"));
  flushSync();
  expect(fonts.family("read")).toBe("Georgia");
});

test("a Headings face that cannot draw narrow says it draws at its normal width", async () => {
  await draw("head");
  fonts.setFamily("head", "Arial");
  flushSync();
  expect(caption()).toBe("Arial has no narrow width, so headings and the job key draw at its normal width.");
  fonts.setFamily("head", "Avenir Next Condensed");
  flushSync();
  expect(caption()).toBe("Avenir Next Condensed, 100 %");
});

test("Use the bundled face goes back to it", async () => {
  await draw("ui");
  expect(button("Use Inter")).toBeUndefined();
  fonts.setFamily("ui", "Arial");
  flushSync();
  button("Use Inter")!.click();
  flushSync();
  expect(fonts.family("ui")).toBeNull();
  expect(style.props.has("--ui-chosen")).toBe(false);
  expect(field().value).toBe("");
});

test("the list failing to read leaves a field that still takes a name", async () => {
  await draw("code", () => Promise.reject(new Error("no fonts")));
  open();
  expect(target.querySelector('[role="listbox"]')).toBeNull();
  field().value = "Menlo";
  field().dispatchEvent(new Event("input", { bubbles: true }));
  key("Enter");
  expect(fonts.family("code")).toBe("Menlo");
  expect(caption()).toBe("Menlo, 100 %");
});

test("the size is a slider of the ten steps: the arrows step it, Home and End go to the ends, 0 to 100 %", async () => {
  await draw("read");
  expect(slider().getAttribute("aria-label")).toBe("Reading size");
  expect(slider().getAttribute("aria-valuenow")).toBe("100");
  expect(target.querySelectorAll(".tf-fr-notch")).toHaveLength(10);
  key("ArrowRight", slider());
  key("ArrowUp", slider());
  expect(fonts.size("read")).toBe(110);
  expect(style.props.get("--size-read")).toBe("1.1");
  expect(slider().getAttribute("aria-valuetext")).toBe("110 %");
  expect(target.querySelector(".tf-fr-fig")!.textContent).toBe("110 %");
  key("End", slider());
  expect(fonts.size("read")).toBe(130);
  key("ArrowRight", slider());
  expect(fonts.size("read")).toBe(130);
  key("Home", slider());
  key("ArrowLeft", slider());
  expect(fonts.size("read")).toBe(85);
  key("0", slider());
  expect(fonts.size("read")).toBe(100);
  expect(style.props.has("--size-read")).toBe(false);
});

test("a notch clicked sets its size, and Back to 100 % shows only off 100", async () => {
  await draw();
  expect(button("Back to 100 %")).toBeUndefined();
  target.querySelectorAll<HTMLButtonElement>(".tf-fr-notch")[6]!.click();
  flushSync();
  expect(fonts.size("code")).toBe(115);
  expect(target.querySelector(".tf-fr-on")).toBe(target.querySelectorAll(".tf-fr-notch")[6]);
  expect(caption()).toBe("Cascadia Mono, 115 %");
  button("Back to 100 %")!.click();
  flushSync();
  expect(fonts.size("code")).toBe(100);
});

test("the step that evens the chosen face out is ringed and named, and nothing sets itself", async () => {
  await draw("ui");
  expect(target.querySelector(".tf-fr-even")).toBeNull();
  // Avenir Next Condensed's x-height .468 against Inter's .546: 117 %, on the scale at 115.
  fonts.setFamily("ui", "Avenir Next Condensed");
  flushSync();
  const even = target.querySelector(".tf-fr-even")!;
  expect(even).toBe(target.querySelectorAll(".tf-fr-notch")[6]);
  expect(even.getAttribute("title")).toBe("115 %: lower case as tall as Inter's");
  expect(target.querySelector(".tf-fr-hint")!.textContent).toBe("The ring at 115 % draws Avenir Next Condensed's lower case as tall as Inter's.");
  expect(fonts.size("ui")).toBe(100);
  fonts.setSize("ui", 115);
  flushSync();
  expect(target.querySelector(".tf-fr-hint")).toBeNull();
});

test("Ligatures in code is a switch under Code alone, off until turned on, and on draws Cascadia Code for all code at once", async () => {
  await draw("read");
  expect(target.querySelector('[role="switch"]')).toBeNull();
  unmount(view!);
  view = null;
  await draw("code");
  const sw = target.querySelector<HTMLButtonElement>('[role="switch"]')!;
  expect(sw.getAttribute("aria-label")).toBe("Ligatures in code");
  expect(sw.getAttribute("aria-checked")).toBe("false");
  expect(caption()).toBe("Cascadia Mono, 100 %");
  sw.click();
  flushSync();
  expect(sw.getAttribute("aria-checked")).toBe("true");
  expect(style.props.get("--mono-ligatures")).toBe("normal");
  expect(style.props.get("--mono-bundled")).toBe('"Cascadia Code"');
  // The row names the face drawn: the caption, the field's placeholder.
  expect(caption()).toBe("Cascadia Code, 100 %");
  expect(target.querySelector<HTMLInputElement>('[role="combobox"]')!.placeholder).toBe("Cascadia Code (comes with knobas)");
  sw.click();
  flushSync();
  expect(style.props.has("--mono-ligatures")).toBe(false);
  expect(style.props.has("--mono-bundled")).toBe(false);
  expect(caption()).toBe("Cascadia Mono, 100 %");
});

test("the Fonts section is the proof line and the four roles in order, one read of the fonts for all", async () => {
  const installedFonts = vi.fn(() => Promise.resolve(FONTS));
  view = mount(TfFontsSection, { target, props: { installedFonts } });
  await settle();
  expect(target.querySelector("h1")!.textContent).toBe("Fonts");
  expect(target.querySelector(".tf-stg-h p")!.textContent!.replace(/\s+/g, " ").trim()).toBe(
    "Size evens a face out against the others; to make everything bigger, use Zoom in Appearance. A font this computer does not have draws in the one that comes with knobas. Kept on this computer.",
  );
  expect([...target.querySelectorAll(".tf-fr-pw")].map((el) => el.className.match(/tf-fr-pw-(\w+)/)![1])).toEqual(["ui", "head", "read", "code"]);
  expect([...target.querySelectorAll('[role="slider"]')].map((el) => el.getAttribute("aria-label"))).toEqual([
    "Interface size",
    "Headings size",
    "Reading size",
    "Code size",
  ]);
  expect(installedFonts).toHaveBeenCalledTimes(1);
});

test("the faces that come with knobas are listed with their licences, each text read when it is opened (2026-11-20)", async () => {
  view = mount(TfFontsSection, { target, props: { installedFonts: () => Promise.resolve(FONTS) } });
  await settle();
  const section = target.querySelector<HTMLElement>(".tf-fr-lics")!;
  expect(section.querySelector("h2")!.textContent).toBe("Fonts that come with knobas");
  expect(section.querySelector(".tf-fr-lics-note")!.textContent).toBe("Free to use, share and bundle under the SIL Open Font License 1.1.");
  const rows = [...section.querySelectorAll<HTMLDetailsElement>("details.tf-fr-lic")];
  expect(rows.map((row) => row.querySelector("summary")!.textContent!.replace(/\s+/g, " ").trim())).toEqual([
    "Inter 4.1Interface, Headings, Reading© The Inter Project Authors",
    "Cascadia Mono 2407.24Code© Microsoft Corporation",
    "Cascadia Code 2407.24Code, with ligatures on© Microsoft Corporation",
  ]);
  const text = rows[1]!.querySelector("pre")!;
  expect(text.textContent).toBe("");
  rows[1]!.open = true;
  rows[1]!.dispatchEvent(new Event("toggle"));
  await vi.waitFor(() => expect(text.textContent).toContain("with Reserved Font Name Cascadia Code."));
  expect(text.textContent).toContain("SIL OPEN FONT LICENSE Version 1.1");
  expect(rows[0]!.querySelector("pre")!.textContent).toBe("");
});
