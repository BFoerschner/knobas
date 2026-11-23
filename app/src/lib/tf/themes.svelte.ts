/**
 * The themes the window draws (ADR-0051, ADR-0055): a person's own, the built-ins, and four slots. *While light* and
 * *While dark* for the window, and the same two for code. A code slot holds a theme of its own or **follows** the
 * window's: it draws the window slot's theme where that has code colours, else knobas's.
 *
 * Both sides are written on `<html>` at once, through the CSSOM, so the system changing sides runs no script:
 * for a window theme that is not knobas's own, every colour as `--<token>-<side>` (`tf/css/tokens.css` reads them,
 * `-light` in `:root` and `-dark` in the dark blocks, knobas's own values behind them); for a code theme, its roles
 * as `--hl-<role>-<side>`, the diff's colours it sets and whether comments slant. A window that is all knobas's writes
 * nothing. `data-sel` names the sides whose theme sets selected text (and `data-sel-each` marks an engine whose
 * highlights do not inherit, `theme/selection.ts`), `data-paint` counts the changes a diagram draws
 * again for, and `knobas.paint` keeps what was written for the head's script (`public/theme-boot.js`), so the first
 * paint has the theme.
 *
 * Kept in the webview's `localStorage` (this computer's, like the fonts; a backup does not carry them, *Export…* is
 * how a theme moves): `knobas.app-theme.light` and `.dark` the window slots, `knobas.theme.light` and `.dark` the
 * code slots (absent: follows), `knobas.themes` a person's own as a JSON array, `knobas.app-theme.asis` the themes
 * kept although unreadable, `knobas.paint` the cache, and `knobas.diff.unchanged` how a diff's unchanged lines draw.
 */
import { ROLES } from "../render/roles";
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";
import { type Colours, derive } from "./theme/app-derive";
import { unreadable } from "./theme/app-check";
import { APP_KEYS } from "./theme/app-roles";
import { BUILTINS, DEFAULT_THEME } from "./theme/builtins";
import { selectionInherits } from "./theme/selection";
import { hasApp, hasCode, MOST_THEMES, ownId, readThemes, uniqueName, type DiffKey, type ImportedTheme, type Theme, type ThemeKind } from "./theme/themes";

export const SLOT_KEY: Record<ThemeKind, string> = { light: "knobas.theme.light", dark: "knobas.theme.dark" };
export const APP_SLOT_KEY: Record<ThemeKind, string> = { light: "knobas.app-theme.light", dark: "knobas.app-theme.dark" };
export const THEMES_KEY = "knobas.themes";
export const ASIS_KEY = "knobas.app-theme.asis";

export const PAINT_KEY = "knobas.paint";
export const UNCHANGED_KEY = "knobas.diff.unchanged";
export const KINDS: readonly ThemeKind[] = ["light", "dark"];

/** How a diff's unchanged lines draw: all their colour, 40 % of it mixed into the secondary ink, or none. */
export type Unchanged = "full" | "quiet" | "grey";
export const UNCHANGED: readonly Unchanged[] = ["full", "quiet", "grey"];
export const DEFAULT_UNCHANGED: Unchanged = "quiet";

/** The diff's colour each theme key writes, as `--<name>-<side>`. */
export const DIFF_PROPERTY: Record<DiffKey, string> = { added: "--diff-add", removed: "--diff-del", addedWord: "--add-word", removedWord: "--del-word" };

/** How long after the last change a person's theme is saved. */
export const SAVE_AFTER = 300;
/** How long an unreadable theme stands before the window goes back by itself (the decisions' 3.9). */
export const KEEP_SECONDS = 15;

/**
 * An unreadable theme drawn on trial: `use` after *Use as it is*, `edit` after an edit took the drawn theme under the
 * floor. It ends by *Keep it*, by going back, or by the time running out, which goes back.
 */
export interface Trial {
  kind: "use" | "edit";
  theme: Theme;
  /** The name of what going back draws: the slot's theme before *Use as it is*, or the theme being edited. */
  backTo: string;
  /** When the window goes back by itself, in `Date.now()`'s milliseconds. */
  endsAt: number;
}

