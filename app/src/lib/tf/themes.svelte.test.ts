/** The window's and code's slots on <html>, a person's own themes, and what is kept (ADR-0051, ADR-0055). */
import { expect, test, vi } from "vitest";

import { ROLES } from "../render/roles";
import type { KeptStorage } from "./kept-value";
import { derive } from "./theme/app-derive";
import { APP_KEYS } from "./theme/app-roles";
import { BUILTINS } from "./theme/builtins";
import { createTheme, MODE_KEY } from "./theme.svelte";
import { APP_SLOT_KEY, ASIS_KEY, createThemes, PAINT_KEY, SAVE_AFTER, scopeValues, SLOT_KEY, THEMES_KEY, UNCHANGED_KEY, type ThemeRoot, type ThemeTimers } from "./themes.svelte";

function memory(seed: Record<string, string> = {}): KeptStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

function root(): ThemeRoot & { props: Map<string, string>; attrs: Map<string, string> } {
  const props = new Map<string, string>();
  const attrs = new Map<string, string>();
  return {
    props,
    attrs,
    style: { setProperty: (k: string, v: string) => void props.set(k, v), removeProperty: (k: string) => (props.delete(k), "") },
    setAttribute: (k, v) => void attrs.set(k, v),
    removeAttribute: (k) => void attrs.delete(k),
  };
}

/** Timers whose frames run when the test says. */
function frames(): ThemeTimers & { run(): number } {
  let queue: (() => void)[] = [];
  return {
    set: setTimeout,
    clear: clearTimeout,
    frame: (run) => queue.push(run),
    run() {
      const now = queue;
      queue = [];
      now.forEach((f) => f());
      return now.length;
    },
  };
}

const MINE = { id: "own-abcdefgh", name: "Mine", kind: "dark", syntax: { kw: "#FF79C6", str: "#F1FA8C" }, diff: { added: "#203020" }, italicComments: false };
const PURPLE = { id: "own-purple00", name: "Purple", kind: "dark", app: { sheet: "#2A1B3D", ink: "#EDE4F7" }, syntax: {} };
/** Material Theme Lighter's text on its field: 2.2:1, under the floor. */
const FAINT = { id: "own-faint000", name: "Faint", kind: "light", app: { sheet: "#FAFAFA", ink: "#AAAAAA" }, syntax: {} };
const appProps = (html: ReturnType<typeof root>, side: string) => [...html.props.keys()].filter((k) => k.endsWith(`-${side}`) && !k.startsWith("--hl-") && !/^--(diff-add|diff-del|add-word|del-word)-/.test(k));

// ---------- code, as ADR-0051 has it ----------

