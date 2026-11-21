/**
 * A VS Code theme's `colors`, a Zed theme's `style` and a Base16 scheme read onto the window's colours (ADR-0055;
 * the decisions' 3.10 and section 5). For each of knobas's colours the first key present wins; a colour with alpha is
 * laid over what it would lie on; a guard refuses a colour that cannot play its part here (a page on the other side
 * of the panel, a border louder than 2.2:1, a button that does not stand off the panel), and the next key is tried.
 * After the last key the colour is derived (`app-derive.ts`). The card and a diff's colours are never imported.
 */
import { type AppColours, KNOBAS } from "./app-derive";
import type { AppKey } from "./app-roles";
import { contrast, luminance, over, parse } from "./colour";
import type { Role } from "../../render/roles";
import type { ThemeKind } from "./themes";

type Guard = (value: string, set: AppColours) => boolean;
/** One of knobas's colours: the keys it is read from, what a translucent one is laid over, and its guard. */
export interface MapRow {
  key: AppKey;
  from: readonly string[];
  on?: AppKey;
  guard?: Guard;
  /** Read only where this key was read: a button's words where the button was. */
  needs?: AppKey;
}

const pageSide: Guard = (v, c) => contrast(v, c.sheet!) <= 1.45;
const insetGuard = (most: number): Guard => (v, c) => contrast(v, c.sheet!) >= 1.04 && contrast(v, c.sheet!) <= most;
/** Something drawn behind text in ink. */
const behindInk: Guard = (v, c) => contrast(v, c.sheet!) >= 1.1 && contrast(v, c.ink!) >= 4.5;

/** A VS Code theme's `colors`, section 5's first table. */
export const VSCODE: readonly MapRow[] = [
  { key: "sheet", from: ["editor.background"] },
  { key: "ink", from: ["editor.foreground", "foreground"] },
  // The page is the panel's own side: a light theme's dark side bar is not a page text is read on.
  { key: "paper", from: ["sideBar.background", "activityBar.background", "editorGroupHeader.tabsBackground", "titleBar.activeBackground"], on: "sheet", guard: pageSide },
  { key: "sheet2", from: ["list.hoverBackground", "editor.lineHighlightBackground", "editorWidget.background"], on: "sheet", guard: insetGuard(1.45) },
  { key: "field", from: ["input.background", "dropdown.background"], on: "sheet", guard: pageSide },
  // Secondary text is read everywhere: one too faint to read is derived instead.
  { key: "ink2", from: ["descriptionForeground", "sideBar.foreground"], on: "sheet", guard: (v, c) => contrast(v, c.ink!) >= 1.15 && contrast(v, c.sheet!) >= 4.5 && contrast(v, c.paper ?? c.sheet!) >= 4.5 },
  // A border is quiet: an accent drawn as a border in VS Code (Dracula's purple) is not every border in knobas.
  {
    key: "rule",
    from: ["editorGroup.border", "panel.border", "sideBar.border", "input.border", "contrastBorder"],
    on: "sheet",
    guard: (v, c) => {
      const r = Math.max(contrast(v, c.sheet!), contrast(v, c.paper ?? c.sheet!));
      return r >= 1.15 && r <= 2.2;
    },
  },
  // The primary button is the theme's own where it stands off the panel; what is selected or pressed stays the ink's fill.
  { key: "primary", from: ["button.background"], on: "sheet", guard: (v, c) => contrast(v, c.sheet!) >= 3 && contrast(v, c.paper ?? c.sheet!) >= 3 },
  { key: "onPrimary", from: ["button.foreground"], on: "primary", needs: "primary" },
  { key: "focus", from: ["focusBorder"], on: "sheet", guard: (v, c) => contrast(v, c.sheet!) >= 1.5 },
  { key: "link", from: ["textLink.foreground"], on: "sheet" },
  { key: "mark", from: ["editor.findMatchHighlightBackground", "editor.findMatchBackground"], on: "sheet", guard: behindInk },
  { key: "selection", from: ["editor.selectionBackground", "selection.background"], on: "sheet", guard: behindInk },
  // The states: the terminal's four first, the one place an editor theme names a green, a yellow, a red and a blue as
  // text on its own background, chosen together. Then the keys it writes for words, a squiggle's colour last.
  { key: "ok", from: ["terminal.ansiGreen", "gitDecoration.addedResourceForeground", "gitDecoration.untrackedResourceForeground", "editorGutter.addedBackground"], on: "sheet" },
  { key: "warn", from: ["terminal.ansiYellow", "list.warningForeground", "editorWarning.foreground"], on: "sheet" },
  { key: "err", from: ["terminal.ansiRed", "errorForeground", "list.errorForeground", "editorError.foreground"], on: "sheet" },
  { key: "info", from: ["terminal.ansiBlue", "editorInfo.foreground", "textLink.foreground"], on: "sheet" },
];