/** Where the choices are written: `<html>`'s inline style and attributes. */
export interface ThemeRoot {
  style: Pick<CSSStyleDeclaration, "setProperty" | "removeProperty">;
  setAttribute(name: string, value: string): void;
  removeAttribute(name: string): void;
}

export const isKnobas = (id: string): boolean => id === DEFAULT_THEME.light || id === DEFAULT_THEME.dark;

/** Every property a side's window theme may write. */
export function appProperties(kind: ThemeKind): string[] {
  return APP_KEYS.map(([, token]) => `${token}-${kind}`);
}
/** Every property a side's code theme may write. */
export function codeProperties(kind: ThemeKind): string[] {
  return [...ROLES.map((role) => `--hl-${role}-${kind}`), `--hl-com-style-${kind}`, ...Object.values(DIFF_PROPERTY).map((p) => `${p}-${kind}`)];
}

/** What a window theme writes for its side: every colour, derived where it sets none. Nothing for knobas's own. */
export function appValues(theme: Theme, colours: Colours): Record<string, string> {
  const out: Record<string, string> = {};
  if (isKnobas(theme.id)) return out;
  for (const [key, token] of APP_KEYS) {
    const value = colours[key];
    if (value !== undefined) out[`${token}-${theme.kind}`] = value;
  }
  return out;
}

/** What a code theme writes for its side: every role (ink where it sets none), comments' slant and the diff colours it sets. */
export function codeValues(theme: Theme, side: ThemeKind = theme.kind): Record<string, string> {
  const out: Record<string, string> = {};
  for (const role of ROLES) out[`--hl-${role}-${side}`] = theme.syntax[role] ?? "var(--ink)";
  out[`--hl-com-style-${side}`] = theme.italicComments === false ? "normal" : "italic";
  for (const [key, property] of Object.entries(DIFF_PROPERTY) as [DiffKey, string][]) {
    const value = theme.diff?.[key];
    if (value) out[`${property}-${side}`] = value;
  }
  return out;
}

const SHADOW: Record<ThemeKind, string> = {
  light: "0 1px 0 rgba(23,33,43,.06), 0 8px 24px -12px rgba(23,33,43,.25)",
  dark: "0 1px 0 rgba(0,0,0,.3), 0 10px 30px -12px rgba(0,0,0,.6)",
};

/**
 * Every token by its plain name, for an element of its own that draws `app`'s window colours and `code`'s code
 * colours whatever the window wears: a preview of a theme no slot holds or of the other side, and the keep plate.
 * Set through `cssVars`. The tokens `tokens.css` works out from others on `<html>` (a role's ink, a diff's tints,
 * the ring's colour, the shadow) are written too, since a custom property inherits what it computed there.
 */
export function scopeValues(app: Theme, code: Theme | null, colours: Colours = derive(app.kind, app.app ?? {}).colours): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, token] of APP_KEYS) {
    const value = colours[key];
    if (value !== undefined) out[token] = value;
  }
  const kind = app.kind;
  out["--shadow"] = SHADOW[kind];
  out["--sel-halo"] = kind === "dark" ? colours.ink : "transparent";
  out["--focus-c"] = colours.focus;
  out["--under"] = colours.paper;
  out["color-scheme"] = kind;
  out["accent-color"] = colours.ink;
  const roles = code ?? app;
  for (const role of ROLES) out[`--hl-${role}`] = roles.syntax[role] ?? colours.ink;
  out["--hl-com-style"] = roles.italicComments === false ? "normal" : "italic";
  out["--diff-add"] = roles.diff?.added ?? colours.okBg;
  out["--diff-del"] = roles.diff?.removed ?? colours.errBg;
  out["--add-word"] = roles.diff?.addedWord ?? colours.okWord;
  out["--del-word"] = roles.diff?.removedWord ?? colours.errWord;
  return out;
}

/** What a backup carries of the themes (Björn, 2026-11-22): a person's own, the four slots and the *Keep it*s. */
export interface ThemeSnapshot {
  v: 1;
  themes: Theme[];
  app: Record<ThemeKind, string | null>;
  code: Record<ThemeKind, string | null>;
  asis: string[];
}

