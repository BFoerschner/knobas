/**
 * A theme (ADR-0051, ADR-0055): a name, a side, and colours in two parts. Its window part (`app`) sets any of the
 * window's 51 colours (`app-roles.ts`), the rest derived (`app-derive.ts`); its code part sets a colour for some of
 * the roles (`render/roles.ts`), a role it leaves out drawing in ink, and may set the diff's four colours and whether
 * comments slant. A theme may hold either part or both.
 *
 * This file is the model: what a theme may hold, reading one a person kept, and reading and writing theme files.
 * Import reads knobas's own file, a VS Code theme, a Zed theme, a Base16 scheme and a JetBrains scheme (code only)
 * onto the roles; export writes knobas's own. Nothing here touches the page or storage (`tf/themes.svelte.ts` does).
 */
import { PIERRE_ROLE, ROLES, type Role } from "../../render/roles";
import { appFromVsCode, appFromZed, fromBase16, kindOfVsCode, readBase16 } from "./app-import";
import { type AppKey, KEYS } from "./app-roles";
import { luminance } from "./colour";

export type ThemeKind = "light" | "dark";

/** The diff's own colours: the added and removed lines' tints and the changed words' marks. */
export interface DiffColours {
  added?: string;
  removed?: string;
  addedWord?: string;
  removedWord?: string;
}
export const DIFF_KEYS = ["added", "removed", "addedWord", "removedWord"] as const;
export type DiffKey = (typeof DIFF_KEYS)[number];

export interface Theme {
  /** `knobas-light` and the other built-ins, or `own-` and 8 base-36 characters for a person's own. */
  id: string;
  /** 1 to 60 characters. */
  name: string;
  kind: ThemeKind;
  /** The name it was copied or imported from, for the editor's line. */
  from?: string;
  /** The window's colours it sets, `#RRGGBB` a key; absent where the theme has no window colours. */
  app?: Partial<Record<AppKey, string>>;
  /** Colours its file had that knobas offers and does not set: a selected row as the editor draws it. */
  offers?: Partial<Record<AppKey, string>>;
  /** `#RRGGBB` a role; a role left out draws in ink. Empty where the theme has no code colours. */
  syntax: Partial<Record<Role, string>>;
  diff?: DiffColours;
  /** Comments slant unless this is `false`. */
  italicComments?: boolean;
}

/** Whether a theme holds window colours. */
export const hasApp = (t: Pick<Theme, "app">): boolean => t.app !== undefined;
/** Whether a theme holds code colours. */
export const hasCode = (t: Pick<Theme, "syntax">): boolean => Object.keys(t.syntax).length > 0;

/** The most own themes a person keeps. */
export const MOST_THEMES = 64;
/** The largest theme file read, in bytes. */
export const LARGEST_FILE = 1024 * 1024;
export const LONGEST_NAME = 60;

export const REFUSED = "That file is not a theme knobas can read. It has no `knobasTheme`, `colors`, `tokenColors`, `style`, Base16 colours or JetBrains `<scheme>`.";

/** A colour a theme may hold, as `#RRGGBB` upper case, or `null`: `#RGB`, `#RRGGBB` and `#RRGGBBAA` are read, alpha dropped. */
export function colour(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const v = value.trim();
  let m = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(v);
  if (m) return `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}`.toUpperCase();
  m = /^#([0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(v);
  return m ? `#${m[1]!.toUpperCase()}` : null;
}

/** A fresh id for a person's own theme. */
export function ownId(random: () => number = Math.random): string {
  let id = "own-";
  for (let i = 0; i < 8; i += 1) id += Math.floor(random() * 36).toString(36);
  return id;
}

/** A name cut to what a theme may hold; an empty one is *Untitled*. */
export function themeName(value: unknown): string {
  const name = typeof value === "string" ? value.trim().slice(0, LONGEST_NAME) : "";
  return name === "" ? "Untitled" : name;
}

/** `name`, or `name 2`, `name 3`… where `taken` holds it already. */
export function uniqueName(name: string, taken: Iterable<string>): string {
  const names = new Set(taken);
  if (!names.has(name)) return name;
  for (let n = 2; ; n += 1) {
    const next = `${name.slice(0, LONGEST_NAME - String(n).length - 1)} ${n}`;
    if (!names.has(next)) return next;
  }
}

