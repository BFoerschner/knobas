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
  version: null,
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

async function draw(role: Role = "code", read: () => Promise<InstalledFont[]> = () => Promise.resolve(FONTS), draws: (family: string) => boolean = () => false) {
  const installedFonts = vi.fn(read);
  view = mount(TfFontRole, { target, props: { role, installedFonts, fonts, draws } });
  await settle();
  return installedFonts;
}

const field = () => target.querySelector<HTMLInputElement>('input[role="combobox"]')!;
const options = () => [...target.querySelectorAll('[role="option"]')].map((el) => el.textContent!.trim());
const heads = () => [...target.querySelectorAll(".tf-cf-mh")].map((el) => el.textContent);
const caption = () => target.querySelector(".tf-fr-spec figcaption")!.textContent!.replace(/\s+/g, " ").trim();
const specimen = () => target.querySelector<HTMLElement>(".tf-fr-spec > :first-child")!;
const slider = (label?: string) => (label ? target.querySelector<HTMLElement>(`[role="slider"][aria-label="${label}"]`)! : target.querySelector<HTMLElement>('[role="slider"]')!);
/** A slider's notches. */
const notches = (label: string) => [...slider(label).querySelectorAll<HTMLButtonElement>(".tf-fr-notch")];
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
  // Code's list starts with the faces that come with knobas (2026-11-21).
  expect(heads()).toEqual(["Comes with knobas", "Monospace", "Other fonts"]);
  expect(options()).toEqual(["Cascadia Mono", "Cascadia Code with ligatures", "Cascadia Mono 2009.22 older release", "Cascadia Code 2009.22 older release, with ligatures", "Fira Code", "Menlo", "Arial", "Avenir Next Condensed", "Georgia"]);
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
  expect(caption()).toBe("Cascadia Code 2009.22, 100 %");
  for (let i = 0; i < 6; i++) key("ArrowDown");
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

test("the faces that come with knobas are the switch's two places: picking one sets it, and the selected row follows it", async () => {
  await draw();
  open();
  const selected = () => [...target.querySelectorAll('[role="option"][aria-selected="true"]')].map((el) => el.textContent!.trim());
  expect(selected()).toEqual(["Cascadia Code 2009.22 older release, with ligatures"]);
  key("ArrowDown");
  key("ArrowDown");
  // Under the keys, the specimen draws the bundled Cascadia Code by its own name.
  expect(specimen().style.getPropertyValue("--mono")).toMatch(/^"knobas Cascadia Code"/);
  expect(caption()).toBe("Cascadia Code, 100 %");
  key("Enter");
  expect(fonts.family("code")).toBeNull();
  expect(fonts.ligatures).toBe(true);
  expect(field().value).toBe("");
  open();
  expect(selected()).toEqual(["Cascadia Code with ligatures"]);
  // The switch turned by hand moves the selected row back.
  fonts.setLigatures(false);
  flushSync();
  expect(selected()).toEqual(["Cascadia Mono"]);
});

test("an installed Cascadia reads (installed) with its version, is chosen as the machine's copy, and stays chosen when the field is left", async () => {
  await draw("code", () => Promise.resolve([font("Cascadia Mono", { monospace: true, x_height: 0.518, version: "2407.024" }), ...FONTS]));
  open();
  expect(options()).toEqual(["Cascadia Mono", "Cascadia Code with ligatures", "Cascadia Mono 2009.22 older release", "Cascadia Code 2009.22 older release, with ligatures", "Cascadia Mono (installed) 2407.024", "Fira Code", "Menlo", "Arial", "Avenir Next Condensed", "Georgia"]);
  for (let i = 0; i < 5; i++) key("ArrowDown");
  expect(specimen().style.getPropertyValue("--mono")).toMatch(/^"Cascadia Mono"/);
  expect(caption()).toBe("Cascadia Mono (installed), 100 %");
  key("Enter");
  expect(fonts.family("code")).toBe("Cascadia Mono");
  expect(style.props.get("--mono-chosen")).toBe('"Cascadia Mono"');
  expect(field().value).toBe("Cascadia Mono (installed)");
  // Leaving the field as it is keeps the machine's copy; it is not read back as the bundled face.
  field().dispatchEvent(new FocusEvent("blur"));
  flushSync();
  expect(fonts.family("code")).toBe("Cascadia Mono");
  open();
  const selected = [...target.querySelectorAll('[role="option"][aria-selected="true"]')].map((el) => el.textContent!.trim());
  expect(selected).toEqual(["Cascadia Mono (installed) 2407.024"]);
});

test("a Cascadia the webview draws but the read missed (Windows Terminal's) is listed as installed, with no version", async () => {
  const asked: string[] = [];
  await draw("code", () => Promise.resolve(FONTS), (family) => (asked.push(family), family === "Cascadia Code"));
  open();
  expect(asked).toEqual(["Cascadia Mono", "Cascadia Code"]);
  expect(options()).toEqual(["Cascadia Mono", "Cascadia Code with ligatures", "Cascadia Mono 2009.22 older release", "Cascadia Code 2009.22 older release, with ligatures", "Cascadia Code (installed)", "Fira Code", "Menlo", "Arial", "Avenir Next Condensed", "Georgia"]);
  unmount(view!);
  view = null;
  // Interface's list is the read alone.
  forgetInstalledFonts();
  const other: string[] = [];
  await draw("ui", () => Promise.resolve(FONTS), (family) => (other.push(family), true));
  expect(other).toEqual([]);
});

