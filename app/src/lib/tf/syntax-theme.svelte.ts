/**
 * The two themes the window draws code in (ADR-0051): the one in the *While light* slot and the one in the *While
 * dark* slot, and a person's own themes.
 *
 * Both slots' colours are written on `<html>` at once, through the CSSOM: `--hl-<role>-light` and `--hl-<role>-dark`
 * (a colour, or `var(--ink)` for a role the theme leaves out), the diff's `--diff-add-<side>` and its three siblings
 * where a theme sets them, and `--hl-com-style-<side>`. `tf/css/tokens.css` reads them, `-light` in `:root` and `-dark`
 * in the dark blocks, with knobas's own colours as the fallbacks, so the system changing sides runs no script, and a
 * slot holding a knobas theme writes nothing at all.
 *
 * Kept in the webview's `localStorage`, read before the shell mounts (`main.ts` imports this module beside the fonts):
 * `knobas.theme.light` and `knobas.theme.dark` a theme's id each, `knobas.themes` a person's own as a JSON array, and
 * `knobas.diff.unchanged` how a diff's unchanged lines draw (`full`, `quiet`, `grey`; `data-diff-unchanged` on
 * `<html>`). This computer's, like the fonts: a backup does not carry them, and *Export…* is how a theme moves.
 */
import { ROLES } from "../render/roles";
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";
import { BUILTINS, DEFAULT_THEME } from "./theme/builtins";
import { MOST_THEMES, ownId, readThemes, uniqueName, type DiffKey, type ImportedTheme, type Theme, type ThemeKind } from "./theme/themes";

export const SLOT_KEY: Record<ThemeKind, string> = { light: "knobas.theme.light", dark: "knobas.theme.dark" };
export const THEMES_KEY = "knobas.themes";
export const UNCHANGED_KEY = "knobas.diff.unchanged";

/** How a diff's unchanged lines draw: all their colour, 40 % of it mixed into the secondary ink, or none. */
export type Unchanged = "full" | "quiet" | "grey";
export const UNCHANGED: readonly Unchanged[] = ["full", "quiet", "grey"];
export const DEFAULT_UNCHANGED: Unchanged = "quiet";

/** The diff's colour each theme key writes, as `--<name>-<side>`. */
export const DIFF_PROPERTY: Record<DiffKey, string> = { added: "--diff-add", removed: "--diff-del", addedWord: "--add-word", removedWord: "--del-word" };

/** How long after the last change a person's theme is saved. */
export const SAVE_AFTER = 300;

/** Where the choices are written: `<html>`'s inline style and attributes. */
export interface ThemeRoot {
  style: Pick<CSSStyleDeclaration, "setProperty" | "removeProperty">;
  setAttribute(name: string, value: string): void;
}

export interface SyntaxThemes {
  /** A person's own themes, oldest first. */
  readonly own: readonly Theme[];
  /** The id of the theme in a slot. */
  slot(kind: ThemeKind): string;
  /** The theme a slot draws now. */
  drawn(kind: ThemeKind): Theme;
  /** A theme by id, built-in or own. */
  find(id: string): Theme | undefined;
  /** The themes a slot may take: the built-ins of its kind, then a person's own. */
  of(kind: ThemeKind): Theme[];
  /** Puts a theme in its kind's slot; one of the other kind, or unknown, is refused. */
  choose(id: string): void;
  /** Keeps a new own theme (a copy, an import), its name made unique; `null` past {@link MOST_THEMES}. */
  add(theme: ImportedTheme): Theme | null;
  /** Changes an own theme: drawn at once where a slot holds it, kept {@link SAVE_AFTER} ms after the last change. */
  update(id: string, change: (theme: Theme) => Theme): void;
  /** Removes an own theme; a slot that held it goes back to knobas's. Returns how to put it back as it was. */
  remove(id: string): (() => void) | null;
  readonly unchanged: Unchanged;
  setUnchanged(value: Unchanged): void;
  /** Keeps what waits for its save now. */
  flush(): void;
  /** Tests only: nothing kept, both slots knobas's, nothing written. */
  reset(): void;
}

const isKnobas = (id: string) => id === DEFAULT_THEME.light || id === DEFAULT_THEME.dark;

/** The properties a slot's theme writes, every one, so a change removes what it no longer sets. */
export function slotProperties(kind: ThemeKind): string[] {
  return [...ROLES.map((role) => `--hl-${role}-${kind}`), `--hl-com-style-${kind}`, ...Object.values(DIFF_PROPERTY).map((p) => `${p}-${kind}`)];
}

/** What `theme` writes for its slot: every property it sets, by name. Nothing for a knobas theme. */
export function slotValues(theme: Theme): Record<string, string> {
  const out: Record<string, string> = {};
  if (isKnobas(theme.id)) return out;
  const side = theme.kind;
  for (const role of ROLES) out[`--hl-${role}-${side}`] = theme.syntax[role] ?? "var(--ink)";
  out[`--hl-com-style-${side}`] = theme.italicComments === false ? "normal" : "italic";
  for (const [key, property] of Object.entries(DIFF_PROPERTY) as [DiffKey, string][]) {
    const value = theme.diff?.[key];
    if (value) out[`${property}-${side}`] = value;
  }
  return out;
}

