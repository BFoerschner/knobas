/**
 * Colour maths for themes (ADR-0051, ADR-0055): WCAG contrast, OKLab and OKLCH both ways, mixing, the lightness fix,
 * a translucent colour laid over an opaque one, and Machado's colour-vision simulation with the OKLab distance.
 *
 * Every colour in and out is `#RRGGBB`. Nothing here touches the page.
 */

const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gam = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
export type Triple = [number, number, number];

export const rgb = (hex: string): Triple => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as Triple;
export const toHex = (c: readonly number[]): string =>
  `#${c
    .map((v) =>
      Math.round(Math.min(1, Math.max(0, v)) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")
    .toUpperCase()}`;

/** `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA` as `[#RRGGBB, alpha]`, or `null`. */
export function parse(value: unknown): [string, number] | null {
  if (typeof value !== "string") return null;
  let v = value.trim();
  let m = /^#([0-9a-f])([0-9a-f])([0-9a-f])([0-9a-f])?$/i.exec(v);
  if (m) v = `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}${m[4] ? m[4] + m[4] : ""}`;
  m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(v);
  if (!m) return null;
  return [`#${m[1]!.toUpperCase()}`, m[2] ? parseInt(m[2], 16) / 255 : 1];
}

/** A colour, translucent or not, laid over an opaque one in sRGB as a browser composites it; `null` where it is not a colour. */
export function over(value: unknown, under: string): string | null {
  const p = parse(value);
  if (!p) return null;
  const [hex, a] = p;
  if (a >= 1) return hex;
  const f = rgb(hex);
  const b = rgb(under);
  return toHex(f.map((c, i) => c * a + b[i]! * (1 - a)));
}

export function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map(lin) as Triple;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG's contrast ratio of two colours. */
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

/** A ratio as a person reads it: cut to one decimal and never rounded up, so 4.45 reads *4.4:1* and never passes by a rounding. */
export function ratioText(ratio: number): string {
  return `${(Math.floor(ratio * 10 + 1e-9) / 10).toFixed(1)}:1`;
}

export function toOklab(hex: string): Triple {
  const [r, g, b] = rgb(hex).map(lin) as Triple;
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
}

/** `#RRGGBB` in OKLCH: lightness, chroma, hue in radians. */
export function toOklch(hex: string): Triple {
  const [L, A, B] = toOklab(hex);
  return [L, Math.hypot(A, B), Math.atan2(B, A)];
}

/** OKLCH to linear sRGB, unclamped: a channel outside 0 to 1 is out of the gamut. */
function linearOf([L, C, H]: Triple): Triple {
  const a = C * Math.cos(H);
  const b = C * Math.sin(H);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s];
}
const inGamut = (lch: Triple) => linearOf(lch).every((c) => c >= -1e-4 && c <= 1 + 1e-4);

/** OKLCH to `#RRGGBB`, the chroma cut back to the gamut's edge so the hue and lightness hold. */
export function fromOklch([L0, C, H]: Triple): string {
  const L = Math.min(1, Math.max(0, L0));
  let c = C;
  if (!inGamut([L, c, H])) {
    let low = 0;
    let high = c;
    for (let k = 0; k < 18; k += 1) {
      const t = (low + high) / 2;
      if (inGamut([L, t, H])) low = t;
      else high = t;
    }
    c = low;
  }
  return toHex(linearOf([L, c, H]).map(gam));
}

/** `a` mixed into `b` in OKLab, `share` of `a` (0 to 1): what `color-mix(in oklab, a share, b)` draws. */
export function mix(a: string, b: string, share: number): string {
  const [L1, A1, B1] = toOklab(a);
  const [L2, A2, B2] = toOklab(b);
  const L = L1 * share + L2 * (1 - share);
  const A = A1 * share + A2 * (1 - share);
  const B = B1 * share + B2 * (1 - share);
  return fromOklch([L, Math.hypot(A, B), Math.atan2(B, A)]);
}

/** `hex` with its OKLCH lightness moved by `by`, its chroma scaled by `chroma`. */
export function shift(hex: string, by: number, chroma = 1): string {
  const [L, C, H] = toOklch(hex);
  return fromOklch([L + by, C * chroma, H]);
}

export type Away = "auto" | "darker" | "lighter";

/**
 * `colour` with its OKLCH lightness moved as little as reaches `want` on every one of `surfaces`, its hue kept and its
 * chroma cut only where the gamut ends. `auto` moves away from the surfaces, darker where they are light on average;
 * where that way cannot reach `want` the other way is tried, and where neither can, the better of black and white
 * comes back. A colour that passes already comes back as it is.
 */
export function fix(colour: string, surfaces: Record<string, string>, want = 4.5, away: Away = "auto"): string {
  if (worst(colour, surfaces).ratio >= want) return colour;
  const grounds = Object.values(surfaces);
  const meanL = grounds.reduce((n, s) => n + toOklch(s)[0], 0) / grounds.length;
  const first = away === "auto" ? (meanL > 0.55 ? "darker" : "lighter") : away;
  const [L, C, H] = toOklch(colour);
  const run = (dir: "darker" | "lighter"): string | null => {
    let lo = dir === "lighter" ? L : 0;
    let hi = dir === "lighter" ? 1 : L;
    let best: string | null = null;
    for (let i = 0; i < 24; i += 1) {
      const mid = (lo + hi) / 2;
      const out = fromOklch([mid, C, H]);
      const ok = worst(out, surfaces).ratio >= want;
      if (ok) best = out;
      if ((dir === "lighter") === ok) hi = mid;
      else lo = mid;
    }
    return best;
  };
  const got = run(first) ?? run(first === "lighter" ? "darker" : "lighter");
  if (got) return got;
  return worst("#000000", surfaces).ratio >= worst("#FFFFFF", surfaces).ratio ? "#000000" : "#FFFFFF";
}

// ---------- colour vision (Machado, Oliveira and Fernandes 2009, severity 1.0, on linear RGB) ----------

const CVD = {
  protan: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
  ],
  deutan: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.01182, 0.04294, 0.968881],
  ],
  tritan: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.3039],
  ],
} as const;
export type Vision = keyof typeof CVD;
export const CVD_KINDS = Object.keys(CVD) as Vision[];

/** How `hex` looks to someone with that colour-vision deficiency at full severity. */
export function simulate(hex: string, kind: Vision): string {
  const c = rgb(hex).map(lin);
  return toHex(CVD[kind].map((row) => row[0] * c[0]! + row[1] * c[1]! + row[2] * c[2]!).map((v) => gam(Math.min(1, Math.max(0, v)))));
}

/** The OKLab distance of two colours (ΔEok). */
export function deltaE(a: string, b: string): number {
  const [L1, A1, B1] = toOklab(a);
  const [L2, A2, B2] = toOklab(b);
  return Math.hypot(L1 - L2, A1 - A2, B1 - B2);
}

/** The smallest distance of two colours over full colour vision and the three simulations, and under which. */
export function apart(a: string, b: string): { delta: number; under: "full colour vision" | Vision } {
  let low = deltaE(a, b);
  let under: "full colour vision" | Vision = "full colour vision";
  for (const kind of CVD_KINDS) {
    const d = deltaE(simulate(a, kind), simulate(b, kind));
    if (d < low) {
      low = d;
      under = kind;
    }
  }
  return { delta: low, under };
}