function syntaxOf(value: unknown): Partial<Record<Role, string>> {
  const out: Partial<Record<Role, string>> = {};
  if (value === null || typeof value !== "object") return out;
  for (const role of ROLES) {
    const c = colour((value as Record<string, unknown>)[role]);
    if (c !== null) out[role] = c;
  }
  return out;
}

/** A theme's window colours read key by key: a key it does not know, or a colour it cannot read, is dropped. */
export function appOf(value: unknown): Partial<Record<AppKey, string>> | undefined {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return undefined;
  const out: Partial<Record<AppKey, string>> = {};
  for (const key of KEYS) {
    const c = colour((value as Record<string, unknown>)[key]);
    if (c !== null) out[key] = c;
  }
  return out;
}

function diffOf(value: unknown): DiffColours | undefined {
  if (value === null || typeof value !== "object") return undefined;
  const out: DiffColours = {};
  for (const key of DIFF_KEYS) {
    const c = colour((value as Record<string, unknown>)[key]);
    if (c !== null) out[key] = c;
  }
  return Object.keys(out).length > 0 ? out : undefined;
}

/** A kept theme read back, or `null` where it is not one (a bad colour is dropped, not the theme). */
export function readTheme(value: unknown): Theme | null {
  if (value === null || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (typeof v.id !== "string" || !/^own-[0-9a-z]{8}$/.test(v.id)) return null;
  if (v.kind !== "light" && v.kind !== "dark") return null;
  const theme: Theme = { id: v.id, name: themeName(v.name), kind: v.kind, syntax: syntaxOf(v.syntax) };
  if (typeof v.from === "string" && v.from !== "") theme.from = v.from.slice(0, LONGEST_NAME);
  const app = appOf(v.app);
  if (app) theme.app = app;
  const offers = appOf(v.offers);
  if (offers && Object.keys(offers).length > 0) theme.offers = offers;
  const diff = diffOf(v.diff);
  if (diff) theme.diff = diff;
  if (v.italicComments === false) theme.italicComments = false;
  return theme;
}

/** A person's themes as kept (`knobas.themes`, a JSON array), each read; anything unreadable is left out. */
export function readThemes(kept: string | null): Theme[] {
  if (kept === null) return [];
  try {
    const list: unknown = JSON.parse(kept);
    if (!Array.isArray(list)) return [];
    const seen = new Set<string>();
    const out: Theme[] = [];
    for (const item of list) {
      const theme = readTheme(item);
      if (theme && !seen.has(theme.id) && out.length < MOST_THEMES) {
        seen.add(theme.id);
        out.push(theme);
      }
    }
    return out;
  } catch {
    return [];
  }
}

// ---------- export ----------

/** knobas's own theme file. */
export interface ThemeFile {
  knobasTheme: 1;
  name: string;
  kind: ThemeKind;
  /** The window's colours, where the theme has them (ADR-0055). A build from before ignores the key. */
  app?: Partial<Record<AppKey, string>>;
  /** What its source file offered and knobas did not set. */
  offers?: Partial<Record<AppKey, string>>;
  syntax: Partial<Record<Role, string>>;
  diff?: DiffColours;
  italicComments: boolean;
}

/** `theme` as knobas's file: its name, `<name>.knobas-theme.json`, and its text. */
export function exportTheme(theme: Theme): { fileName: string; text: string } {
  const file: ThemeFile = { knobasTheme: 1, name: theme.name, kind: theme.kind, ...(theme.app ? { app: { ...theme.app } } : {}), ...(theme.offers ? { offers: { ...theme.offers } } : {}), syntax: { ...theme.syntax }, italicComments: theme.italicComments !== false };
  if (theme.diff) file.diff = { ...theme.diff };
  const safe = theme.name.replace(/[\\/:*?"<>|]+/g, " ").trim() || "theme";
  return { fileName: `${safe}.knobas-theme.json`, text: `${JSON.stringify(file, null, 2)}\n` };
}

// ---------- import ----------

/** A theme read from a file: everything but the id, which the store gives it. */
export type ImportedTheme = Omit<Theme, "id">;

/** The scopes a VS Code theme is asked for each role, first match winning. */
export const PROBES: Record<Role, readonly string[]> = {
  com: ["comment"],
  kw: ["keyword", "storage.type", "storage.modifier", "keyword.control"],
  str: ["string"],
  esc: ["constant.character.escape", "string.regexp"],
  num: ["constant.numeric"],
  con: ["constant.language", "variable.other.constant", "constant"],
  fn: ["entity.name.function", "support.function"],
  ty: ["entity.name.type", "entity.name.class", "support.type", "support.class"],
  prop: ["variable.other.property", "variable.other.object.property", "meta.object-literal.key"],
  key: ["support.type.property-name", "entity.name.tag.yaml", "meta.object-literal.key"],
  tag: ["entity.name.tag"],
  attr: ["entity.other.attribute-name", "storage.type.annotation", "meta.decorator"],
  var: ["variable.parameter", "variable"],
  op: ["keyword.operator"],
  punct: ["punctuation", "meta.brace"],
  head: ["markup.heading", "entity.name.section"],
  link: ["markup.underline.link", "string.other.link"],
};

interface VsCodeRule {
  scope?: string | string[];
  settings?: { foreground?: string; fontStyle?: string };
}

/** The rule of a VS Code theme that colours `scope`: the longest selector it starts with, a later rule winning a tie. */
export function ruleFor(rules: readonly VsCodeRule[], scope: string, want: "foreground" | "fontStyle" = "foreground"): string | null {
  let best: string | null = null;
  let depth = -1;
  for (const rule of rules) {
    const value = rule.settings?.[want];
    if (typeof value !== "string" || !rule.scope) continue;
    const selectors = (Array.isArray(rule.scope) ? rule.scope : String(rule.scope).split(",")).map((s) => s.trim());
    for (const selector of selectors) {
      // A selector on a parent scope is not a role's own.
      if (selector === "" || selector.includes(" ")) continue;
      if (scope === selector || scope.startsWith(`${selector}.`)) {
        const d = selector.split(".").length;
        if (d >= depth) {
          depth = d;
          best = value;
        }
      }
    }
  }
  return best;
}


/** A VS Code theme: `tokenColors` onto the roles, `colors` onto the window's colours (`app-import.ts`). A file with one of the two gives a theme with one part. */
export function fromVsCode(json: Record<string, unknown>): ImportedTheme {
  const rules = (Array.isArray(json.tokenColors) ? json.tokenColors : []) as VsCodeRule[];
  const syntax: Partial<Record<Role, string>> = {};
  for (const role of ROLES)
    for (const probe of PROBES[role]) {
      const c = colour(ruleFor(rules, probe));
      if (c !== null) {
        syntax[role] = c;
        break;
      }
    }
  const name = themeName(json.name);
  // Its `type`, else the panel's luminance, else its name; a `type` the background contradicts gives way.
  const kind = kindOfVsCode(json);
  const theme: ImportedTheme = { name, kind, syntax, from: name };
  const colors = json.colors;
  if (colors !== null && typeof colors === "object" && !Array.isArray(colors) && Object.keys(colors).length > 0) {
    const read = appFromVsCode(json);
    theme.app = read.set;
    if (Object.keys(read.offers).length > 0) theme.offers = read.offers;
  }
  const slant = ruleFor(rules, "comment", "fontStyle");
  if (slant !== null && !/italic/.test(slant)) theme.italicComments = false;
  return theme;
}

/** A Zed theme family onto the roles: each of its themes, a syntax name by Pierre's table, the shortest name of a role winning. */
export function fromZed(json: Record<string, unknown>): ImportedTheme[] {
  const themes = Array.isArray(json.themes) ? json.themes : [];
  const out: ImportedTheme[] = [];
  for (const item of themes as Record<string, unknown>[]) {
    const style = item?.style as Record<string, unknown> | undefined;
    if (!style || typeof style !== "object") continue;
    const syntaxStyles = (style.syntax ?? {}) as Record<string, { color?: string; font_style?: string } | undefined>;
    const syntax: Partial<Record<Role, string>> = {};
    const nameLength: Partial<Record<Role, number>> = {};
    let italic: boolean | undefined;
    for (const [zedName, value] of Object.entries(syntaxStyles)) {
      const role = PIERRE_ROLE.get(zedName.replace(/\./g, "-"));
      if (role === undefined || role === "" || role === "strong" || role === "emph") continue;
      const c = colour(value?.color);
      if (c === null) continue;
      if (nameLength[role] === undefined || zedName.length < nameLength[role]) {
        nameLength[role] = zedName.length;
        syntax[role] = c;
        if (role === "com") italic = value?.font_style === "italic";
      }
    }
    const name = themeName(item.name);
    const kind: ThemeKind = item.appearance === "light" ? "light" : "dark";
    const theme: ImportedTheme = { name, kind, syntax, from: name };
    if (style && typeof style === "object") {
      const read = appFromZed(style, kind);
      if (Object.keys(read.set).length > 0) theme.app = read.set;
      if (Object.keys(read.offers).length > 0) theme.offers = read.offers;
    }
    if (italic === false) theme.italicComments = false;
    out.push(theme);
  }
  return out;
}

/** What a file read for import gave: the themes in it (and a file it includes that knobas cannot read here), or why it was refused. */
export type ImportResult = { themes: ImportedTheme[]; includes?: string } | { refused: string };

// ---------- import: a JetBrains colour scheme (`.icls`) ----------

/**
 * The attributes a JetBrains scheme is asked for each role, first present winning (Björn, 2026-11-22: "as an option").
 * An exported scheme lists only what differs from its parent (Darcula or the light Default), so a role it leaves out
 * draws in ink. It holds no window colours knobas can use, so the theme has a code part only.
 */
export const ICLS: Record<Role, readonly string[]> = {
  com: ["DEFAULT_LINE_COMMENT", "DEFAULT_BLOCK_COMMENT", "DEFAULT_DOC_COMMENT"],
  kw: ["DEFAULT_KEYWORD"],
  str: ["DEFAULT_STRING"],
  esc: ["DEFAULT_VALID_STRING_ESCAPE"],
  num: ["DEFAULT_NUMBER"],
  con: ["DEFAULT_CONSTANT", "DEFAULT_STATIC_FIELD"],
  fn: ["DEFAULT_FUNCTION_DECLARATION", "DEFAULT_FUNCTION_CALL", "DEFAULT_INSTANCE_METHOD"],
  ty: ["DEFAULT_CLASS_NAME", "DEFAULT_CLASS_REFERENCE", "DEFAULT_INTERFACE_NAME"],
  prop: ["DEFAULT_INSTANCE_FIELD"],
  key: ["JSON.PROPERTY_KEY", "YAML_SCALAR_KEY"],
  tag: ["DEFAULT_TAG", "HTML_TAG_NAME", "XML_TAG_NAME"],
  attr: ["DEFAULT_ATTRIBUTE", "HTML_ATTRIBUTE_NAME", "DEFAULT_METADATA"],
  var: ["DEFAULT_LOCAL_VARIABLE", "DEFAULT_PARAMETER"],
  op: ["DEFAULT_OPERATION_SIGN"],
  punct: ["DEFAULT_COMMA", "DEFAULT_SEMICOLON", "DEFAULT_BRACES"],
  head: ["MARKDOWN_HEADER_LEVEL_1", "MARKDOWN_HEADER"],
  link: ["DEFAULT_HYPERLINK_ATTRIBUTES", "HYPERLINK_ATTRIBUTES"],
};

/** A JetBrains colour value (`cc7832`, `0`, `80`: hex digits, left-padded to six) as `#RRGGBB`, or `null`. */
export function iclsColour(value: string | null | undefined): string | null {
  if (typeof value !== "string" || !/^[0-9a-f]{1,6}$/i.test(value.trim())) return null;
  return `#${value.trim().padStart(6, "0").toUpperCase()}`;
}

/** A JetBrains `.icls` scheme onto the roles: its name, its side and the colours it sets. `null` where it is not one. */
export function fromIcls(text: string): ImportedTheme | null {
  if (typeof DOMParser === "undefined") return null;
  const doc = new DOMParser().parseFromString(text, "application/xml");
  const scheme = doc.documentElement;
  if (scheme.nodeName !== "scheme" || doc.getElementsByTagName("parsererror").length > 0) return null;
  /** An attribute's own options, and those of the attribute it is based on where it sets none of its own. */
  const attribute = (name: string, depth = 0): Record<string, string> | null => {
    const node = [...scheme.querySelectorAll(":scope > attributes > option")].find((o) => o.getAttribute("name") === name);
    if (!node) return null;
    const out: Record<string, string> = {};
    for (const o of node.querySelectorAll(":scope > value > option")) out[o.getAttribute("name") ?? ""] = o.getAttribute("value") ?? "";
    const base = node.getAttribute("baseAttributes");
    if (Object.keys(out).length === 0 && base && depth < 4) return attribute(base, depth + 1);
    return out;
  };
  const syntax: Partial<Record<Role, string>> = {};
  for (const role of ROLES)
    for (const name of ICLS[role]) {
      const c = iclsColour(attribute(name)?.FOREGROUND);
      if (c !== null) {
        syntax[role] = c;
        break;
      }
    }
  const name = themeName(scheme.getAttribute("name"));
  const parent = scheme.getAttribute("parent_scheme") ?? "";
  const ground = iclsColour(attribute("TEXT")?.BACKGROUND);
  // Its own page colour, else its parent (Darcula is dark, Default light), else its name.
  const kind: ThemeKind = ground
    ? luminance(ground) > 0.18 ? "light" : "dark"
    : /darcula|dark/i.test(parent) ? "dark"
    : /default|light/i.test(parent) ? "light"
    : /\blight\b/i.test(name) ? "light"
    : "dark";
  const theme: ImportedTheme = { name, kind, syntax, from: name };
  // FONT_TYPE: 1 bold, 2 italic, 3 both. A comment set upright says so.
  const style = attribute("DEFAULT_LINE_COMMENT")?.FONT_TYPE ?? attribute("DEFAULT_BLOCK_COMMENT")?.FONT_TYPE;
  if (style !== undefined && style !== "2" && style !== "3") theme.italicComments = false;
  return theme;
}

/** A Base16 scheme as a theme with both parts. */
function base16Theme(text: string): ImportResult | null {
  const read = readBase16(text);
  if (read === null) return null;
  const { kind, set, syntax } = fromBase16(read.base);
  const name = themeName(read.name);
  return { themes: [{ name, kind, app: set, syntax, from: name }] };
}

/** A file's text read as themes: knobas's own, a VS Code theme, a Zed family, or a Base16 or Base24 scheme. */
export function importThemes(text: string): ImportResult {
  if (text.length > LARGEST_FILE) return { refused: "That file is over 1 MB, too large for a theme." };
  let json: unknown;
  try {
    // VS Code themes may hold comments and trailing commas (JSONC).
    json = JSON.parse(text);
  } catch {
    try {
      json = JSON.parse(stripJsonc(text));
    } catch {
      // A JetBrains scheme is XML; a Base16 scheme is YAML as often as JSON.
      if (text.trimStart().startsWith("<")) {
        const icls = fromIcls(text);
        return icls ? { themes: [icls] } : { refused: REFUSED };
      }
      return base16Theme(text) ?? { refused: REFUSED };
    }
  }
  if (json === null || typeof json !== "object" || Array.isArray(json)) return { refused: REFUSED };
  const v = json as Record<string, unknown>;
  if (v.knobasTheme === 1) {
    if (v.kind !== "light" && v.kind !== "dark") return { refused: REFUSED };
    const name = themeName(v.name);
    const theme: ImportedTheme = { name, kind: v.kind, syntax: syntaxOf(v.syntax), from: name };
    const app = appOf(v.app);
    if (app) theme.app = app;
    const offers = appOf(v.offers);
    if (offers && Object.keys(offers).length > 0) theme.offers = offers;
    const diff = diffOf(v.diff);
    if (diff) theme.diff = diff;
    if (v.italicComments === false) theme.italicComments = false;
    return { themes: [theme] };
  }
  const colors = v.colors !== null && typeof v.colors === "object" && !Array.isArray(v.colors);
  if (Array.isArray(v.tokenColors) || colors) {
    const out: ImportResult = { themes: [fromVsCode(v)] };
    // An `include` is not followed: the colours that file sets are worked out.
    if (typeof v.include === "string" && v.include !== "") out.includes = v.include.split(/[\\/]/).pop()!;
    return out;
  }
  if (Array.isArray(v.themes)) {
    const themes = fromZed(v);
    if (themes.length > 0) return { themes };
  }
  return base16Theme(text) ?? { refused: REFUSED };
}

/** JSONC to JSON: comments and trailing commas out, strings left as they are. */
function stripJsonc(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i]!;
    if (ch === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j;
    } else if (ch === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i += 1;
      out += "\n";
    } else if (ch === "/" && text[i + 1] === "*") {
      const end = text.indexOf("*/", i + 2);
      i = end === -1 ? text.length : end + 1;
    } else out += ch;
  }
  return out.replace(/,(\s*[}\]])/g, "$1");
}