export interface Themes {
  /** A person's own themes, oldest first. */
  readonly own: readonly Theme[];
  /** A theme by id, built-in or own. */
  find(id: string): Theme | undefined;
  // ---------- the window ----------
  /** The theme the window draws for a side: its slot's where that may be drawn, else knobas's. */
  appDrawn(kind: ThemeKind): Theme;
  /** The id a window slot holds, drawn or not (`knobas-light` where it holds none). */
  appSlot(kind: ThemeKind): string;
  /** The themes a window slot may take: the built-ins of its side, then a person's own with window colours. */
  appOf(kind: ThemeKind): Theme[];
  /** Every colour the window draws for a side. */
  colours(kind: ThemeKind): Colours;
  /** Every colour of a theme's window part, derived. */
  coloursOf(theme: Theme): Colours;
  /** Puts a theme in its side's window slot. One of the other side, or with no window colours, is refused. */
  chooseApp(id: string): void;
  /** Whether a theme is drawn although unreadable, because someone said *Keep it*. */
  keptAsIs(id: string): boolean;
  /** Remembers that an unreadable theme may be drawn (*Keep it*). */
  keepAsIs(id: string): void;
  /** Whether a theme's window colours fall under the floor and it was not kept as it is: choosing it asks first. */
  asksFirst(theme: Theme): boolean;
  /** *Use as it is*: draws an unreadable theme in its slot for {@link KEEP_SECONDS} s, then goes back unless kept. */
  useAsIs(id: string): void;
  /** The unreadable theme drawn on trial, if one is. */
  readonly trial: Trial | null;
  /** *Keep it*: the trial's theme is kept as it is. */
  keepTrial(): void;
  /** *Go back now*: the slot as it was before *Use as it is*, or the theme as it was before the first edit under the floor. */
  endTrial(): void;
  // ---------- code ----------
  /** The id of the code theme drawn for a side. */
  slot(kind: ThemeKind): string;
  /** The code theme drawn for a side. */
  drawn(kind: ThemeKind): Theme;
  /** Whether a side's code slot follows the window's. */
  follows(kind: ThemeKind): boolean;
  /** The themes a code slot may take: those of its side with code colours, built-ins first. */
  of(kind: ThemeKind): Theme[];
  /** Puts a theme in its side's code slot (its id is kept, knobas's too). */
  choose(id: string): void;
  /** The side's code slot follows the window's again. */
  follow(kind: ThemeKind): void;
  /** The side's code slot keeps what it draws now, apart from the window's. */
  setApart(kind: ThemeKind): void;
  // ---------- a person's own ----------
  /** Keeps a new own theme (a copy, an import), its name made unique; `null` past {@link MOST_THEMES}. */
  add(theme: ImportedTheme): Theme | null;
  /**
   * Changes an own theme: drawn by the next frame where a slot holds it, kept {@link SAVE_AFTER} ms after the last change.
   * `preview` (a colour picker's drag): the window is drawn only when the change is kept, or at the next change without
   * it, so a drag repaints the editor's preview each frame and the whole window once (the themes decisions' risk 9).
   */
  update(id: string, change: (theme: Theme) => Theme, preview?: boolean): void;
  /** Removes an own theme; each slot that held it lets go. Returns how to put it back as it was. */
  remove(id: string): (() => void) | null;
  /** knobas's own themes in both window slots and both code slots following; returns how to undo it. Nothing is deleted. */
  putKnobasBack(): () => void;
  /** Whether this run started with knobas's own themes (`knobas --default-theme`). */
  readonly startedDefault: boolean;
  readonly unchanged: Unchanged;
  setUnchanged(value: Unchanged): void;
  /** Keeps what waits for its save now, and draws what waits for its frame. */
  flush(): void;
  /** Everything a backup carries of the themes. */
  snapshot(): ThemeSnapshot;
  /** A backup's themes put back as they were kept: they replace what this computer has. */
  restore(snapshot: unknown): boolean;
  /** Tests only: nothing kept, every slot knobas's, nothing written. */
  reset(): void;
}

export interface ThemeTimers {
  set: typeof setTimeout;
  clear: typeof clearTimeout;
  /** The next frame; a test passes one it calls itself. */
  frame: (run: () => void) => unknown;
}