test("Code's specimen shows a box, a tree and the pairs ligatures join, as a file's rows", async () => {
  await draw();
  const rows = [...specimen().querySelectorAll(".tf-l > span:last-child")].map((el) => el.textContent);
  expect(rows.some((row) => /┌─+┐/.test(row!))).toBe(true);
  expect(rows.some((row) => row!.startsWith("├── "))).toBe(true);
  expect(rows.some((row) => row!.startsWith("│   └── "))).toBe(true);
  for (const pair of ["->", "=>", ">=", "!=", "==="]) expect(rows.join("\n")).toContain(pair);
  expect(specimen().classList.contains("tf-code")).toBe(true);
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
  // The faces that come with knobas are listed whatever the read did.
  expect(options()).toEqual(["Cascadia Mono", "Cascadia Code with ligatures", "Cascadia Mono 2009.22 older release", "Cascadia Code 2009.22 older release, with ligatures"]);
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
  expect(notches("Reading size")).toHaveLength(10);
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
  expect(caption()).toBe("Cascadia Code 2009.22, 115 %");
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

test("Ligatures in code is a switch under Code alone, on until turned off, and off draws Cascadia Mono for all code at once (#2516)", async () => {
  await draw("read");
  expect(target.querySelector('[role="switch"]')).toBeNull();
  unmount(view!);
  view = null;
  await draw("code");
  const sw = target.querySelector<HTMLButtonElement>('[role="switch"]')!;
  expect(sw.getAttribute("aria-label")).toBe("Ligatures in code");
  expect(sw.getAttribute("aria-checked")).toBe("true");
  expect(caption()).toBe("Cascadia Code 2009.22, 100 %");
  sw.click();
  flushSync();
  expect(sw.getAttribute("aria-checked")).toBe("false");
  expect(style.props.get("--mono-ligatures")).toBe("none");
  expect(style.props.get("--mono-bundled")).toBe('"knobas Cascadia Mono 2009"');
  // The row names the face drawn: the caption, the field's placeholder.
  expect(caption()).toBe("Cascadia Mono 2009.22, 100 %");
  expect(target.querySelector<HTMLInputElement>('[role="combobox"]')!.placeholder).toBe("Cascadia Mono 2009.22 (comes with knobas)");
  sw.click();
  flushSync();
  expect(style.props.has("--mono-ligatures")).toBe(false);
  expect(style.props.has("--mono-bundled")).toBe(false);
  expect(caption()).toBe("Cascadia Code 2009.22, 100 %");
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
  // Each role's size, then its knobs in one order: line spacing, letter spacing, text weight (Fable's ruling 6).
  expect([...target.querySelectorAll('[role="slider"]')].map((el) => el.getAttribute("aria-label"))).toEqual([
    "Interface size",
    "Interface letter spacing",
    "Interface text weight",
    "Headings size",
    "Headings letter spacing",
    "Reading size",
    "Reading line spacing",
    "Reading letter spacing",
    "Reading text weight",
    "Code size",
    "Code line spacing",
    "Code letter spacing",
    "Code text weight",
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
    "Cascadia Mono and Cascadia Code 2009.22Code, the older release© Microsoft Corporation",
  ]);
  const text = rows[1]!.querySelector("pre")!;
  expect(text.textContent).toBe("");
  rows[1]!.open = true;
  rows[1]!.dispatchEvent(new Event("toggle"));
  await vi.waitFor(() => expect(text.textContent).toContain("with Reserved Font Name Cascadia Code."));
  expect(text.textContent).toContain("SIL OPEN FONT LICENSE Version 1.1");
  expect(rows[0]!.querySelector("pre")!.textContent).toBe("");
});

test("each knob is a line of notches with its figure, the arrows step it, 0 goes home, and Back to shows off home", async () => {
  await draw("read");
  const lines = [...target.querySelectorAll(".tf-fr-knob .tf-fr-k")].map((el) => el.textContent);
  expect(lines).toEqual(["Size", "Line spacing", "Letter spacing", "Text weight"]);
  const lh = slider("Reading line spacing");
  expect(notches("Reading line spacing")).toHaveLength(11);
  expect(lh.querySelector(".tf-fr-home")).toBe(notches("Reading line spacing")[5]);
  key("ArrowRight", lh);
  key("ArrowRight", lh);
  expect(fonts.knob("read", "lh")).toBe(175);
  expect(style.props.get("--lh-read")).toBe("1.75");
  expect(lh.getAttribute("aria-valuetext")).toBe("1.75");
  expect(button("Back to 1.65")).toBeDefined();
  key("0", lh);
  expect(fonts.knob("read", "lh")).toBe(165);
  expect(button("Back to 1.65")).toBeUndefined();
  // Letter spacing reads in em, signed; a notch clicked sets it; Back to 0 puts it home.
  notches("Reading letter spacing")[0]!.click();
  flushSync();
  expect(fonts.knob("read", "ls")).toBe(-3);
  expect(style.props.get("--ls-read")).toBe("-0.03em");
  const figures = [...target.querySelectorAll(".tf-fr-knob .tf-fr-fig")].map((el) => el.textContent);
  expect(figures).toEqual(["100 %", "1.65", "−0.03 em", "400"]);
  button("Back to 0")!.click();
  flushSync();
  expect(style.props.has("--ls-read")).toBe(false);
  // The weight's three notches: 350, 400, 450.
  key("End", slider("Reading text weight"));
  expect(fonts.knob("read", "wt")).toBe(450);
  expect(style.props.get("--wt-read")).toBe("450");
});

test("code's line spacing is 1.60 until changed, Joined or Open, or a notch; the ring marks where boxes still join, and its note says what is drawn (#2516)", async () => {
  await draw("code");
  const note = () => target.querySelector(".tf-fr-linesnote")!.textContent;
  const pressed = () => [...target.querySelectorAll('.tf-fr-seg button[aria-pressed="true"]')].map((el) => el.textContent);
  const lines = slider("Code line spacing");
  // The default: the 1.60 notch filled and home, the ring on it too (2009.22's │ joins up to 1.60), no preset pressed.
  expect(pressed()).toEqual([]);
  expect(fonts.lines).toBe(160);
  expect(lines.querySelector(".tf-fr-on")).toBe(notches("Code line spacing")[10]);
  expect(lines.querySelector(".tf-fr-home")).toBe(notches("Code line spacing")[10]);
  expect(lines.querySelector(".tf-fr-even")).toBe(notches("Code line spacing")[10]);
  expect(target.querySelector(".tf-fr-lines .tf-fr-fig")!.textContent).toBe("1.60");
  expect(note()).toBe("1.60 times the size. Cascadia Code 2009.22's box lines join up to the ring at 1.60.");
  expect(button("Back to 1.60")).toBeUndefined();
  button("Joined")!.click();
  flushSync();
  expect(fonts.lines).toBe("joined");
  expect(pressed()).toEqual(["Joined"]);
  expect(note()).toBe("Rows are as tall as Cascadia Code 2009.22 itself, 1.22 times the size. Box lines and trees join.");
  // An arrow steps from the drawn 1.22 to the next notch that way.
  key("ArrowRight", lines);
  expect(fonts.lines).toBe(125);
  expect(pressed()).toEqual([]);
  expect(target.querySelector(".tf-fr-lines .tf-fr-fig")!.textContent).toBe("1.25");
  // 0 goes home: 1.60.
  key("0", lines);
  expect(fonts.lines).toBe(160);
  button("Open")!.click();
  flushSync();
  expect(fonts.lines).toBe("open");
  expect(style.props.get("--code-lh")).toBe("initial");
  expect(target.querySelector(".tf-fr-lines .tf-fr-fig")!.textContent).toBe("1.5 to 1.6");
  expect(note()).toBe("The spacing before: 1.6 in files and diffs, 1.55 in logs, 1.5 in short logs.");
  key("ArrowLeft", lines);
  expect(fonts.lines).toBe(155);
  button("Back to 1.60")!.click();
  flushSync();
  expect(fonts.lines).toBe(160);
  // The newer release's │ is shorter: the ring moves to 1.20, and the same spacing is kept.
  fonts.setFamily("code", "Cascadia Mono");
  flushSync();
  expect(lines.querySelector(".tf-fr-even")).toBe(notches("Code line spacing")[2]);
  expect(fonts.lines).toBe(160);
  expect(note()).toBe("1.60 times the size. Cascadia Mono's box lines join up to the ring at 1.20.");
  // Code's letter spacing says what it costs.
  expect([...target.querySelectorAll(".tf-fr-hint")].map((el) => el.textContent)).toContain("Ligatures draw only at 0. Above 0, box lines show gaps.");
});

test("the older Cascadia is two rows that come with knobas, and picking one sets the release and the switch", async () => {
  await draw("code");
  open();
  for (let i = 0; i < 4; i++) key("ArrowDown");
  expect(specimen().style.getPropertyValue("--mono")).toMatch(/^"knobas Cascadia Code 2009"/);
  expect(caption()).toBe("Cascadia Code 2009.22, 100 %");
  key("Enter");
  expect(fonts.release).toBe("2009.22");
  expect(fonts.ligatures).toBe(true);
  expect(fonts.family("code")).toBeNull();
  expect(target.querySelector<HTMLInputElement>('[role="combobox"]')!.placeholder).toBe("Cascadia Code 2009.22 (comes with knobas)");
  open();
  const selected = [...target.querySelectorAll('[role="option"][aria-selected="true"]')].map((el) => el.textContent!.trim());
  expect(selected).toEqual(["Cascadia Code 2009.22 older release, with ligatures"]);
});
