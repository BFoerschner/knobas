/**
 * The faces the app draws in, and each one's size (Settings › Fonts, 2026-11-18, Björn: "a settings pane for being
 * able to switch all the different fonts, not just the code one and also being able to change their individual font
 * sizes (relative, since some fonts turn out smaller etc.)"; `docs/decisions/2026-11-18-every-face-is-yours.md`).
 *
 * Four roles, each a family and a relative size:
 *
 * - **Interface** (`ui`, Inter since 2026-11-20, Archivo before): menus, lists, fields, buttons, and Markdown's text
 *   (READMEs, pull request and ticket bodies). `--sans`.
 * - **Headings** (`head`, Inter in its display cut; Archivo drawn narrow before): page and section titles, every
 *   Markdown heading, the job key and the big figures. `--head`. Every rule that narrows with `"wdth" N` sets
 *   `font-stretch: N%` too, so a chosen face narrows through its own width axis or condensed members, and one that
 *   has neither, Inter among them, draws at its normal width.
 * - **Reading** (`read`, Inter; Source Serif 4 before): Docs pages. `--serif`.
 * - **Code** (`code`, Cascadia Mono since 2026-11-21, Cascadia Code from 2026-11-20): code everywhere (`code-face`
 *   before this pass). `--mono`. *Ligatures in code* is off until turned on; on, the bundled face is Cascadia Code,
 *   whose ligatures draw `=>` and `->` as one sign, and a chosen face draws its own
 *   (`docs/decisions/2026-11-21-code-is-cascadia-mono.md`).
 *
 * Each family token (`css/tokens.css`) is `var(--<role>-chosen, <bundled>)` first, then the bundled face and its
 * stand-ins, so a family missing on this machine draws in the bundled face. A size is `--size-<role>`, a number, which
 * every rule that sets a role's family copies into the inherited `--fs`, and every font size multiplies by `--fs`
 * (`code-face.test.ts` holds both). A size evens one face out against the others: text and its line boxes follow it,
 * the lengths tuned around text (controls, rows, padding) hold. Zoom makes everything bigger.
 *
 * Kept in the webview's `localStorage` (`knobas.<role>.font`, `knobas.<role>.size` a whole percent, absent for 100,
 * and `knobas.code.ligatures`, `on` when turned on), and applied before the shell mounts (`main.ts`), as the code face was: read from the
 * database, a choice would land after the first draw. It names fonts installed on this machine, so it is this
 * machine's, not the profile's a backup carries.
 */
import { keptValue, windowStorage, type KeptStorage, type KeptValue } from "./kept-value";

export type Role = "ui" | "head" | "read" | "code";
export const ROLES: readonly Role[] = ["ui", "head", "read", "code"];

/** What each role is called in Settings. */
export const ROLE_NAMES: Record<Role, string> = { ui: "Interface", head: "Headings", read: "Reading", code: "Code" };

/** Inter's lower case over its em (`OS/2.sxHeight`, 1118 of 2048 units). */
export const INTER_X_HEIGHT = 1118 / 2048;
/** Cascadia's lower case over its em (`OS/2.sxHeight`, 1060 of 2048 units), Mono's and Code's alike. */
export const CASCADIA_X_HEIGHT = 1060 / 2048;
/**
 * The bundled Cascadias' `size-adjust` (`fonts/fonts.css`): drawn at 105 %, the lower case is as tall as the
 * JetBrains Mono they replaced, which is the size Björn set for Cascadia by hand. The code role's 100 % is that
 * (2026-11-20; kept for Cascadia Mono on 2026-11-21, so its "105 %" is what the slider calls 100 %).
 */