/**
 * A Zed theme's `style`, section 5's second table, with the guards of the spike's `ZED` (its file is the ruling where the
 * two differ). A Zed theme has no button and no field colour, so those are derived.
 */
export const ZED: readonly MapRow[] = [
  { key: "sheet", from: ["editor.background", "background"] },
  { key: "ink", from: ["editor.foreground", "text"] },
  { key: "paper", from: ["panel.background", "surface.background", "background"], on: "sheet" },
  { key: "sheet2", from: ["ghost_element.hover", "element.hover", "editor.active_line.background"], on: "sheet", guard: insetGuard(1.5) },
  { key: "ink2", from: ["text.muted"], on: "sheet", guard: (v, c) => contrast(v, c.ink!) >= 1.15 },
  { key: "rule", from: ["border"], on: "sheet", guard: (v, c) => Math.max(contrast(v, c.sheet!), contrast(v, c.paper ?? c.sheet!)) >= 1.15 },
  { key: "rule2", from: ["border.variant"], on: "sheet" },
  { key: "focus", from: ["border.focused", "text.accent"], on: "sheet", guard: (v, c) => contrast(v, c.sheet!) >= 1.5 },
  { key: "link", from: ["link_text.hover", "text.accent"], on: "sheet" },
  { key: "mark", from: ["search.match_background"], on: "sheet", guard: (v, c) => contrast(v, c.sheet!) >= 1.1 },
  { key: "selection", from: ["element.selected"], on: "sheet", guard: (v, c) => contrast(v, c.sheet!) >= 1.1 },
  { key: "ok", from: ["success", "created"], on: "sheet" },
  { key: "warn", from: ["warning", "modified"], on: "sheet" },
  { key: "err", from: ["error", "deleted"], on: "sheet" },
  { key: "info", from: ["info"], on: "sheet" },
];

/** `colors` read by `table`: the set colours, and for each the key that gave it. */
function read(table: readonly MapRow[], colors: Record<string, unknown>, kind: ThemeKind): { set: AppColours; from: Partial<Record<AppKey, string>> } {
  const set: AppColours = {};
  const from: Partial<Record<AppKey, string>> = {};
  for (const row of table) {
    if (row.needs && !set[row.needs]) continue;
    for (const name of row.from) {
      const under = row.on ? (set[row.on] ?? KNOBAS[kind][row.on as Exclude<AppKey, "selection">]) : "#000000";
      const v = over(colors[name], under);
      if (v === null) continue;
      if (row.guard && !row.guard(v, { ...set, sheet: set.sheet ?? KNOBAS[kind].sheet, ink: set.ink ?? KNOBAS[kind].ink })) continue;
      set[row.key] = v;
      from[row.key] = name;
      break;
    }
  }
  return { set, from };
}

