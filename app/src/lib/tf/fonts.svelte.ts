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
 * Each role is tuned too (2026-11-21, Björn: "maybe add a setting for line height, spacing etc so I can tweak fonts
 * to my liking?"; `docs/decisions/2026-11-21-fonts-you-can-tune.md`, Fable's rulings): letter spacing for every role,
 * the weight of regular text for Interface, Reading and Code, Reading's line spacing, and code's line spacing, *Joined*
 * (the drawn face's own line height, rounded to whole device pixels, so box drawing joins; the default), *Open* (1.6,
 * 1.55 and 1.5 as before) or a notch. Each is one custom property on `<html>` (`--ls-<role>`, `--wt-<role>`,
 * `--lh-read`, `--code-lh-face`), absent at its default, so a default draws what it drew.
 *
 * Kept in the webview's `localStorage` (`knobas.<role>.font`, `knobas.<role>.size` a whole percent, absent for 100,
 * `knobas.code.ligatures`, `on` when turned on, `knobas.code.release`, `2009.22` for the older Cascadia, and the
 * knobs as whole numbers, absent at their default), and applied before the shell mounts (`main.ts`), as the code face
 * was: read from the database, a choice would land after the first draw. It names fonts installed on this machine, so
 * it is this machine's, not the profile's a backup carries.
 */
import { currentScale, onScaleChange } from "../shell/device-scale.svelte";
import { keptValue, windowStorage, type KeptStorage, type KeptValue } from "./kept-value";

export type Role = "ui" | "head" | "read" | "code";
export const ROLES: readonly Role[] = ["ui", "head", "read", "code"];

/** What each role is called in Settings. */
export const ROLE_NAMES: Record<Role, string> = { ui: "Interface", head: "Headings", read: "Reading", code: "Code" };

/** Inter's lower case over its em (`OS/2.sxHeight`, 1118 of 2048 units). */
export const INTER_X_HEIGHT = 1118 / 2048;
/** Cascadia's lower case over its em (`OS/2.sxHeight`, 1060 of 2048 units), Mono's and Code's alike, both releases. */
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
/**
 * The bundled Cascadias' CSS names (`fonts/fonts.css`, 2026-11-21): names of their own, so the bundled file draws even
 * where a font of the plain name is installed, and the plain name is the machine's copy (Björn: "is it possible to
 * have knobas pick the bundled font over the installed one? Maybe have them as different fonts in the list").
 */