export const CASCADIA_SIZE_ADJUST = 1.05;
/** The code face knobas draws in (2026-11-21): no programming ligatures, so ASCII art and boxes stay as drawn. */
export const CASCADIA_MONO = "Cascadia Mono";
/** The code face drawn with *Ligatures in code* on: Cascadia Mono's metrics, with `=>`, `->` and the like joined. */
export const CASCADIA_CODE = "Cascadia Code";
/** A kept code family that is Cascadia under one of its names (Code, Mono, NF, PL, or Nerd Fonts' CaskaydiaCove/Mono). */
const CASCADIA = /^\s*["']?\s*(?:cascadia|caskaydia)/i;

/** The face knobas brings along for each role, as a person names it. */
export const BUNDLED: Record<Role, string> = { ui: "Inter", head: "Inter", read: "Inter", code: CASCADIA_MONO };
/** The bundled faces' own family names (`fonts/fonts.css` declares them), which a typed name may also be. */
export const BUNDLED_CSS: Record<Role, string> = { ui: "Inter", head: "Inter", read: "Inter", code: CASCADIA_MONO };
/**
 * The bundled faces' x-heights over their em (`OS/2.sxHeight`, read from the vendored files): the step that draws a
 * chosen face's lower case as tall as these is the one the size marks.
 */
export const BUNDLED_X_HEIGHT: Record<Role, number> = { ui: INTER_X_HEIGHT, head: INTER_X_HEIGHT, read: INTER_X_HEIGHT, code: CASCADIA_X_HEIGHT * CASCADIA_SIZE_ADJUST };

/** The custom property each role's whole family list is (`css/tokens.css`). */
export const FAMILY_PROPERTY: Record<Role, string> = { ui: "--sans", head: "--head", read: "--serif", code: "--mono" };

/** The custom property each role's chosen family is written to on `<html>`. */
export const CHOSEN_PROPERTY: Record<Role, string> = { ui: "--ui-chosen", head: "--head-chosen", read: "--read-chosen", code: "--mono-chosen" };

export const fontKey = (role: Role) => `knobas.${role}.font`;
export const sizeKey = (role: Role) => `knobas.${role}.size`;
export const LIGATURES_KEY = "knobas.code.ligatures";
/** Set once this machine's code settings have been carried over to Cascadia Mono (`toCascadiaMono`). */
export const MONO_KEY = "knobas.code.mono";

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
  // Both Cascadias are declared by the bundle, which wins over an installed face of the same name: either is the bundled face.
  if (role === "code" && lower === CASCADIA_CODE.toLowerCase()) return null;
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
  /** The face the role draws in when none is chosen: for code, Cascadia Code while ligatures are on, else Cascadia Mono. */
  bundled(role: Role): string;
  /** The role's size, in percent. */
  size(role: Role): number;
  /**
   * Whether code draws ligatures (`=>`, `!=`, `->` as one sign): the bundled face is then Cascadia Code, and a chosen
   * face draws its own. Off until turned on (2026-11-21).
   */
  readonly ligatures: boolean;
  /**
   * Choose a family; an empty name or the bundled face's goes back to the bundled face. Cascadia Code is the bundled
   * face with ligatures on, and Cascadia Mono with them off, so choosing either sets the switch too.
   */
  setFamily(role: Role, name: string | null): void;
  /** Choose a size, in percent; put on the nearest step. */
  setSize(role: Role, percent: number): void;
  setLigatures(on: boolean): void;
  /** Calls `heard` after each change of a size, until the returned function is called. */
  onSize(heard: SizeListener): () => void;
  /** Tests only: the bundled faces at 100 %, ligatures off, nothing stored, nobody listening. */
  reset(): void;
}

/**
 * Loads a bundled face's upright and italic before code turns to it, so no line is drawn in the stand-ins first and
 * again in the face (the window's font set); `null` when there is nothing to wait for, and the turn is at once.
 */
export type LoadFace = (family: string) => Promise<unknown> | null;

const loadFromDocument: LoadFace = (family) => {
  const set = typeof document === "undefined" ? undefined : document.fonts;
  if (!set || typeof set.load !== "function") return null;
  const name = cssFamily(family);
  return Promise.all([set.load(`16px ${name}`), set.load(`italic 16px ${name}`)]);
};