test("both code slots' variables are on the root after boot from what was kept", () => {
  const storage = memory({ [SLOT_KEY.light]: "github-light", [SLOT_KEY.dark]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const html = root();
  createThemes(() => storage, () => html);
  expect(html.props.get("--hl-kw-light")).toBe("#C3243A");
  expect(html.props.get("--hl-com-style-light")).toBe("italic");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  expect(html.props.get("--diff-add-dark")).toBe("#203020");
  expect(html.props.has("--diff-del-dark")).toBe(false);
  expect(html.props.get("--hl-com-style-dark")).toBe("normal");
  expect(html.props.get("--hl-fn-dark")).toBe("var(--ink)");
  for (const role of ROLES) {
    expect(html.props.has(`--hl-${role}-light`)).toBe(true);
    expect(html.props.has(`--hl-${role}-dark`)).toBe(true);
  }
});

test("a window that is all knobas's writes nothing, so the stylesheet's own colours draw", () => {
  const html = root();
  const storage = memory();
  const themes = createThemes(() => storage, () => html);
  expect([...html.props.keys()]).toEqual([]);
  expect(storage.data.has(PAINT_KEY)).toBe(false);
  expect(themes.slot("light")).toBe("knobas-light");
  themes.choose("dracula");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  themes.follow("dark");
  expect([...html.props.keys()]).toEqual([]);
});

test("a kept code slot naming a theme that is gone, or one of the other side, draws knobas's", () => {
  const themes = createThemes(() => memory({ [SLOT_KEY.light]: "own-gonegone", [SLOT_KEY.dark]: "github-light" }), () => root());
  expect(themes.drawn("light").id).toBe("knobas-light");
  expect(themes.drawn("dark").id).toBe("knobas-dark");
});

// ---------- four slots: code follows the window until set apart (3.6) ----------

test("with nothing kept, choosing Dracula for the window draws Dracula's code colours too", () => {
  const html = root();
  const themes = createThemes(() => memory(), () => html);
  themes.chooseApp("dracula");
  expect(themes.follows("dark")).toBe(true);
  expect(themes.drawn("dark").id).toBe("dracula");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  expect(html.props.get("--sheet-dark")).toBe("#282A36");
});

test("then choosing knobas Dark for code keeps its id, and the window stays Dracula", () => {
  const storage = memory();
  const html = root();
  const themes = createThemes(() => storage, () => html);
  themes.chooseApp("dracula");
  themes.choose("knobas-dark");
  expect(storage.data.get(SLOT_KEY.dark)).toBe("knobas-dark");
  expect(themes.appDrawn("dark").id).toBe("dracula");
  // knobas Dark's code under a Dracula window writes its own colours: a string is not Dracula's Passed.
  expect(html.props.get("--hl-str-dark")).toBe("#6FCF97");
});

test("Set apart keeps what is drawn and changes no property; Follow removes the key", () => {
  const storage = memory();
  const html = root();
  const themes = createThemes(() => storage, () => html);
  themes.chooseApp("dracula");
  const before = new Map(html.props);
  themes.setApart("dark");
  expect(storage.data.get(SLOT_KEY.dark)).toBe("dracula");
  expect(html.props).toEqual(before);
  themes.chooseApp("one-dark");
  expect(themes.drawn("dark").id).toBe("dracula");
  themes.follow("dark");
  expect(storage.data.has(SLOT_KEY.dark)).toBe(false);
  expect(themes.drawn("dark").id).toBe("one-dark");
});

test("a code theme chosen before this build is kept apart when the window's slot changes", () => {
  const themes = createThemes(() => memory({ [SLOT_KEY.dark]: "one-dark" }), () => root());
  themes.chooseApp("dracula");
  expect(themes.drawn("dark").id).toBe("one-dark");
});

test("a window theme with no code colours and a following code slot draws knobas's code colours", () => {
  const html = root();
  const themes = createThemes(() => memory({ [THEMES_KEY]: JSON.stringify([PURPLE]) }), () => html);
  themes.chooseApp(PURPLE.id);
  expect(themes.drawn("dark").id).toBe("knobas-dark");
  expect(html.props.get("--hl-kw-dark")).toBe("#C79BF2");
});

test("a theme of the other side is refused by a slot", () => {
  const themes = createThemes(() => memory(), () => root());
  themes.chooseApp("github-light");
  expect(themes.appDrawn("dark").id).toBe("knobas-dark");
  expect(themes.appDrawn("light").id).toBe("github-light");
});

test("deleting a theme both slots held lets both go, and its undo puts both back", () => {
  const OWN = { ...BUILTINS.find((t) => t.id === "dracula")!, id: "own-dracula0", name: "Dracula copy" };
  const storage = memory({ [APP_SLOT_KEY.dark]: OWN.id, [SLOT_KEY.dark]: OWN.id, [THEMES_KEY]: JSON.stringify([OWN]) });
  const html = root();
  const themes = createThemes(() => storage, () => html);
  expect(themes.appDrawn("dark").id).toBe(OWN.id);
  const undo = themes.remove(OWN.id)!;
  expect(themes.appDrawn("dark").id).toBe("knobas-dark");
  expect(themes.follows("dark")).toBe(true);
  expect(storage.data.has(APP_SLOT_KEY.dark)).toBe(false);
  expect([...html.props.keys()]).toEqual([]);
  undo();
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe(OWN.id);
  expect(storage.data.get(SLOT_KEY.dark)).toBe(OWN.id);
  expect(html.props.get("--sheet-dark")).toBe("#282A36");
});

// ---------- what is written (3.7) ----------

test("after a boot from kept values, both sides' window properties are on the root", () => {
  const storage = memory({ [APP_SLOT_KEY.light]: "github-light", [APP_SLOT_KEY.dark]: "dracula" });
  const html = root();
  createThemes(() => storage, () => html);
  expect(appProps(html, "light").length).toBeGreaterThanOrEqual(50);
  expect(appProps(html, "dark").length).toBeGreaterThanOrEqual(50);
  expect(html.props.get("--primary-light")).toBe("#006423");
  expect(html.attrs.get("data-sel")).toBe("light dark");
});

test("a window slot holding knobas's writes no window property, and going back from Dracula removes all of them", () => {
  const html = root();
  const themes = createThemes(() => memory(), () => html);
  themes.chooseApp("dracula");
  const dracula = derive("dark", BUILTINS.find((t) => t.id === "dracula")!.app).colours;
  for (const [key, token] of APP_KEYS) expect(html.props.get(`${token}-dark`), key).toBe(dracula[key]);
  themes.chooseApp("knobas-dark");
  expect(appProps(html, "dark")).toEqual([]);
  expect(html.attrs.has("data-sel")).toBe(false);
});

test("ten edits in one frame apply once, with the last value, and data-paint moves only on the save", () => {
  vi.useFakeTimers();
  try {
    const OWN = { ...PURPLE, id: "own-purple01" };
    const storage = memory({ [APP_SLOT_KEY.dark]: OWN.id, [THEMES_KEY]: JSON.stringify([OWN]) });
    const html = root();
    const timers = { ...frames(), set: setTimeout, clear: clearTimeout };
    const themes = createThemes(() => storage, () => html, timers);
    const paint = html.attrs.get("data-paint");
    for (let i = 0; i < 10; i += 1) themes.update(OWN.id, (t) => ({ ...t, app: { ...t.app, ink: `#EDE4F${i}` } }));
    expect(html.props.get("--ink-dark")).toBe("#EDE4F7");
    expect(timers.run()).toBe(1);
    expect(html.props.get("--ink-dark")).toBe("#EDE4F9");
    expect(html.attrs.get("data-paint")).toBe(paint);
    vi.advanceTimersByTime(SAVE_AFTER);
    expect(html.attrs.get("data-paint")).not.toBe(paint);
    expect(storage.data.get(THEMES_KEY)).toContain("#EDE4F9");
  } finally {
    vi.useRealTimers();
  }
});

test("the paint cache equals what is on the root after every apply, and goes when the window is all knobas's", () => {
  const storage = memory();
  const html = root();
  const themes = createThemes(() => storage, () => html);
  const cache = () => JSON.parse(storage.data.get(PAINT_KEY)!) as { v: number; vars: Record<string, string>; sel?: string };
  themes.chooseApp("dracula");
  expect(cache().v).toBe(1);
  expect(cache().vars).toEqual(Object.fromEntries(html.props));
  expect(cache().sel).toBe("dark");
  themes.choose("one-dark");
  expect(cache().vars).toEqual(Object.fromEntries(html.props));
  themes.chooseApp("knobas-dark");
  themes.follow("dark");
  expect(storage.data.has(PAINT_KEY)).toBe(false);
});

test("a scope draws a theme by plain token names, whatever the window wears", () => {
  const dracula = BUILTINS.find((t) => t.id === "dracula")!;
  const scope = scopeValues(dracula, null);
  expect(scope["--sheet"]).toBe("#282A36");
  expect(scope["--hl-kw"]).toBe("#FF79C6");
  expect(scope["--diff-add"]).toBe(derive("dark", dracula.app).colours.okBg);
  expect(scope["color-scheme"]).toBe("dark");
  const knobasDark = BUILTINS.find((t) => t.id === "knobas-dark")!;
  expect(scopeValues(knobasDark, dracula)["--paper"]).toBe("#12171C");
  expect(scopeValues(knobasDark, dracula)["--hl-kw"]).toBe("#FF79C6");
});

// ---------- the floor at start (3.9) ----------

test("a boot with an unreadable, unkept theme in a slot draws knobas's for that side, keeps the value, and says so once", () => {
  const storage = memory({ [APP_SLOT_KEY.light]: FAINT.id, [THEMES_KEY]: JSON.stringify([FAINT]) });
  const html = root();
  const told: string[] = [];
  const themes = createThemes(() => storage, () => html, undefined, { onUnreadable: (t, kind) => told.push(`${t.name} ${kind}`) });
  expect(told).toEqual(["Faint light"]);
  expect(themes.appDrawn("light").id).toBe("knobas-light");
  expect(appProps(html, "light")).toEqual([]);
  expect(storage.data.get(APP_SLOT_KEY.light)).toBe(FAINT.id);
  themes.keepAsIs(FAINT.id);
  expect(storage.data.get(ASIS_KEY)).toBe(JSON.stringify([FAINT.id]));
  expect(themes.appDrawn("light").id).toBe(FAINT.id);
});

test("started with knobas's own themes, a boot writes nothing and leaves what is kept", () => {
  const storage = memory({ [APP_SLOT_KEY.dark]: "dracula", [SLOT_KEY.dark]: "one-dark", [PAINT_KEY]: '{"v":1,"vars":{"--sheet-dark":"#282A36"}}' });
  const html = root();
  const themes = createThemes(() => storage, () => html, undefined, { defaultTheme: true });
  expect([...html.props.keys()]).toEqual([]);
  expect(themes.appDrawn("dark").id).toBe("knobas-dark");
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe("dracula");
  expect(storage.data.get(SLOT_KEY.dark)).toBe("one-dark");
  expect(storage.data.get(PAINT_KEY)).toContain("#282A36");
});

test("putting knobas's themes back fills both window slots with knobas's and lets code follow; its undo writes all four back", () => {
  const storage = memory({ [APP_SLOT_KEY.dark]: "dracula", [APP_SLOT_KEY.light]: "github-light", [SLOT_KEY.dark]: "one-dark" });
  const html = root();
  const themes = createThemes(() => storage, () => html);
  const undo = themes.putKnobasBack();
  for (const key of [APP_SLOT_KEY.dark, APP_SLOT_KEY.light, SLOT_KEY.dark]) expect(storage.data.has(key)).toBe(false);
  expect([...html.props.keys()]).toEqual([]);
  undo();
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe("dracula");
  expect(storage.data.get(SLOT_KEY.dark)).toBe("one-dark");
  expect(storage.data.get(APP_SLOT_KEY.light)).toBe("github-light");
});

// ---------- a person's own ----------

test("a copy gets a unique name and an own id, and keeps both parts", () => {
  const themes = createThemes(() => memory(), () => root());
  const copy = themes.add({ name: "One Dark", kind: "dark", app: { sheet: "#282C34" }, syntax: { kw: "#C678DD" }, from: "One Dark" })!;
  expect(copy.name).toBe("One Dark 2");
  expect(copy.id).toMatch(/^own-[0-9a-z]{8}$/);
  expect(copy.app).toEqual({ sheet: "#282C34" });
  expect(themes.of("dark").at(-1)).toEqual(copy);
  expect(themes.appOf("dark").at(-1)).toEqual(copy);
  expect(themes.of("light").some((t) => t.id === copy.id)).toBe(false);
});

test("unchanged lines start quiet; a choice is kept and written on <html>", () => {
  const storage = memory();
  const html = root();
  const themes = createThemes(() => storage, () => html);
  expect(html.attrs.get("data-diff-unchanged")).toBe("quiet");
  themes.setUnchanged("grey");
  expect(html.attrs.get("data-diff-unchanged")).toBe("grey");
  expect(storage.data.get(UNCHANGED_KEY)).toBe("grey");
  expect(createThemes(() => storage, () => root()).unchanged).toBe("grey");
});

test("the window's side is kept and written on <html> before anything mounts", () => {
  const storage = memory();
  const attrs = new Map<string, string>();
  const html = { setAttribute: (k: string, v: string) => void attrs.set(k, v), removeAttribute: (k: string) => void attrs.delete(k) };
  const mode = createTheme(() => storage, () => html);
  expect(mode.choice).toBe("system");
  mode.set("dark");
  expect(storage.data.get(MODE_KEY)).toBe("dark");
  expect(attrs.get("data-theme")).toBe("dark");
  attrs.clear();
  const again = createTheme(() => storage, () => html);
  expect(again.choice).toBe("dark");
  again.set("system");
  expect(storage.data.has(MODE_KEY)).toBe(false);
  expect(attrs.has("data-theme")).toBe(false);
});