export const CASCADIA_MONO_CSS = "knobas Cascadia Mono";
export const CASCADIA_CODE_CSS = "knobas Cascadia Code";
/** What Settings › Fonts puts after an installed font that has a bundled face's name, so the two read apart. */
export const INSTALLED_SUFFIX = " (installed)";
/** A kept code family that is Cascadia under one of its names (Code, Mono, NF, PL, or Nerd Fonts' CaskaydiaCove/Mono). */
const CASCADIA = /^\s*["']?\s*(?:cascadia|caskaydia)/i;

/**
 * Cascadia's own line height over its size: ascent 1900 and descent 480 of 2048, no gap, drawn at 105 %
 * (`CASCADIA_SIZE_ADJUST`), so 1.2202; both releases, Mono and Code alike. `css/tokens.css`'s `--code-lh-face` starts
 * at it.
 */
export const CASCADIA_LINE = 1.2202;

/** A release of Cascadia that comes with knobas: the current one, and the older one Björn asked for (2026-11-21). */
export type Release = "2407.24" | "2009.22";
export const CURRENT_RELEASE: Release = "2407.24";
export const OLDER_RELEASE: Release = "2009.22";

/** A code face that comes with knobas: its name in Settings, its CSS name, what follows its name in the list. */
export interface BundledCode {
  name: string;
  css: string;
  note: string | null;
  release: Release;
  ligatures: boolean;
  /**
   * How tall its `│` draws over the font size, which is the tallest row it joins at: 2407.24's `rclt` swaps it for a
   * `.stypo` glyph of 2430 of 2048 units, 2009.22 draws its own 3200 (−891 to 2309), both at 105 %.
   */
  bar: number;
}

/**
 * The code faces that come with knobas, as Settings › Fonts lists them first: each release's Mono and Code, the
 * *Ligatures in code* switch's two places within a release (Fable's ruling 7). Picking one sets the release and the
 * switch; the switch flips Mono and Code within the release.
 */
export const BUNDLED_CODE: readonly BundledCode[] = [
  { name: CASCADIA_MONO, css: CASCADIA_MONO_CSS, note: null, release: CURRENT_RELEASE, ligatures: false, bar: (2430 / 2048) * CASCADIA_SIZE_ADJUST },
  { name: CASCADIA_CODE, css: CASCADIA_CODE_CSS, note: "with ligatures", release: CURRENT_RELEASE, ligatures: true, bar: (2430 / 2048) * CASCADIA_SIZE_ADJUST },
  { name: "Cascadia Mono 2009.22", css: "knobas Cascadia Mono 2009", note: "older release", release: OLDER_RELEASE, ligatures: false, bar: (3200 / 2048) * CASCADIA_SIZE_ADJUST },
  { name: "Cascadia Code 2009.22", css: "knobas Cascadia Code 2009", note: "older release, with ligatures", release: OLDER_RELEASE, ligatures: true, bar: (3200 / 2048) * CASCADIA_SIZE_ADJUST },
];

/** The bundled code face of a release with the switch on or off. */
export function bundledCodeFace(release: Release, ligatures: boolean): BundledCode {
  return BUNDLED_CODE.find((face) => face.release === release && face.ligatures === ligatures)!;
}

/** The bundled code face a name is, as Settings names it or by its CSS name; `null` for any other name. */
export function bundledCodeNamed(name: string): BundledCode | null {
  const lower = bare(name).toLowerCase();
  return BUNDLED_CODE.find((face) => face.name.toLowerCase() === lower || face.css.toLowerCase() === lower) ?? null;
}

/** The face knobas brings along for each role, as a person names it. */
export const BUNDLED: Record<Role, string> = { ui: "Inter", head: "Inter", read: "Inter", code: CASCADIA_MONO };
/** The bundled faces' own family names (`fonts/fonts.css` declares them), which a typed name may also be. */
export const BUNDLED_CSS: Record<Role, string> = { ui: "Inter", head: "Inter", read: "Inter", code: CASCADIA_MONO_CSS };
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
/** The bundled Cascadia's release: nothing kept for the current one, `2009.22` for the older. */
export const RELEASE_KEY = "knobas.code.release";
/** Code's line spacing (`CodeLines`): nothing kept for *Joined*, `open`, or a notch in hundredths (110 to 180). */
export const LINES_KEY = "knobas.code.lines";
/** An installed code face's own line height and `│` over its size with its family, measured when it was picked. */
export const MEASURE_KEY = "knobas.code.measure";

/**
 * How code rows are spaced (2026-11-21, `docs/decisions/2026-11-21-fonts-you-can-tune.md`): *Joined*, the drawn face's
 * own line height rounded to whole device pixels, so box drawing joins (the default); *Open*, each code area's own
 * number as before (1.6, 1.55 or 1.5 of its size); or a notch, in hundredths of the size, rounded the same way and the
 * same in every code area (Fable's ruling 3).
 */
export type CodeLines = "joined" | "open" | number;
/** Code's line spacing notches, in hundredths of the font size: 1.10 to 1.80 in 0.05. */
export const CODE_LINES: readonly number[] = steps(110, 180, 5);

/** The sizes, in percent: 85 to 130 in fives. Below 85 the 11 px step is under 9.4 px; 130 still fits a 13 px line in a 26 px row. */
export const SIZES = [85, 90, 95, 100, 105, 110, 115, 120, 125, 130] as const;
export const SMALLEST = SIZES[0];
export const LARGEST = SIZES[SIZES.length - 1]!;

/**
 * A role's tuning (Fable's rulings 1 and 2): `ls` letter spacing in hundredths of an em, `wt` the weight of its regular
 * text, `lh` Reading's line spacing in hundredths. Headings has no weight (its rules set their own), Interface and
 * Headings no line spacing (it would fight the rows and controls tuned around them), and code's line spacing is
 * `CodeLines`.
 */
export type Knob = "ls" | "wt" | "lh";
export const KNOBS: Record<Role, readonly Knob[]> = { ui: ["ls", "wt"], head: ["ls"], read: ["lh", "ls", "wt"], code: ["ls", "wt"] };
/** Each knob's notches: letter spacing −0.03 to +0.06 em; a weight under 350 would let `bolder` give bold 400. */
export function knobSteps(role: Role, knob: Knob): readonly number[] {
  if (knob === "ls") return steps(-3, 6, 1);
  if (knob === "lh") return steps(140, 190, 5);
  return role === "code" ? steps(300, 500, 50) : steps(350, 450, 50);
}
/** Each knob's default: what it drew before it was a knob. */
export const KNOB_HOME: Record<Knob, number> = { ls: 0, wt: 400, lh: 165 };
export const knobKey = (role: Role, knob: Knob) => `knobas.${role}.${knob}`;
/** The custom property a knob writes on `<html>` (`css/tokens.css` and every rule of its role read it). */
export const knobProperty = (role: Role, knob: Knob) => `--${knob}-${role}`;

function steps(from: number, to: number, by: number): number[] {
  const found: number[] = [];
  for (let at = from; at <= to; at += by) found.push(at);
  return found;
}

/** `value` on the nearest of `notches`; `null` for anything that is not a number. */
export function onNotch(notches: readonly number[], value: number | string | null): number | null {
  const n = typeof value === "string" ? Number.parseFloat(value) : value;
  if (n === null || !Number.isFinite(n)) return null;
  let best = notches[0]!;
  for (const at of notches) if (Math.abs(at - n) < Math.abs(best - n)) best = at;
  return best;
}

/** A longer name is not a font's. */
const LONGEST = 200;

/** What the choice writes to: `<html>`'s inline style, through the CSSOM (CSP refuses inline styles in markup). */
export type RootStyle = Pick<CSSStyleDeclaration, "setProperty" | "removeProperty">;

/** A name as given: control characters out, trimmed, the quotes of a CSS string dropped. */
function bare(text: string): string {
  let name = text.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  if (/^(["']).*\1$/.test(name)) name = name.slice(1, -1).trim();
  return name;
}

/**
 * A kept family, as the store holds it: a font installed on this machine, Cascadia's plain names included (2026-11-21:
 * the bundled faces are never stored, nothing kept is the bundled face); `null` for nothing, a name too long to be a
 * font's, or the role's bundled face's CSS name.
 */
export function keptFamilyName(kept: string | null, role: Role = "code"): string | null {
  if (kept === null) return null;
  const name = bare(kept);
  if (name === "" || name.length > LONGEST || name.toLowerCase() === BUNDLED_CSS[role].toLowerCase()) return null;
  if (role !== "code" && name.toLowerCase() === BUNDLED[role].toLowerCase()) return null;
  return name;
}

/**
 * What a person typed or picked, as a family name: trimmed, the quotes of a CSS string dropped, and `null` for
 * nothing or for the role's bundled face itself. For code, the four faces that come with knobas (`BUNDLED_CODE`) are
 * the list's first group, and a name with " (installed)" after it is the machine's font of that name.
 */
export function familyName(typed: string | null, role: Role = "code"): string | null {
  if (typed === null) return null;
  const name = bare(typed);
  const lower = name.toLowerCase();
  if (lower.endsWith(INSTALLED_SUFFIX)) return keptFamilyName(name.slice(0, -INSTALLED_SUFFIX.length), role);
  if (name === "" || name.length > LONGEST || lower === BUNDLED[role].toLowerCase() || lower === BUNDLED_CSS[role].toLowerCase()) return null;
  if (role === "code" && bundledCodeNamed(name) !== null) return null;
  return name;
}

/** A chosen family as the field shows it: an installed font with a bundled face's name says so. */
export function shownName(family: string): string {
  const lower = family.toLowerCase();
  return lower === CASCADIA_MONO.toLowerCase() || lower === CASCADIA_CODE.toLowerCase() ? family + INSTALLED_SUFFIX : family;
}

/** `name` as a CSS string, which `font-family` takes whatever the name holds. */
export function cssFamily(name: string): string {
  return `"${name.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

/** A size read or asked for, on its step: the nearest of `SIZES`, 100 for anything that is not a number. */
export function sizeStep(percent: number | string | null): number {
  return onNotch(SIZES, percent) ?? 100;
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

/** Why code rows may have changed: their height (`rows`), the device scale, or code's letter spacing (wrapped rows). */
export type RowsCause = "rows" | "scale" | "track";

/** An installed code face as measured when picked: its own line height and its `│`, over its size. */
export interface FaceMeasure {
  family: string;
  line: number;
  bar: number | null;
}

export interface Fonts {
  /** The role's chosen family, or `null` for its bundled face. */
  family(role: Role): string | null;
  /** The face the role draws in when none is chosen: for code, one of `BUNDLED_CODE`'s, by the release and the switch. */
  bundled(role: Role): string;
  /** That face's CSS name (`fonts/fonts.css`): what a family list names to draw the bundled file. */
  bundledCss(role: Role): string;
  /** The role's size, in percent. */
  size(role: Role): number;
  /**
   * Whether code draws ligatures (`=>`, `!=`, `->` as one sign): the bundled face is then a Cascadia Code, and a chosen
   * face draws its own. Off until turned on (2026-11-21).
   */
  readonly ligatures: boolean;
  /** The bundled Cascadia's release: the current one until the older is picked. */
  readonly release: Release;
  /**
   * Choose a family; an empty name or the bundled face's goes back to the bundled face. A bundled code face's name
   * (`BUNDLED_CODE`) sets the release and the switch too.
   */
  setFamily(role: Role, name: string | null): void;
  /** Choose a size, in percent; put on the nearest step. */
  setSize(role: Role, percent: number): void;
  setLigatures(on: boolean): void;
  /** Calls `heard` after each change of a size, until the returned function is called. */
  onSize(heard: SizeListener): () => void;
  /** A knob's value (`KNOBS`), its default until set. */
  knob(role: Role, knob: Knob): number;
  /** Set a knob; put on its nearest notch. */
  setKnob(role: Role, knob: Knob, value: number): void;
  /** Code's line spacing: *Joined* until another is chosen. */
  readonly lines: CodeLines;
  setLines(lines: CodeLines): void;
  /** The drawn code face's own line height over its size: Cascadia's for a bundled face, an installed one's as measured. */
  readonly faceLine: number;
  /**
   * The tallest code line spacing notch at which the drawn face's `│` still joins from row to row, at 12, 13 and 14 px
   * times code's size on this screen (Fable's ruling 3, the ring); `null` when the face's `│` is not known or joins at
   * none.
   */
  readonly joinsUpTo: number | null;
  /**
   * The height of a code row in CSS px, as `css/tokens.css` draws it: `fontPx` the area's font size at 100 % (13 for a
   * file), `open` its own line height (1.6), at code's size and the window's device scale.
   */
  codeRow(fontPx: number, open: number): number;
  /** Calls `heard` after anything that may change code rows (`RowsCause`), until the returned function is called. */
  onCodeRows(heard: (cause: RowsCause) => void): () => void;
  /** Tests only: the bundled faces at 100 %, every knob at its default, nothing stored, nobody listening. */
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

/** The device scale and who hears it change (`shell/device-scale.svelte.ts` for the window). */
export interface Scale {
  now(): number;
  on(heard: () => void): () => void;
}

const fixedScale: Scale = { now: () => 1, on: () => () => {} };

/**
 * Measures an installed face as it is picked (Fable's ruling 3): its own line height over its size (`line-height:
 * normal` on a box of its own, out of sight) and how tall its `│` draws (a canvas's ink box); `null` where nothing is
 * laid out (jsdom) or the read fails.
 */
export type MeasureFace = (family: string) => { line: number; bar: number | null } | null;

const measureInDocument: MeasureFace = (family) => {
  if (typeof document === "undefined" || !document.body) return null;
  const font = `100px ${cssFamily(family)}, monospace`;
  const probe = document.createElement("span");
  probe.textContent = "x";
  probe.setAttribute("aria-hidden", "true");
  Object.assign(probe.style, { position: "absolute", visibility: "hidden", display: "inline-block", whiteSpace: "pre", font, lineHeight: "normal" });
  document.body.appendChild(probe);
  let line = 0;
  try {
    line = probe.getBoundingClientRect().height;
  } finally {
    probe.remove();
  }
  if (!(line > 0)) return null;
  let bar: number | null = null;
  try {
    const ctx = document.createElement("canvas").getContext("2d");
    if (ctx) {
      ctx.font = font;
      const ink = ctx.measureText("│");
      const tall = ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent;
      if (tall > 0) bar = Math.round(tall) / 100;
    }
  } catch {
    bar = null;
  }
  return { line: Math.round(line) / 100, bar };
};

/** Runs `go` once the window is idle (an installed face measured again for the next start); never in tests. */
export type Later = (go: () => void) => void;

const whenIdle: Later = (go) => {
  if (typeof requestIdleCallback === "function") requestIdleCallback(go, { timeout: 5000 });
  else setTimeout(go, 1500);
};

/** What a kept line spacing reads as: nothing is *Joined*. */
function linesOf(kept: string | null): CodeLines {
  if (kept === null) return "joined";
  if (kept === "open") return "open";
  return onNotch(CODE_LINES, kept) ?? "joined";
}

/** A kept measure, if it is one and of `family`. */
function measureOf(kept: string | null, family: string | null): FaceMeasure | null {
  if (kept === null || family === null) return null;
  try {
    const read = JSON.parse(kept) as Partial<FaceMeasure>;
    if (typeof read.family !== "string" || read.family.toLowerCase() !== family.toLowerCase()) return null;
    const line = faceRatio(read.line ?? null);
    if (line === null) return null;
    return { family: read.family, line, bar: faceRatio(read.bar ?? null) };
  } catch {
    return null;
  }
}

/** A face's line height or `│` over its size, if a face can have it (0.8 to 2.5). */
function faceRatio(value: number | null): number | null {
  return value !== null && Number.isFinite(value) && value >= 0.8 && value <= 2.5 ? value : null;
}

export function createFonts(
  storage: () => KeptStorage | null,
  root: () => RootStyle | null,
  loadFace: LoadFace = loadFromDocument,
  measureFace: MeasureFace = measureInDocument,
  scale: Scale = fixedScale,
  later: Later = () => {},
): Fonts {
  const keptFamily = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, fontKey(role))])) as Record<Role, KeptValue>;
  const keptSize = Object.fromEntries(ROLES.map((role) => [role, keptValue(storage, sizeKey(role))])) as Record<Role, KeptValue>;
  const keptLigatures = keptValue(storage, LIGATURES_KEY);
  const keptRelease = keptValue(storage, RELEASE_KEY);
  const keptLines = keptValue(storage, LINES_KEY);
  const keptMeasure = keptValue(storage, MEASURE_KEY);
  const keptKnob = (role: Role, knob: Knob) => keptValue(storage, knobKey(role, knob));
  toCascadiaMono(keptFamily.code, keptSize.code, keptLigatures, keptValue(storage, MONO_KEY));

  const families = $state(Object.fromEntries(ROLES.map((role) => [role, keptFamilyName(keptFamily[role].read(), role)])) as Record<Role, string | null>);
  const sizes = $state(Object.fromEntries(ROLES.map((role) => [role, sizeStep(keptSize[role].read())])) as Record<Role, number>);
  // On only when turned on (2026-11-21); `toCascadiaMono` cleared what earlier builds kept.
  let ligatures = $state(keptLigatures.read() === "on");
  let release = $state<Release>(keptRelease.read() === OLDER_RELEASE ? OLDER_RELEASE : CURRENT_RELEASE);
  /** The release and switch drawn now: they follow a press once its face has landed. */
  let drawn = { release, ligatures };
  const knobs = $state(
    Object.fromEntries(
      ROLES.map((role) => [role, Object.fromEntries(KNOBS[role].map((knob) => [knob, onNotch(knobSteps(role, knob), keptKnob(role, knob).read()) ?? KNOB_HOME[knob]]))]),
    ) as Record<Role, Partial<Record<Knob, number>>>,
  );
  let lines = $state<CodeLines>(linesOf(keptLines.read()));
  let measure = $state<FaceMeasure | null>(measureOf(keptMeasure.read(), families.code));
  let dpr = $state(scale.now());
  /** Counts the presses that turn the bundled face, so a face that lands after a later press is not applied. */
  let pressed = 0;
  const listeners = new Set<SizeListener>();
  const rowListeners = new Set<(cause: RowsCause) => void>();

  const faceLine = () => (families.code === null ? CASCADIA_LINE : (measure?.line ?? CASCADIA_LINE));
  const faceBar = () => (families.code === null ? bundledCodeFace(release, ligatures).bar : (measure?.bar ?? null));

  function rowsChanged(cause: RowsCause): void {
    for (const heard of [...rowListeners]) heard(cause);
  }

  function applyFamily(style: RootStyle, role: Role): void {
    const family = families[role];
    if (family === null) style.removeProperty(CHOSEN_PROPERTY[role]);
    else style.setProperty(CHOSEN_PROPERTY[role], cssFamily(family));
    if (role === "code") applySynthesis(style);
  }

  function applySize(style: RootStyle, role: Role): void {
    const size = sizes[role];
    if (size === 100) style.removeProperty(`--size-${role}`);
    else style.setProperty(`--size-${role}`, String(size / 100));
  }

  /**
   * The bundled face drawn (`--mono-bundled`, the sheet's Cascadia Mono when absent) and every face's ligatures
   * (`--mono-ligatures`, `css/tokens.css`).
   */
  function applyBundled(style: RootStyle): void {
    const face = bundledCodeFace(drawn.release, drawn.ligatures);
    if (face.css === CASCADIA_MONO_CSS) style.removeProperty("--mono-bundled");
    else style.setProperty("--mono-bundled", cssFamily(face.css));
    if (drawn.ligatures) style.setProperty("--mono-ligatures", "normal");
    else style.removeProperty("--mono-ligatures");
    applySynthesis(style);
  }

  /**
   * 2009.22 has no italic: while it draws, comments stand upright, not in a slant the browser makes up, which would
   * lean every `│` in a comment (Fable's ruling 7).
   */
  function applySynthesis(style: RootStyle): void {
    if (drawn.release === OLDER_RELEASE && families.code === null) style.setProperty("--mono-synth", "none");
    else style.removeProperty("--mono-synth");
  }

  function applyKnob(style: RootStyle, role: Role, knob: Knob): void {
    const value = knobs[role][knob] ?? KNOB_HOME[knob];
    const property = knobProperty(role, knob);
    if (value === KNOB_HOME[knob]) style.removeProperty(property);
    else style.setProperty(property, knob === "ls" ? `${value / 100}em` : knob === "lh" ? String(value / 100) : String(value));
  }

  /** `--code-lh-face` the line height drawn (the face's own, or a notch), `--code-lh: initial` for *Open*. */
  function applyLines(style: RootStyle): void {
    if (lines === "open") {
      style.setProperty("--code-lh", "initial");
      style.removeProperty("--code-lh-face");
      return;
    }
    style.removeProperty("--code-lh");
    const ratio = lines === "joined" ? faceLine() : lines / 100;
    if (ratio === CASCADIA_LINE) style.removeProperty("--code-lh-face");
    else style.setProperty("--code-lh-face", String(ratio));
  }

  function applyScale(style: RootStyle): void {
    if (dpr === 1) style.removeProperty("--dpr");
    else style.setProperty("--dpr", String(dpr));
  }

  /** Turns the bundled code face to `to`, once its files have landed; the last press wins. */
  function turn(to: { release: Release; ligatures: boolean }): void {
    ligatures = to.ligatures;
    release = to.release;
    if (to.ligatures) keptLigatures.write("on");
    else keptLigatures.clear();
    if (to.release === OLDER_RELEASE) keptRelease.write(OLDER_RELEASE);
    else keptRelease.clear();
    const press = ++pressed;
    const apply = () => {
      if (press !== pressed) return;
      drawn = { ...to };
      const style = root();
      if (style) applyBundled(style);
    };
    // The face turned to is drawn once it has landed; one that fails to load draws in its stand-ins, as at a start.
    const landing = loadFace(bundledCodeFace(to.release, to.ligatures).css);
    if (landing === null) apply();
    else void landing.then(apply, apply);
  }

  {
    const style = root();
    if (style) {
      for (const role of ROLES) {
        applyFamily(style, role);
        applySize(style, role);
        for (const knob of KNOBS[role]) applyKnob(style, role, knob);
      }
      applyBundled(style);
      applyLines(style);
      applyScale(style);
    }
  }
  // Kept for the window's life: a scale change rounds code rows to the new device grid.
  scale.on(() => {
    const now = scale.now();
    if (now === dpr) return;
    dpr = now;
    const style = root();
    if (style) applyScale(style);
    rowsChanged("scale");
  });
  // An installed face measured again once the window is idle: a font updated since is drawn so from the next start.
  if (families.code !== null) {
    const family = families.code;
    later(() => {
      if (families.code !== family) return;
      const now = measureFace(family);
      if (now !== null && (now.line !== measure?.line || now.bar !== measure?.bar)) keptMeasure.write(JSON.stringify({ family, ...now }));
    });
  }

  return {
    family: (role) => families[role],
    bundled: (role) => (role === "code" ? bundledCodeFace(release, ligatures).name : BUNDLED[role]),
    bundledCss: (role) => (role === "code" ? bundledCodeFace(release, ligatures).css : BUNDLED_CSS[role]),
    size: (role) => sizes[role],
    get ligatures() {
      return ligatures;
    },
    get release() {
      return release;
    },
    setFamily(role, name) {
      const family = familyName(name, role);
      if (role === "code" && family === null && name !== null) {
        const face = bundledCodeNamed(name);
        if (face !== null && (face.release !== release || face.ligatures !== ligatures)) turn(face);
      }
      const wasLine = faceLine();
      families[role] = family;
      if (family === null) keptFamily[role].clear();
      else keptFamily[role].write(family);
      const style = root();
      if (style) applyFamily(style, role);
      if (role !== "code") return;
      // An installed face's own line height and `│`, measured once as it is picked and kept, so a start never measures.
      if (family === null) {
        measure = null;
        keptMeasure.clear();
      } else if (measure?.family.toLowerCase() !== family.toLowerCase()) {
        const read = measureFace(family);
        measure = { family, line: faceRatio(read?.line ?? null) ?? CASCADIA_LINE, bar: faceRatio(read?.bar ?? null) };
        keptMeasure.write(JSON.stringify(measure));
      }
      if (faceLine() === wasLine || lines !== "joined") return;
      if (style) applyLines(style);
      rowsChanged("rows");
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
      if (role === "code") rowsChanged("rows");
    },
    setLigatures(on) {
      if (on !== ligatures) turn({ release, ligatures: on });
    },
    onSize(heard) {
      listeners.add(heard);
      return () => listeners.delete(heard);
    },
    knob: (role, knob) => knobs[role][knob] ?? KNOB_HOME[knob],
    setKnob(role, knob, value) {
      if (!KNOBS[role].includes(knob)) return;
      const to = onNotch(knobSteps(role, knob), value) ?? KNOB_HOME[knob];
      const kept = keptKnob(role, knob);
      if (to === KNOB_HOME[knob]) kept.clear();
      else kept.write(String(to));
      if (to === knobs[role][knob]) return;
      knobs[role][knob] = to;
      const style = root();
      if (style) applyKnob(style, role, knob);
      // Code's letter spacing rewraps a wrapped log line: the lists read their rows again.
      if (role === "code" && knob === "ls") rowsChanged("track");
    },
    get lines() {
      return lines;
    },
    setLines(next) {
      const to = next === "joined" || next === "open" ? next : (onNotch(CODE_LINES, next) ?? "joined");
      if (to === "joined") keptLines.clear();
      else keptLines.write(String(to));
      if (to === lines) return;
      lines = to;
      const style = root();
      if (style) applyLines(style);
      rowsChanged("rows");
    },
    get faceLine() {
      return faceLine();
    },
    get joinsUpTo() {
      const bar = faceBar();
      if (bar === null) return null;
      const size = sizes.code / 100;
      for (let i = CODE_LINES.length - 1; i >= 0; i--) {
        const notch = CODE_LINES[i]!;
        if ([12, 13, 14].every((fontPx) => Math.round(fontPx * size * (notch / 100) * dpr) / dpr <= fontPx * size * bar)) return notch;
      }
      return null;
    },
    codeRow(fontPx, open) {
      const px = fontPx * (sizes.code / 100);
      if (lines === "open") return px * open;
      const ratio = lines === "joined" ? faceLine() : lines / 100;
      return Math.round(px * ratio * dpr) / dpr;
    },
    onCodeRows(heard) {
      rowListeners.add(heard);
      return () => rowListeners.delete(heard);
    },
    reset() {
      listeners.clear();
      rowListeners.clear();
      ligatures = false;
      release = CURRENT_RELEASE;
      drawn = { release, ligatures };
      pressed += 1;
      lines = "joined";
      measure = null;
      keptLigatures.clear();
      keptRelease.clear();
      keptLines.clear();
      keptMeasure.clear();
      const style = root();
      for (const role of ROLES) {
        families[role] = null;
        sizes[role] = 100;
        keptFamily[role].clear();
        keptSize[role].clear();
        for (const knob of KNOBS[role]) {
          knobs[role][knob] = KNOB_HOME[knob];
          keptKnob(role, knob).clear();
        }
        if (style) {
          applyFamily(style, role);
          applySize(style, role);
          for (const knob of KNOBS[role]) applyKnob(style, role, knob);
        }
      }
      if (style) {
        applyBundled(style);
        applyLines(style);
      }
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

/** The bundled code faces no rule draws while the release and the switch are as they are, which the idle loader leaves alone (`shell/faces.ts`). */
export function unusedCodeFaces(of: Fonts = fonts): string[] {
  const drawn = of.bundledCss("code");
  return BUNDLED_CODE.map((face) => face.css).filter((css) => css !== drawn);
}

/**
 * A windowed list of code's lines (a file's source, a diff's lines, a run's log, a JAR's class): its row height now, as
 * its estimate, and `textSize` for `shell/windowed.svelte.ts`, which hears each change of the rows' height with the new
 * height over the old, so the list takes its held heights by the ratio and reads its drawn rows again. `fontPx` and
 * `open` are the area's font size at 100 % and its own line height (`css/*.css`). A change of code's letter spacing is
 * heard at 1, so a wrapped row is read again.
 */
export function codeRows(fontPx: number, open: number, of: Fonts = fonts): { estimate: number; textSize: (heard: (ratio: number) => void) => () => void } {
  return {
    estimate: of.codeRow(fontPx, open),
    textSize: (heard) => {
      let was = of.codeRow(fontPx, open);
      return of.onCodeRows((cause) => {
        const now = of.codeRow(fontPx, open);
        const ratio = now / was;
        was = now;
        if (ratio !== 1 || cause === "track") heard(ratio);
      });
    },
  };
}

/** The window's choices, written on `<html>` as soon as this module loads (`main.ts` loads it before mounting). */
export const fonts: Fonts = createFonts(
  windowStorage,
  () => (typeof document === "undefined" ? null : document.documentElement.style),
  loadFromDocument,
  measureInDocument,
  { now: currentScale, on: onScaleChange },
  whenIdle,
);