/** A VS Code theme's side: its `type`, else the panel's luminance, else its name. A `type` the background contradicts gives way. */
export function kindOfVsCode(json: Record<string, unknown>): ThemeKind {
  const type = typeof json.type === "string" ? json.type.toLowerCase() : "";
  const colors = (json.colors && typeof json.colors === "object" ? json.colors : {}) as Record<string, unknown>;
  const bg = parse(colors["editor.background"]);
  const byBg: ThemeKind | null = bg ? (luminance(bg[0]) > 0.4 ? "light" : "dark") : null;
  let kind: ThemeKind | null = null;
  if (type === "light" || type === "hc-light") kind = "light";
  else if (type === "dark" || type === "hc" || type === "hc-black") kind = "dark";
  if (kind && byBg && kind !== byBg) return byBg;
  if (kind) return kind;
  if (byBg) return byBg;
  return /\blight\b/i.test(String(json.name ?? "")) ? "light" : "dark";
}

/** A VS Code theme's `colors` onto the window's colours. */
export function appFromVsCode(json: Record<string, unknown>): { kind: ThemeKind; set: AppColours; from: Partial<Record<AppKey, string>> } {
  const colors = (json.colors && typeof json.colors === "object" ? json.colors : {}) as Record<string, unknown>;
  const kind = kindOfVsCode(json);
  return { kind, ...read(VSCODE, colors, kind) };
}

/** A Zed theme's `style` onto the window's colours (its `syntax` is read by `fromZed` in `themes.ts`). */
export function appFromZed(style: Record<string, unknown>, kind: ThemeKind): { set: AppColours; from: Partial<Record<AppKey, string>> } {
  return read(ZED, style, kind);
}

export const BASE16_KEYS = ["base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08", "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F"] as const;
export type Base16 = Record<(typeof BASE16_KEYS)[number], string>;

/** A Base16 (or Base24) scheme onto both parts, section 5's third table. The side is `base00`'s luminance. */
export function fromBase16(b: Base16): { kind: ThemeKind; set: AppColours; syntax: Partial<Record<Role, string>> } {
  const kind: ThemeKind = luminance(b.base00) > 0.4 ? "light" : "dark";
  const set: AppColours = { sheet: b.base00, sheet2: b.base01, selection: b.base02, ink: b.base05, err: b.base08, warn: b.base0A, ok: b.base0B, info: b.base0D, link: b.base0D, focus: b.base0D };
  // A selection the text cannot be read on is left to the system, as the VS Code guard does.
  if (!behindInk(b.base02, set)) delete set.selection;
  const syntax: Partial<Record<Role, string>> = {
    com: b.base03,
    kw: b.base0E,
    str: b.base0B,
    esc: b.base0C,
    num: b.base09,
    con: b.base09,
    fn: b.base0D,
    ty: b.base0A,
    key: b.base0D,
    tag: b.base08,
    attr: b.base09,
    var: b.base08,
    op: b.base05,
    punct: b.base05,
    head: b.base0D,
    link: b.base0C,
  };
  return { kind, set, syntax };
}

/**
 * A Base16 scheme's text, YAML or JSON, read with a line pattern: sixteen `base00` to `base0F` with a colour each, with
 * or without `#`, quoted or not, at the top or under `palette:`. `null` where any of the sixteen is missing.
 */
export function readBase16(text: string): { name: string; base: Base16 } | null {
  const base: Partial<Base16> = {};
  for (const m of text.matchAll(/["']?(base0[0-9A-Fa-f])["']?\s*:\s*["']?#?([0-9A-Fa-f]{6})\b/g)) {
    const key = `base0${m[1]!.slice(5).toUpperCase()}` as keyof Base16;
    if (base[key] === undefined) base[key] = `#${m[2]!.toUpperCase()}`;
  }
  if (BASE16_KEYS.some((k) => base[k] === undefined)) return null;
  const name = /(?:^|\n)\s*["']?(?:scheme|name)["']?\s*:\s*["']?([^"'\n,]+?)["']?\s*,?\s*(?:\n|$)/.exec(text)?.[1]?.trim() ?? "Base16";
  return { name, base: base as Base16 };
}