export const defaultTimers: ThemeTimers = {
  // Called as the window's own: `timers.set(...)` would call `setTimeout` on this object, which a browser refuses
  // ("Illegal invocation"); Node does not, so only the window shows it.
  set: ((run: () => void, ms?: number) => setTimeout(run, ms)) as typeof setTimeout,
  clear: ((id?: ReturnType<typeof setTimeout>) => clearTimeout(id)) as typeof clearTimeout,
  frame: (run) => (typeof requestAnimationFrame === "function" ? requestAnimationFrame(() => run()) : setTimeout(run, 16)),
};

export interface ThemeOptions {
  /** `knobas --default-theme` (`window.__knobasDefaultTheme`): this run draws knobas's own themes and touches nothing kept. */
  defaultTheme?: boolean;
  /** What is told once at start: a slot whose theme cannot be read and was not kept draws knobas's. */
  onUnreadable?: (theme: Theme, kind: ThemeKind) => void;
  /**
   * Whether the engine's highlights inherit (`theme/selection.ts`), asked the first time a theme's selection is drawn;
   * where they do not, `<html>` gets `data-sel-each` for good. Unset: they inherit.
   */
  selectionInherits?: () => boolean;
}

export function createThemes(storage: () => KeptStorage | null, root: () => ThemeRoot | null, timers: ThemeTimers = defaultTimers, options: ThemeOptions = {}): Themes {
  const keptCode = { light: keptValue(storage, SLOT_KEY.light), dark: keptValue(storage, SLOT_KEY.dark) };
  const keptApp = { light: keptValue(storage, APP_SLOT_KEY.light), dark: keptValue(storage, APP_SLOT_KEY.dark) };
  const keptThemes = keptValue(storage, THEMES_KEY);
  const keptAsis = keptValue(storage, ASIS_KEY);
  const keptPaint = keptValue(storage, PAINT_KEY);

  const keptUnchanged = keptValue(storage, UNCHANGED_KEY);
  const defaultTheme = options.defaultTheme === true;

  const readAsis = (): string[] => {
    try {
      const list: unknown = JSON.parse(keptAsis.read() ?? "[]");
      return Array.isArray(list) ? list.filter((id): id is string => typeof id === "string") : [];
    } catch {
      return [];
    }
  };

  let own = $state<Theme[]>(readThemes(keptThemes.read()));
  /** The window slots: an id, or `null` for knobas's. */
  const appSlots = $state<Record<ThemeKind, string | null>>({ light: defaultTheme ? null : keptApp.light.read(), dark: defaultTheme ? null : keptApp.dark.read() });
  /** The code slots: an id, or `null` where the slot follows the window's. */
  const codeSlots = $state<Record<ThemeKind, string | null>>({ light: defaultTheme ? null : keptCode.light.read(), dark: defaultTheme ? null : keptCode.dark.read() });
  let asis = $state<string[]>(readAsis());

  const readUnchanged = (): Unchanged => {
    const kept = keptUnchanged.read();
    return UNCHANGED.includes(kept as Unchanged) ? (kept as Unchanged) : DEFAULT_UNCHANGED;
  };
  let unchanged = $state<Unchanged>(readUnchanged());
  let saving: ReturnType<typeof setTimeout> | null = null;
  let trial = $state<Trial | null>(null);
  /** What going back restores: the slot's id before *Use as it is*, or the theme before the first edit under the floor. */
  let trialBack: { slot: string | null } | { theme: Theme } | null = null;
  let trialTimer: ReturnType<typeof setTimeout> | null = null;
  let framed = false;
  /** A preview's change waits for the window: drawn with the save, or the next change that is no preview. */
  let held = false;
  let paint = 0;
  /** Whether the engine's highlights inherit, once asked. */
  let inherits: boolean | null = null;

  const find = (id: string) => BUILTINS.find((t) => t.id === id) ?? own.find((t) => t.id === id);
  const knobas = (kind: ThemeKind) => find(DEFAULT_THEME[kind])!;

  // Derived colours, kept per theme object: an edit makes a new object, so a stale answer is never read.
  const derivedOf = new WeakMap<Theme, Colours>();
  const coloursOf = (theme: Theme): Colours => {
    let c = derivedOf.get(theme);
    if (!c) {
      c = derive(theme.kind, theme.app ?? {}).colours;
      derivedOf.set(theme, c);
    }
    return c;
  };
  const drawable = (theme: Theme) => asis.includes(theme.id) || trial?.theme.id === theme.id || unreadable(coloursOf(theme)).length === 0;

  /** The window slot's theme where it exists, is of that side, has window colours and may be drawn; else knobas's. */
  const appDrawn = (kind: ThemeKind): Theme => {
    const id = appSlots[kind];
    const theme = id === null ? undefined : find(id);
    return theme && theme.kind === kind && hasApp(theme) && drawable(theme) ? theme : knobas(kind);
  };
  /** The code slot's theme where it holds one of that side with code colours; a following slot draws the window's. */
  const codeDrawn = (kind: ThemeKind): Theme => {
    const id = codeSlots[kind];
    if (id === null) {
      const app = appDrawn(kind);
      return hasCode(app) ? app : knobas(kind);
    }
    const theme = find(id);
    return theme && theme.kind === kind && hasCode(theme) ? theme : knobas(kind);
  };

  /** Everything written on `<html>` for both sides now. */
  function values(): { vars: Record<string, string>; sel: ThemeKind[] } {
    const vars: Record<string, string> = {};
    const sel: ThemeKind[] = [];
    for (const kind of KINDS) {
      const app = appDrawn(kind);
      const colours = coloursOf(app);
      Object.assign(vars, appValues(app, colours));
      if (!isKnobas(app.id) && colours.selection) sel.push(kind);
      const code = codeDrawn(kind);
      // A knobas code theme under a window theme that is not knobas's writes its colours too: the stylesheet's
      // fallbacks read the window's states (a string is `var(--ok)`), so it would take the other theme's.
      if (!(isKnobas(code.id) && isKnobas(app.id))) Object.assign(vars, codeValues(code, kind));
    }
    return { vars, sel };
  }

  function apply(): void {
    framed = false;
    held = false;
    const html = root();
    const { vars, sel } = values();
    if (html) {
      for (const kind of KINDS)
        for (const property of [...appProperties(kind), ...codeProperties(kind)]) {
          const value = vars[property];
          if (value === undefined) html.style.removeProperty(property);
          else html.style.setProperty(property, value);
        }
      if (sel.length === 0) html.removeAttribute("data-sel");
      else {
        if (inherits === null) {
          inherits = options.selectionInherits?.() ?? true;
          if (!inherits) html.setAttribute("data-sel-each", "");
        }
        html.setAttribute("data-sel", sel.join(" "));
      }
    }
    // The run started with knobas's own themes keeps the cache as it was, for the next start.
    if (defaultTheme) return;
    if (Object.keys(vars).length === 0) keptPaint.clear();
    else keptPaint.write(JSON.stringify(sel.length === 0 ? { v: 1, vars } : { v: 1, vars, sel: sel.join(" ") }));
  }

  /** A diagram draws again in the new colours: a slot changed, or an edit was saved. */
  function repaint(): void {
    paint += 1;
    root()?.setAttribute("data-paint", String(paint));
  }

  function applyNow(): void {
    apply();
    repaint();
  }

  function applyLater(): void {
    if (framed) return;
    framed = true;
    timers.frame(apply);
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
    saving = timers.set(() => {
      save();
      if (framed || held) apply();
      repaint();
    }, SAVE_AFTER);
  }

  function writeApp(kind: ThemeKind, id: string | null): void {
    appSlots[kind] = id;
    if (defaultTheme) return;
    if (id === null) keptApp[kind].clear();
    else keptApp[kind].write(id);
  }
  function writeCode(kind: ThemeKind, id: string | null): void {
    codeSlots[kind] = id;
    if (defaultTheme) return;
    if (id === null) keptCode[kind].clear();
    else keptCode[kind].write(id);
  }
  function writeAsis(next: string[]): void {
    asis = next;
    if (next.length === 0) keptAsis.clear();
    else keptAsis.write(JSON.stringify(next));
  }

  function startTrial(kind: Trial["kind"], theme: Theme, back: { slot: string | null } | { theme: Theme }): void {
    if (trialTimer !== null) timers.clear(trialTimer);
    const backTo = "slot" in back ? (back.slot === null ? knobas(theme.kind) : (find(back.slot) ?? knobas(theme.kind))).name : back.theme.name;
    trial = { kind, theme, backTo, endsAt: Date.now() + KEEP_SECONDS * 1000 };
    trialBack = back;
    trialTimer = timers.set(() => goBack(), KEEP_SECONDS * 1000);
  }
  function stopTrial(): void {
    if (trialTimer !== null) timers.clear(trialTimer);
    trialTimer = null;
    trial = null;
    trialBack = null;
  }
  function goBack(): void {
    const now = trial;
    const back = trialBack;
    stopTrial();
    if (!now || !back) return;
    if ("slot" in back) writeApp(now.theme.kind, back.slot);
    else {
      own = own.map((t) => (t.id === back.theme.id ? back.theme : t));
      save();
    }
    applyNow();
  }

  // At start: a slot whose theme cannot be read and was not kept draws knobas's, keeps its value, and says so once.
  for (const kind of KINDS) {
    const id = appSlots[kind];
    const theme = id === null ? undefined : find(id);
    if (theme && theme.kind === kind && hasApp(theme) && !drawable(theme)) options.onUnreadable?.(theme, kind);
  }
  apply();
  applyUnchanged();

  return {
    get own() {
      return own;
    },
    find,
    appDrawn,
    appSlot: (kind) => appSlots[kind] ?? DEFAULT_THEME[kind],
    appOf: (kind) => [...BUILTINS.filter((t) => t.kind === kind), ...own.filter((t) => t.kind === kind && hasApp(t))],
    colours: (kind) => coloursOf(appDrawn(kind)),
    coloursOf,
    chooseApp(id) {
      const theme = find(id);
      if (!theme || !hasApp(theme)) return;
      writeApp(theme.kind, isKnobas(id) ? null : id);
      applyNow();
    },
    keptAsIs: (id) => asis.includes(id),
    keepAsIs(id) {
      if (asis.includes(id)) return;
      writeAsis([...asis, id]);
      applyNow();
    },
    asksFirst: (theme) => hasApp(theme) && !asis.includes(theme.id) && unreadable(coloursOf(theme)).length > 0,
    useAsIs(id) {
      const theme = find(id);
      if (!theme || !hasApp(theme)) return;
      const before = appSlots[theme.kind];
      startTrial("use", theme, { slot: before });
      writeApp(theme.kind, isKnobas(id) ? null : id);
      applyNow();
    },
    get trial() {
      return trial;
    },
    keepTrial() {
      const now = trial;
      stopTrial();
      if (now && !asis.includes(now.theme.id)) writeAsis([...asis, now.theme.id]);
      applyNow();
    },
    endTrial: () => goBack(),
    slot: (kind) => codeDrawn(kind).id,
    drawn: codeDrawn,
    follows: (kind) => codeSlots[kind] === null,
    of: (kind) => [...BUILTINS.filter((t) => t.kind === kind && hasCode(t)), ...own.filter((t) => t.kind === kind && hasCode(t))],
    choose(id) {
      const theme = find(id);
      if (!theme || !hasCode(theme)) return;
      writeCode(theme.kind, id);
      applyNow();
    },
    follow(kind) {
      writeCode(kind, null);
      applyNow();
    },
    setApart(kind) {
      // The picture does not change: the slot keeps what it draws now.
      writeCode(kind, codeDrawn(kind).id);
      applyNow();
    },
    add(imported) {
      if (own.length >= MOST_THEMES) return null;
      const taken = [...BUILTINS, ...own].map((t) => t.name);
      const theme: Theme = { ...imported, id: ownId(), name: uniqueName(imported.name, taken), syntax: { ...imported.syntax } };
      if (imported.app) theme.app = { ...imported.app };
      if (imported.offers) theme.offers = { ...imported.offers };
      if (imported.diff) theme.diff = { ...imported.diff };
      while (find(theme.id)) theme.id = ownId();
      own = [...own, theme];
      save();
      return theme;
    },
    update(id, change, preview = false) {
      const at = own.findIndex((t) => t.id === id);
      if (at === -1) return;
      const before = own[at]!;
      const next = { ...change(before), id, kind: before.kind };
      own = own.map((t, i) => (i === at ? next : t));
      // An edit that takes the drawn theme under the floor keeps drawing it on trial, and goes back unless kept; an
      // edit that brings it back over the floor ends the trial.
      if (appSlots[next.kind] === id && hasApp(next) && !asis.includes(id)) {
        const under = unreadable(coloursOf(next)).length > 0;
        if (under && trial === null && drawable(before)) startTrial("edit", next, { theme: before });
        else if (under && trial?.theme.id === id) trial = { ...trial, theme: next };
        else if (!under && trial?.kind === "edit" && trial.theme.id === id) stopTrial();
      }
      if (KINDS.some((kind) => appDrawn(kind).id === id || codeDrawn(kind).id === id || appSlots[kind] === id)) {
        if (preview) held = true;
        else applyLater();
      }
      saveLater();
    },
    remove(id) {
      const at = own.findIndex((t) => t.id === id);
      if (at === -1) return null;
      const theme = own[at]!;
      const heldApp = KINDS.filter((kind) => appSlots[kind] === id);
      const heldCode = KINDS.filter((kind) => codeSlots[kind] === id);
      const wasAsis = asis.includes(id);
      own = own.filter((t) => t.id !== id);
      save();
      for (const kind of heldApp) writeApp(kind, null);
      for (const kind of heldCode) writeCode(kind, null);
      if (wasAsis) writeAsis(asis.filter((x) => x !== id));
      applyNow();
      return () => {
        if (find(id)) return;
        const list = [...own];
        list.splice(Math.min(at, list.length), 0, theme);
        own = list;
        save();
        for (const kind of heldApp) writeApp(kind, id);
        for (const kind of heldCode) writeCode(kind, id);
        if (wasAsis) writeAsis([...asis, id]);
        applyNow();
      };
    },
    putKnobasBack() {
      const was = { app: { ...appSlots }, code: { ...codeSlots } };
      for (const kind of KINDS) {
        writeApp(kind, null);
        writeCode(kind, null);
      }
      applyNow();
      return () => {
        for (const kind of KINDS) {
          writeApp(kind, was.app[kind]);
          writeCode(kind, was.code[kind]);
        }
        applyNow();
      };
    },
    startedDefault: defaultTheme,
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
      if (saving !== null) {
        save();
        repaint();
      }
      if (framed || held) apply();
    },
    snapshot: () => ({ v: 1, themes: $state.snapshot(own) as Theme[], app: { ...appSlots }, code: { ...codeSlots }, asis: [...asis] }),
    restore(value) {
      if (value === null || typeof value !== "object") return false;
      const s = value as Partial<ThemeSnapshot>;
      if (s.v !== 1 || !Array.isArray(s.themes)) return false;
      own = readThemes(JSON.stringify(s.themes));
      save();
      const id = (x: unknown) => (typeof x === "string" && x !== "" ? x : null);
      for (const kind of KINDS) {
        writeApp(kind, id(s.app?.[kind]));
        writeCode(kind, id(s.code?.[kind]));
      }
      writeAsis(Array.isArray(s.asis) ? s.asis.filter((x): x is string => typeof x === "string") : []);
      applyNow();
      return true;
    },
    reset() {
      if (saving !== null) timers.clear(saving);
      saving = null;
      own = [];
      keptThemes.clear();
      writeAsis([]);
      unchanged = DEFAULT_UNCHANGED;
      keptUnchanged.clear();
      applyUnchanged();
      for (const kind of KINDS) {
        appSlots[kind] = null;
        codeSlots[kind] = null;
        keptApp[kind].clear();
        keptCode[kind].clear();
      }
      apply();
    },
  };
}

/** Whether this run was started with `knobas --default-theme` (the Rust side sets the global before the page). */
const startedDefault = typeof window !== "undefined" && (window as unknown as { __knobasDefaultTheme?: boolean }).__knobasDefaultTheme === true;

/** Told once at start, by the shell, once it can show a toast (`TfShell.svelte` reads it). */
export const unreadableAtStart: { theme: Theme; kind: ThemeKind }[] = [];

/** The window's themes, written on `<html>` as soon as this module loads (`main.ts` loads it before mounting). */
export const themes: Themes = createThemes(windowStorage, () => (typeof document === "undefined" ? null : document.documentElement), defaultTimers, {
  defaultTheme: startedDefault,
  onUnreadable: (theme, kind) => unreadableAtStart.push({ theme, kind }),
  selectionInherits: () => selectionInherits(),
});
