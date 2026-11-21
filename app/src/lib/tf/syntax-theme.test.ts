/** The two slots' themes on <html>, a person's own, and what is kept (ADR-0051). */
import { expect, test, vi } from "vitest";

import { ROLES } from "../render/roles";
import type { KeptStorage } from "./kept-value";
import { createSyntaxThemes, SAVE_AFTER, SLOT_KEY, THEMES_KEY, UNCHANGED_KEY, type ThemeRoot } from "./syntax-theme.svelte";
import { createTheme, MODE_KEY } from "./theme.svelte";

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
  };
}

const MINE = { id: "own-abcdefgh", name: "Mine", kind: "dark", syntax: { kw: "#FF79C6", str: "#F1FA8C" }, diff: { added: "#203020" }, italicComments: false };

test("both slots' variables are on the root after boot from what was kept", () => {
  const storage = memory({ [SLOT_KEY.light]: "github-light", [SLOT_KEY.dark]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const html = root();
  createSyntaxThemes(() => storage, () => html);
  expect(html.props.get("--hl-kw-light")).toBe("#C3243A");
  expect(html.props.get("--hl-com-style-light")).toBe("italic");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  expect(html.props.get("--hl-str-dark")).toBe("#F1FA8C");
  expect(html.props.get("--diff-add-dark")).toBe("#203020");
  expect(html.props.has("--diff-del-dark")).toBe(false);
  expect(html.props.get("--hl-com-style-dark")).toBe("normal");
  for (const role of ROLES) {
    expect(html.props.has(`--hl-${role}-light`)).toBe(true);
    expect(html.props.has(`--hl-${role}-dark`)).toBe(true);
  }
});

test("a role a theme leaves out is ink", () => {
  const storage = memory({ [SLOT_KEY.dark]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const html = root();
  createSyntaxThemes(() => storage, () => html);
  expect(html.props.get("--hl-fn-dark")).toBe("var(--ink)");
});

test("the knobas pair writes nothing, so the stylesheet's own colours draw", () => {
  const html = root();
  const themes = createSyntaxThemes(() => memory(), () => html);
  expect([...html.props.keys()]).toEqual([]);
  expect(themes.slot("light")).toBe("knobas-light");
  themes.choose("dracula");
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
  themes.choose("knobas-dark");
  expect([...html.props.keys()]).toEqual([]);
});

test("choosing a theme writes the kept value of its own kind's slot, and the other slot's variables stay", () => {
  const storage = memory();
  const html = root();
  const themes = createSyntaxThemes(() => storage, () => html);
  themes.choose("github-light");
  themes.choose("one-dark");
  expect(storage.data.get(SLOT_KEY.light)).toBe("github-light");
  expect(storage.data.get(SLOT_KEY.dark)).toBe("one-dark");
  // Both are on the root, so the system changing sides needs no script.
  expect(html.props.get("--hl-kw-light")).toBe("#C3243A");
  expect(html.props.get("--hl-kw-dark")).toBe("#C678DD");
});

test("a deleted theme's slot goes back to knobas's, and its undo puts both back", () => {
  const storage = memory({ [SLOT_KEY.dark]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
  const html = root();
  const themes = createSyntaxThemes(() => storage, () => html);
  const undo = themes.remove(MINE.id)!;
  expect(themes.slot("dark")).toBe("knobas-dark");
  expect(storage.data.has(SLOT_KEY.dark)).toBe(false);
  expect(storage.data.has(THEMES_KEY)).toBe(false);
  expect(html.props.has("--hl-kw-dark")).toBe(false);
  undo();
  expect(themes.slot("dark")).toBe(MINE.id);
  expect(themes.own.map((t) => t.id)).toEqual([MINE.id]);
  expect(html.props.get("--hl-kw-dark")).toBe("#FF79C6");
});

test("a kept slot naming a theme that is gone, or one of the other kind, draws knobas's", () => {
  const themes = createSyntaxThemes(() => memory({ [SLOT_KEY.light]: "own-gonegone", [SLOT_KEY.dark]: "github-light" }), () => root());
  expect(themes.drawn("light").id).toBe("knobas-light");
  expect(themes.drawn("dark").id).toBe("knobas-dark");
});

test("an edit draws at once and is kept 300 ms after the last change", () => {
  vi.useFakeTimers();
  try {
    const storage = memory({ [SLOT_KEY.dark]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) });
    const html = root();
    const themes = createSyntaxThemes(() => storage, () => html);
    themes.update(MINE.id, (t) => ({ ...t, syntax: { ...t.syntax, kw: "#00FF00" } }));
    expect(html.props.get("--hl-kw-dark")).toBe("#00FF00");
    vi.advanceTimersByTime(SAVE_AFTER - 1);
    expect(storage.data.get(THEMES_KEY)).not.toContain("#00FF00");
    themes.update(MINE.id, (t) => ({ ...t, syntax: { ...t.syntax, kw: "#0000FF" } }));
    vi.advanceTimersByTime(SAVE_AFTER);
    expect(storage.data.get(THEMES_KEY)).toContain("#0000FF");
  } finally {
    vi.useRealTimers();
  }
});

test("a copy gets a unique name and an own id", () => {
  const themes = createSyntaxThemes(() => memory(), () => root());
  const copy = themes.add({ name: "One Dark", kind: "dark", syntax: { kw: "#C678DD" }, from: "One Dark" })!;
  expect(copy.name).toBe("One Dark 2");
  expect(copy.id).toMatch(/^own-[0-9a-z]{8}$/);
  expect(themes.of("dark").at(-1)).toEqual(copy);
  expect(themes.of("light").some((t) => t.id === copy.id)).toBe(false);
});

test("unchanged lines start quiet; a choice is kept and written on <html>", () => {
  const storage = memory();
  const html = root();
  const themes = createSyntaxThemes(() => storage, () => html);
  expect(html.attrs.get("data-diff-unchanged")).toBe("quiet");
  themes.setUnchanged("grey");
  expect(html.attrs.get("data-diff-unchanged")).toBe("grey");
  expect(storage.data.get(UNCHANGED_KEY)).toBe("grey");
  expect(createSyntaxThemes(() => storage, () => root()).unchanged).toBe("grey");
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
  expect(attrs.get("data-theme")).toBe("dark");
  again.set("system");
  expect(storage.data.has(MODE_KEY)).toBe(false);
  expect(attrs.has("data-theme")).toBe(false);
});
