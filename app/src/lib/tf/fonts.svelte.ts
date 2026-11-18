/**
 * The faces the app draws in, and each one's size (Settings › Fonts, 2026-11-18, Björn: "a settings pane for being
 * able to switch all the different fonts, not just the code one and also being able to change their individual font
 * sizes (relative, since some fonts turn out smaller etc.)"; `docs/decisions/2026-11-18-every-face-is-yours.md`).
 *
 * Four roles, each a family and a relative size:
 *
 * - **Interface** (`ui`, Archivo): menus, lists, fields, buttons, and Markdown's text (READMEs, pull request and
 *   ticket bodies). `--sans`.
 * - **Headings** (`head`, Archivo drawn narrow): page and section titles, every Markdown heading, the job key and the
 *   big figures. `--head`. Every rule that narrows with `"wdth" N` sets `font-stretch: N%` too, so a chosen face
 *   narrows through its own width axis or condensed members, and one that has neither draws at its normal width.
 * - **Reading** (`read`, Source Serif 4): Docs pages. `--serif`.
 * - **Code** (`code`, JetBrains Mono): code everywhere (`code-face` before this pass). `--mono`, and its ligatures.
 *
 * Each family token (`css/tokens.css`) is `var(--<role>-chosen, <bundled>)` first, then the bundled face and its
 * stand-ins, so a family missing on this machine draws in the bundled face. A size is `--size-<role>`, a number, which
 * every rule that sets a role's family copies into the inherited `--fs`, and every font size multiplies by `--fs`
 * (`code-face.test.ts` holds both). A size evens one face out against the others: text and its line boxes follow it,
 * the lengths tuned around text (controls, rows, padding) hold. Zoom makes everything bigger.
 *
 * Kept in the webview's `localStorage` (`knobas.<role>.font`, `knobas.<role>.size` a whole percent, absent for 100,
 * and `knobas.code.ligatures`), and applied before the shell mounts (`main.ts`), as the code face was: read from the
 * database, a choice would land after the first draw. It names fonts installed on this machine, so it is this
 * machine's, not the profile's a backup carries.
 */
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";

export type Role = "ui" | "head" | "read" | "code";
export const ROLES: readonly Role[] = ["ui", "head", "read", "code"];

/** What each role is called in Settings. */
export const ROLE_NAMES: Record<Role, string> = { ui: "Interface", head: "Headings", read: "Reading", code: "Code" };

/** The face knobas brings along for each role, as a person names it. */
export const BUNDLED: Record<Role, string> = { ui: "Archivo", head: "Archivo", read: "Source Serif 4", code: "JetBrains Mono" };
/** The bundled faces' own family names (`tf.css` imports them), which a typed name may also be. */
export const BUNDLED_CSS: Record<Role, string> = { ui: "Archivo Variable", head: "Archivo Variable", read: "Source Serif 4 Variable", code: "JetBrains Mono" };
/**
 * The bundled faces' x-heights over their em (`OS/2.sxHeight`, read from the vendored files): the step that draws a
 * chosen face's lower case as tall as these is the one the size marks.
 */
export const BUNDLED_X_HEIGHT: Record<Role, number> = { ui: 0.526, head: 0.526, read: 0.475, code: 0.55 };

/** The custom property each role's whole family list is (`css/tokens.css`). */
export const FAMILY_PROPERTY: Record<Role, string> = { ui: "--sans", head: "--head", read: "--serif", code: "--mono" };

/** The custom property each role's chosen family is written to on `<html>`. */
export const CHOSEN_PROPERTY: Record<Role, string> = { ui: "--ui-chosen", head: "--head-chosen", read: "--read-chosen", code: "--mono-chosen" };

export const fontKey = (role: Role) => `knobas.${role}.font`;
export const sizeKey = (role: Role) => `knobas.${role}.size`;
export const LIGATURES_KEY = "knobas.code.ligatures";

/** The sizes, in percent: 85 to 130 in fives. Below 85 the 11 px step is under 9.4 px; 130 still fits a 13 px line in a 26 px row. */
export const SIZES = [85, 90, 95, 100, 105, 110, 115, 120, 125, 130] as const;
export const SMALLEST = SIZES[0];
export const LARGEST = SIZES[SIZES.length - 1]!;

/** A longer name is not a font's. */
const LONGEST = 200;

/** What the choice writes to: `<html>`'s inline style, through the CSSOM (CSP refuses inline styles in markup). */
export type RootStyle = Pick<CSSStyleDeclaration, "setProperty" | "removeProperty">;

/**
 * What a person typed or picked, as a family name: trimmed, the quotes of a CSS string dropped, and `null` for
 * nothing or for the role's bundled face itself.
 */
