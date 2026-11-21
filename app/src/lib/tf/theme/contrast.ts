/**
 * A theme's colours against every surface code is drawn on (ADR-0051, section 9 of the decisions): the file viewer
 * and a diff on `--sheet`, a Docs code block on `--sheet-2`, a Markdown fence on `--paper`, and a diff's added and
 * removed lines. A role's figure is its lowest WCAG ratio over those; under 4.5:1 the editor names the surface and
 * offers a fix that moves the colour's lightness alone.
 *
 * The surfaces are knobas's own (`tf/css/tokens.css`): until the rest of the window is themed, they are what code is
 * drawn on. `contrast.test.ts` holds them equal to the stylesheet.
 */
import type { Role } from "../../render/roles";
import type { ThemeKind } from "./themes";

/** The ratio text needs. */
export const TEXT_RATIO = 4.5;

export type Surface = "sheet" | "sheet-2" | "paper" | "added line" | "removed line";

/** Each side's surfaces, as `tokens.css` defines `--sheet`, `--sheet-2`, `--paper`, `--ok-bg` and `--err-bg`. */
export const SURFACES: Record<ThemeKind, Record<Surface, string>> = {
  light: { sheet: "#F7F8F9", "sheet-2": "#EEF1F4", paper: "#E4E8EC", "added line": "#DDEFE4", "removed line": "#F7DEDD" },
  dark: { sheet: "#1A2027", "sheet-2": "#212932", paper: "#12171C", "added line": "#1B3427", "removed line": "#3F1F1F" },
};

/** Each side's `--ink`, what a role left out draws in. */
export const INK: Record<ThemeKind, string> = { light: "#17212B", dark: "#E4E9EE" };
/** Each side's `--ink-2`, what a quiet line mixes towards. */
export const INK_2: Record<ThemeKind, string> = { light: "#4B5663", dark: "#A0ABB7" };

/** The surfaces `role` is checked on, with `added`/`removed` the theme's own tints where it sets them. A comment draws `--ink-2` on a diff's lines. */
export function surfacesFor(kind: ThemeKind, role: Role | null, tints: { added?: string; removed?: string } = {}): Record<string, string> {
  const s: Record<string, string> = { ...SURFACES[kind] };
  if (tints.added) s["added line"] = tints.added;
  if (tints.removed) s["removed line"] = tints.removed;
  if (role === "com") {
    delete s["added line"];
    delete s["removed line"];
  }
  return s;
}

const rgb = (hex: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gam = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const toHex = (c: number[]) => `#${c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, "0")).join("").toUpperCase()}`;

export function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map(lin) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG's contrast ratio of two `#RRGGBB` colours. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p) as [number, number];
  return (x + 0.05) / (y + 0.05);
}

/** The lowest ratio of `colour` over `surfaces`, and on which. */
export function worst(colour: string, surfaces: Record<string, string>): { ratio: number; on: string } {
  let ratio = Infinity;
  let on = "";
  for (const [name, bg] of Object.entries(surfaces)) {
    const c = contrast(colour, bg);
    if (c < ratio) {
      ratio = c;
      on = name;
    }
  }
  return { ratio, on };
}

/** `#RRGGBB` in OKLCH: lightness, chroma, hue in radians. */
export function toOklch(hex: string): [number, number, number] {
  const [r, g, b] = rgb(hex).map(lin) as [number, number, number];
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(A, B), Math.atan2(B, A)];
}

/** OKLCH to linear sRGB, unclamped: a channel outside 0 to 1 is out of the gamut. */
function linearOf([L, C, H]: [number, number, number]): [number, number, number] {
  const a = C * Math.cos(H);
  const b = C * Math.sin(H);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s];
}

export function fromOklch(lch: [number, number, number]): string {
  return toHex(linearOf(lch).map(gam));
}

const inGamut = (lch: [number, number, number]) => linearOf(lch).every((c) => c >= -1e-4 && c <= 1 + 1e-4);

/**
 * `colour` with its lightness moved as little as reaches `want` on every one of `surfaces`: darker on a light theme,
 * lighter on a dark one, its hue kept and its chroma cut only where the gamut ends. 24 steps of bisection. A colour
 * that passes already comes back as it is.
 */
export function fix(colour: string, surfaces: Record<string, string>, kind: ThemeKind, want = TEXT_RATIO): string {
  if (worst(colour, surfaces).ratio >= want) return colour;
  const dark = kind === "dark";
  const [L, C, H] = toOklch(colour);
  let lo = dark ? L : 0;
  let hi = dark ? 1 : L;
  let best: string | null = null;
  for (let i = 0; i < 24; i += 1) {
    const mid = (lo + hi) / 2;
    // The chroma gives way only as far as the gamut needs, so the hue holds.
    let c = C;
    if (!inGamut([mid, c, H])) {
      let low = 0;
      let high = c;
      for (let k = 0; k < 16; k += 1) {
        const try_ = (low + high) / 2;
        if (inGamut([mid, try_, H])) low = try_;
        else high = try_;
      }
      c = low;
    }
    const out = fromOklch([mid, c, H]);
    const ok = worst(out, surfaces).ratio >= want;
    if (ok) best = out;
    if (dark === ok) hi = mid;
    else lo = mid;
  }
  return best ?? (dark ? "#FFFFFF" : "#000000");
}

/** `a` mixed into `b` in OKLab, `share` of `a` (0 to 1): what `color-mix(in oklab, a share, b)` draws. */
export function mixOklab(a: string, b: string, share: number): string {
  const lab = (hex: string) => {
    const [r, g, bl] = rgb(hex).map(lin) as [number, number, number];
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * bl);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * bl);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * bl);
    return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
  };
  const [L1, A1, B1] = lab(a) as [number, number, number];
  const [L2, A2, B2] = lab(b) as [number, number, number];
  const L = L1 * share + L2 * (1 - share);
  const A = A1 * share + A2 * (1 - share);
  const B = B1 * share + B2 * (1 - share);
  return fromOklch([L, Math.hypot(A, B), Math.atan2(B, A)]);
}

/** A role's figure in the editor: its lowest ratio, and the sentence when it is under 4.5:1. */
export function roleContrast(kind: ThemeKind, role: Role, value: string | undefined, tints: { added?: string; removed?: string } = {}): { ratio: number; on: string; low: boolean } {
  const { ratio, on } = worst(value ?? INK[kind], surfacesFor(kind, role, tints));
  return { ratio, on, low: ratio < TEXT_RATIO };
}

/** The sentence under a low role: *2.5:1 on a removed line. Text needs 4.5:1.* */
export function lowSentence(ratio: number, on: string): string {
  const where = on === "sheet" ? "the sheet" : on === "sheet-2" ? "a Docs code block" : on === "paper" ? "the paper" : `an ${on}`.replace("an removed", "a removed");
  return `${ratio.toFixed(1)}:1 on ${where}. Text needs 4.5:1.`;
}