/** The variables a theme draws in, on an element of its own (the editor's preview of a theme no slot holds). */
export function previewValues(theme: Theme): Record<string, string> {
  const out: Record<string, string> = {};
  for (const role of ROLES) out[`--hl-${role}`] = theme.syntax[role] ?? "var(--ink)";
  out["--hl-com-style"] = theme.italicComments === false ? "normal" : "italic";
  for (const [key, property] of Object.entries(DIFF_PROPERTY) as [DiffKey, string][]) {
    const value = theme.diff?.[key];
    if (value) out[property] = value;
  }
  return out;
}

export function createSyntaxThemes(storage: () => KeptStorage | null, root: () => ThemeRoot | null, timers: { set: typeof setTimeout; clear: typeof clearTimeout } = { set: setTimeout, clear: clearTimeout }): SyntaxThemes {
  const keptSlot = { light: keptValue(storage, SLOT_KEY.light), dark: keptValue(storage, SLOT_KEY.dark) };
  const keptThemes = keptValue(storage, THEMES_KEY);
  const keptUnchanged = keptValue(storage, UNCHANGED_KEY);

  let own = $state<Theme[]>(readThemes(keptThemes.read()));
  const readSlot = (kind: ThemeKind) => keptSlot[kind].read() ?? DEFAULT_THEME[kind];
  const slots = $state<Record<ThemeKind, string>>({ light: readSlot("light"), dark: readSlot("dark") });
  const readUnchanged = (): Unchanged => {
    const kept = keptUnchanged.read();
    return UNCHANGED.includes(kept as Unchanged) ? (kept as Unchanged) : DEFAULT_UNCHANGED;
  };
  let unchanged = $state<Unchanged>(readUnchanged());
  let saving: ReturnType<typeof setTimeout> | null = null;

  const find = (id: string) => BUILTINS.find((t) => t.id === id) ?? own.find((t) => t.id === id);
  /** A slot's theme: what it holds where that is a theme of its kind, else knobas's. */
  const drawn = (kind: ThemeKind): Theme => {
    const theme = find(slots[kind]);
    return theme && theme.kind === kind ? theme : find(DEFAULT_THEME[kind])!;
  };

  function apply(kind: ThemeKind): void {
    const style = root()?.style;
    if (!style) return;
    const values = slotValues(drawn(kind));
    for (const property of slotProperties(kind)) {
      const value = values[property];
      if (value === undefined) style.removeProperty(property);
      else style.setProperty(property, value);
    }
  }

  function applyUnchanged(): void {
    root()?.setAttribute("data-diff-unchanged", unchanged);
  }

  function save(): void {
    if (saving !== null) timers.clear(saving);
    saving = null;
    if (own.length === 0) keptThemes.clear();
    else keptThemes.write(JSON.stringify(own));
  }

  function saveLater(): void {
    if (saving !== null) timers.clear(saving);
    saving = timers.set(save, SAVE_AFTER);
  }

  function setSlot(kind: ThemeKind, id: string): void {
    slots[kind] = id;
    if (id === DEFAULT_THEME[kind]) keptSlot[kind].clear();
    else keptSlot[kind].write(id);
    apply(kind);
  }

  apply("light");
  apply("dark");
  applyUnchanged();

  return {
    get own() {
      return own;
    },
    slot: (kind) => drawn(kind).id,
    drawn,
    find,
    of: (kind) => [...BUILTINS.filter((t) => t.kind === kind), ...own.filter((t) => t.kind === kind)],
    choose(id) {
      const theme = find(id);
      if (theme) setSlot(theme.kind, id);
    },
    add(imported) {
      if (own.length >= MOST_THEMES) return null;
      const taken = [...BUILTINS, ...own].map((t) => t.name);
      const theme: Theme = { ...imported, id: ownId(), name: uniqueName(imported.name, taken), syntax: { ...imported.syntax } };
      if (imported.diff) theme.diff = { ...imported.diff };
      while (find(theme.id)) theme.id = ownId();
      own = [...own, theme];
      save();
      return theme;
    },
    update(id, change) {
      const at = own.findIndex((t) => t.id === id);
      if (at === -1) return;
      const next = { ...change(own[at]!), id, kind: own[at]!.kind };
      own = own.map((t, i) => (i === at ? next : t));
      if (slots[next.kind] === id) apply(next.kind);
      saveLater();
    },
    remove(id) {
      const at = own.findIndex((t) => t.id === id);
      if (at === -1) return null;
      const theme = own[at]!;
      const held = slots[theme.kind] === id;
      own = own.filter((t) => t.id !== id);
      save();
      if (held) setSlot(theme.kind, DEFAULT_THEME[theme.kind]);
      return () => {
        if (find(id)) return;
        const list = [...own];
        list.splice(Math.min(at, list.length), 0, theme);
        own = list;
        save();
        if (held) setSlot(theme.kind, id);
      };
    },
    get unchanged() {
      return unchanged;
    },
    setUnchanged(value) {
      unchanged = value;
      if (value === DEFAULT_UNCHANGED) keptUnchanged.clear();
      else keptUnchanged.write(value);
      applyUnchanged();
    },
    flush() {
      if (saving !== null) save();
    },
    reset() {
      if (saving !== null) timers.clear(saving);
      saving = null;
      own = [];
      keptThemes.clear();
      unchanged = DEFAULT_UNCHANGED;
      keptUnchanged.clear();
      applyUnchanged();
      setSlot("light", DEFAULT_THEME.light);
      setSlot("dark", DEFAULT_THEME.dark);
    },
  };
}

/** The window's themes, written on `<html>` as soon as this module loads (`main.ts` loads it before mounting). */
export const syntaxThemes: SyntaxThemes = createSyntaxThemes(windowStorage, () => (typeof document === "undefined" ? null : document.documentElement));