export function familyName(typed: string | null, role: Role = "code"): string | null {
  if (typed === null) return null;
  let name = typed.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  if (/^(["']).*\1$/.test(name)) name = name.slice(1, -1).trim();
  const lower = name.toLowerCase();
  if (name === "" || name.length > LONGEST || lower === BUNDLED[role].toLowerCase() || lower === BUNDLED_CSS[role].toLowerCase()) return null;
  return name;
}

/** `name` as a CSS string, which `font-family` takes whatever the name holds. */
export function cssFamily(name: string): string {
  return `"${name.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

/** A size read or asked for, on its step: the nearest of `SIZES`, 100 for anything that is not a number. */
export function sizeStep(percent: number | string | null): number {
  const n = typeof percent === "string" ? Number.parseFloat(percent) : percent;
  if (n === null || !Number.isFinite(n)) return 100;
  let best: number = SIZES[0];
  for (const at of SIZES) if (Math.abs(at - n) < Math.abs(best - n)) best = at;
  return best;
}

/**
 * The step that draws a face of x-height `xHeight` (over its em) with its lower case as tall as the role's bundled
 * face's; `null` when the face does not say, or the step would be past the ends (the face is too far off to even out).
 */
export function evenStep(role: Role, xHeight: number | null | undefined): number | null {
  if (xHeight === null || xHeight === undefined || !(xHeight > 0)) return null;
  const want = (BUNDLED_X_HEIGHT[role] / xHeight) * 100;
  if (want < SMALLEST - 2.5 || want > LARGEST + 2.5) return null;
  return sizeStep(want);
}

/** Hears a role's size change: `ratio` is the new size over the old. */
export type SizeListener = (role: Role, ratio: number) => void;

export interface Fonts {
  /** The role's chosen family, or `null` for its bundled face. */
  family(role: Role): string | null;
  /** The role's size, in percent. */
  size(role: Role): number;
  /** Whether code draws its font's ligatures (`=>`, `!=`, `->` as one sign); off until set. */
  readonly ligatures: boolean;
  /** Choose a family; an empty name or the bundled face's goes back to the bundled face. */
  setFamily(role: Role, name: string | null): void;
  /** Choose a size, in percent; put on the nearest step. */
  setSize(role: Role, percent: number): void;
  setLigatures(on: boolean): void;
  /** Calls `heard` after each change of a size, until the returned function is called. */
  onSize(heard: SizeListener): () => void;
  /** Tests only: the bundled faces at 100 %, no ligatures, nothing stored, nobody listening. */
  reset(): void;
}

export function createFonts(storage: () => KeptStorage | null, root: () => RootStyle | null): Fonts {
  const keptFamily = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, fontKey(role))])) as Record<Role, ReturnType<typeof keptValue>>;
  const keptSize = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, sizeKey(role))])) as Record<Role, ReturnType<typeof keptValue>>;
  const keptLigatures = keptValue(storage, LIGATURES_KEY);

  const families = $state(Object.fromEntries(ROLES.map((role) => [role, familyName(keptFamily[role].read(), role)])) as Record<Role, string | null>);
  const sizes = $state(Object.fromEntries(ROLES.map((role) => [role, sizeStep(keptSize[role].read())])) as Record<Role, number>);
  let ligatures = $state(keptLigatures.read() === "on");
  const listeners = new Set<SizeListener>();

  function applyFamily(style: RootStyle, role: Role): void {
    const family = families[role];
    if (family === null) style.removeProperty(CHOSEN_PROPERTY[role]);
    else style.setProperty(CHOSEN_PROPERTY[role], cssFamily(family));
    if (role === "ui") {
      // Archivo's word space is narrow, so the interface opens it a little (`tokens.css`); a chosen face keeps its own.
      if (family === null) {
        style.removeProperty("--ui-letter");
        style.removeProperty("--ui-word");
      } else {
        style.setProperty("--ui-letter", "normal");
        style.setProperty("--ui-word", "normal");
      }
    }
  }

  function applySize(style: RootStyle, role: Role): void {
    const size = sizes[role];
    if (size === 100) style.removeProperty(`--size-${role}`);
    else style.setProperty(`--size-${role}`, String(size / 100));
  }

  function applyLigatures(style: RootStyle): void {
    if (ligatures) style.setProperty("--mono-ligatures", "normal");
    else style.removeProperty("--mono-ligatures");
  }

  {
    const style = root();
    if (style) {
      for (const role of ROLES) {
        applyFamily(style, role);
        applySize(style, role);
      }
      applyLigatures(style);
    }
  }

  return {
    family: (role) => families[role],
    size: (role) => sizes[role],
    get ligatures() {
      return ligatures;
    },
    setFamily(role, name) {
      const family = familyName(name, role);
      families[role] = family;
      if (family === null) keptFamily[role].clear();
      else keptFamily[role].write(family);
      const style = root();
      if (style) applyFamily(style, role);
    },
    setSize(role, percent) {
      const size = sizeStep(percent);
      const was = sizes[role];
      sizes[role] = size;
      if (size === 100) keptSize[role].clear();
      else keptSize[role].write(String(size));
      if (size === was) return;
      const style = root();
      if (style) applySize(style, role);
      for (const heard of [...listeners]) heard(role, size / was);
    },
    setLigatures(on) {
      ligatures = on;
      if (on) keptLigatures.write("on");
      else keptLigatures.clear();
      const style = root();
      if (style) applyLigatures(style);
    },
    onSize(heard) {
      listeners.add(heard);
      return () => listeners.delete(heard);
    },
    reset() {
      listeners.clear();
      ligatures = false;
      keptLigatures.clear();
      const style = root();
      for (const role of ROLES) {
        families[role] = null;
        sizes[role] = 100;
        keptFamily[role].clear();
        keptSize[role].clear();
        if (style) {
          applyFamily(style, role);
          applySize(style, role);
        }
      }
      if (style) applyLigatures(style);
    },
  };
}

/**
 * Hears code's size change, with the new size over the old: a windowed list of code's lines (`textSize` in
 * `shell/windowed.svelte.ts`) takes its held heights by the ratio.
 */
export function onCodeSize(heard: (ratio: number) => void, of: Fonts = fonts): () => void {
  return of.onSize((role, ratio) => {
    if (role === "code") heard(ratio);
  });
}

/** The window's choices, written on `<html>` as soon as this module loads (`main.ts` loads it before mounting). */
export const fonts: Fonts = createFonts(windowStorage, () => (typeof document === "undefined" ? null : document.documentElement.style));
