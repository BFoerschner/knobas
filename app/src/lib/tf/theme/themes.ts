/**
 * A theme (ADR-0051): a name, a side, and a colour for some of the roles (`render/roles.ts`). A role it leaves out draws
 * in ink. It may set the diff's four colours, and whether comments slant.
 *
 * This file is the model: what a theme may hold, reading one a person kept, and reading and writing theme files.
 * Import reads knobas's own file, a VS Code theme and a Zed theme onto the roles; export writes knobas's own. Nothing
 * here touches the page or storage (`tf/syntax-theme.svelte.ts` does).
 */
import { PIERRE_ROLE, ROLES, type Role } from "../../render/roles";

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
  /** `#RRGGBB` a role; a role left out draws in ink. */
  syntax: Partial<Record<Role, string>>;
  diff?: DiffColours;
  /** Comments slant unless this is `false`. */
  italicComments?: boolean;
}

/** The most own themes a person keeps. */
export const MOST_THEMES = 64;
/** The largest theme file read, in bytes. */
export const LARGEST_FILE = 1024 * 1024;
export const LONGEST_NAME = 60;

export const REFUSED = "That file is not a theme knobas can read. It has no `knobasTheme`, `tokenColors` or `syntax`.";

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
  syntax: Partial<Record<Role, string>>;
  diff?: DiffColours;
  italicComments: boolean;
}

/** `theme` as knobas's file: its name, `<name>.knobas-theme.json`, and its text. */
export function exportTheme(theme: Theme): { fileName: string; text: string } {
  const file: ThemeFile = { knobasTheme: 1, name: theme.name, kind: theme.kind, syntax: { ...theme.syntax }, italicComments: theme.italicComments !== false };
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

const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
};

/** A VS Code theme onto the roles. */
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
  let kind: ThemeKind;
  const type = typeof json.type === "string" ? json.type.toLowerCase() : "";
  if (type === "light" || type === "dark") kind = type;
  else if (type === "hc" || type === "hc-black") kind = "dark";
  else if (type === "hc-light") kind = "light";
  else if (/\blight\b/i.test(name)) kind = "light";
  else if (/\bdark\b/i.test(name)) kind = "dark";
  else {
    const bg = colour((json.colors as Record<string, unknown> | undefined)?.["editor.background"]);
    kind = bg !== null && luminance(bg) > 0.4 ? "light" : "dark";
  }
  const theme: ImportedTheme = { name, kind, syntax, from: name };
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
    const syntaxStyles = style?.syntax as Record<string, { color?: string; font_style?: string } | undefined> | undefined;
    if (!syntaxStyles || typeof syntaxStyles !== "object") continue;
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
    if (italic === false) theme.italicComments = false;
    out.push(theme);
  }
  return out;
}

/** What a file read for import gave: the themes in it, or why it was refused. */
export type ImportResult = { themes: ImportedTheme[] } | { refused: string };

/** A file's text read as themes: knobas's own, a VS Code theme, or a Zed family. */
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
      return { refused: REFUSED };
    }
  }
  if (json === null || typeof json !== "object" || Array.isArray(json)) return { refused: REFUSED };
  const v = json as Record<string, unknown>;
  if (v.knobasTheme === 1) {
    if (v.kind !== "light" && v.kind !== "dark") return { refused: REFUSED };
    const name = themeName(v.name);
    const theme: ImportedTheme = { name, kind: v.kind, syntax: syntaxOf(v.syntax), from: name };
    const diff = diffOf(v.diff);
    if (diff) theme.diff = diff;
    if (v.italicComments === false) theme.italicComments = false;
    return { themes: [theme] };
  }
  if (Array.isArray(v.tokenColors)) return { themes: [fromVsCode(v)] };
  if (Array.isArray(v.themes)) {
    const themes = fromZed(v);
    if (themes.length > 0) return { themes };
  }
  return { refused: REFUSED };
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