export function createFonts(storage: () => KeptStorage | null, root: () => RootStyle | null, loadFace: LoadFace = loadFromDocument): Fonts {
  const keptFamily = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, fontKey(role))])) as Record<Role, ReturnType<typeof keptValue>>;
  const keptSize = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, sizeKey(role))])) as Record<Role, ReturnType<typeof keptValue>>;
  const keptLigatures = keptValue(storage, LIGATURES_KEY);
  toCascadiaMono(keptFamily.code, keptSize.code, keptLigatures, keptValue(storage, MONO_KEY));

  const families = $state(Object.fromEntries(ROLES.map((role) => [role, familyName(keptFamily[role].read(), role)])) as Record<Role, string | null>);
  const sizes = $state(Object.fromEntries(ROLES.map((role) => [role, sizeStep(keptSize[role].read())])) as Record<Role, number>);
  // On only when turned on (2026-11-21); `toCascadiaMono` cleared what earlier builds kept.
  let ligatures = $state(keptLigatures.read() === "on");
  /** Counts the switch's presses, so a face that lands after a later press is not applied. */
  let pressed = 0;
  const listeners = new Set<SizeListener>();

  function applyFamily(style: RootStyle, role: Role): void {
    const family = families[role];
    if (family === null) style.removeProperty(CHOSEN_PROPERTY[role]);
    else style.setProperty(CHOSEN_PROPERTY[role], cssFamily(family));
  }

  function applySize(style: RootStyle, role: Role): void {
    const size = sizes[role];
    if (size === 100) style.removeProperty(`--size-${role}`);
    else style.setProperty(`--size-${role}`, String(size / 100));
  }

  /** On: Cascadia Code as the bundled face and every face's ligatures; off: the sheet's Cascadia Mono, none (`css/tokens.css`). */
  function applyLigatures(style: RootStyle, on: boolean = ligatures): void {
    if (on) {
      style.setProperty("--mono-bundled", cssFamily(CASCADIA_CODE));
      style.setProperty("--mono-ligatures", "normal");
    } else {
      style.removeProperty("--mono-bundled");
      style.removeProperty("--mono-ligatures");
    }
  }

  function setLigatures(on: boolean): void {
    ligatures = on;
    if (on) keptLigatures.write("on");
    else keptLigatures.clear();
    const press = ++pressed;
    const apply = () => {
      const style = root();
      if (style && press === pressed) applyLigatures(style, on);
    };
    // The face turned to is drawn once it has landed; one that fails to load draws in its stand-ins, as at a start.
    const landing = loadFace(on ? CASCADIA_CODE : CASCADIA_MONO);
    if (landing === null) apply();
    else void landing.then(apply, apply);
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
    bundled: (role) => (role === "code" && ligatures ? CASCADIA_CODE : BUNDLED[role]),
    size: (role) => sizes[role],
    get ligatures() {
      return ligatures;
    },
    setFamily(role, name) {
      const family = familyName(name, role);
      if (role === "code" && family === null && name !== null) {
        const lower = familyName(name, "ui")?.toLowerCase();
        if (lower === CASCADIA_CODE.toLowerCase() && !ligatures) setLigatures(true);
        else if (lower === CASCADIA_MONO.toLowerCase() && ligatures) setLigatures(false);
      }
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
    setLigatures,
    onSize(heard) {
      listeners.add(heard);
      return () => listeners.delete(heard);
    },
    reset() {
      listeners.clear();
      ligatures = false;
      pressed += 1;
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
 * Once a machine, on the first start since 2026-11-21 (`MONO_KEY`), code's settings move to Cascadia Mono:
 *
 * - A Cascadia chosen from the machine's fonts (Code, Mono, NF, PL, or Nerd Fonts' CaskaydiaCove) becomes the bundled
 *   face, drawn at 105 % (`CASCADIA_SIZE_ADJUST`), its size divided by 1.05 onto the nearest step so it draws as large
 *   as it did: an installed Cascadia Mono at 105 % is the bundled one at 100 %. (From 2026-11-20 this ran on every
 *   start, so a Cascadia picked after it was taken away again at the next; now it runs once and a pick stays.)
 * - Ligatures: the bundled face draws Cascadia Mono, whatever an earlier build kept (an `on` kept before 2026-11-20 was
 *   never cleared, and Björn asked for Mono). A chosen face keeps its ligatures as they drew, on unless turned off,
 *   kept as this build's `on`.
 *
 * Sizes stay as they are: code's 100 % is the same size as before.
 */
function toCascadiaMono(family: KeptValue, size: KeptValue, ligatures: KeptValue, done: KeptValue): void {
  if (done.read() !== null) return;
  const kept = family.read();
  if (kept !== null && CASCADIA.test(kept)) {
    family.clear();
    const now = sizeStep(sizeStep(size.read()) / CASCADIA_SIZE_ADJUST);
    if (now === 100) size.clear();
    else size.write(String(now));
  }
  const chosen = familyName(family.read(), "code") !== null;
  if (chosen && ligatures.read() !== "off") ligatures.write("on");
  else ligatures.clear();
  done.write("1");
}

/** The bundled code face no rule draws while the switch is as it is, which the idle loader leaves alone (`shell/faces.ts`). */
export function unusedCodeFace(of: Fonts = fonts): string {
  return of.ligatures ? CASCADIA_MONO : CASCADIA_CODE;
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
