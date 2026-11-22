/**
 * What a theme's window colours are checked against (ADR-0055; the decisions' 3.8 and 3.9): WCAG 2.2 AA, 4.5:1 for
 * text and 3:1 for a part, each colour on every surface it is drawn on; the floor under which a theme cannot be read;
 * how far apart the four states are for a colour-blind reader; and the fixes, which move lightness alone except
 * where telling the states apart has to move a hue.
 */
import { type AppKey, NAME } from "./app-roles";
import { type AppColours, type Colours, derive, TONES } from "./app-derive";
import { apart, contrast, CVD_KINDS, fix, fromOklch, luminance, mix, ratioText, shift, simulate, toOklab, toOklch, type Triple, type Vision, worst } from "./colour";
import type { ThemeKind } from "./themes";

/** One check: a colour, the surfaces it is drawn on (by the words a low row's sentence uses), and the ratio it needs. */
export type Pair = readonly [key: AppKey, surfaces: Record<string, string>, want: number];

/** Every check of a side's colours. */
export function pairs(c: Colours): Pair[] {
  const grounds = { "the page": c.paper, "the panel": c.sheet, "the inset": c.sheet2, "the field": c.field };
  const three = { "the page": c.paper, "the panel": c.sheet, "the inset": c.sheet2 };
  const onCard = { "the card": c.card, "the card, pressed": c.card2 };
  const sel = { "what is pressed": c.fill };
  const row = { "a selected row": c.selRow };
  const pagePanel = { "the page": c.paper, "the panel": c.sheet };
  return [
    [
      "ink",
      {
        ...grounds,
        "the highlight": c.mark,
        "the Passed tint": c.okBg,
        "the Attention tint": c.warnBg,
        "the Failed tint": c.errBg,
        "the Running tint": c.infoBg,
        "the Unknown tint": c.mutedBg,
        "an added word": c.okWord,
        "a removed word": c.errWord,
        ...(c.selection ? { "selected text": c.selection } : {}),
      },
      4.5,
    ],
    ["ink2", grounds, 4.5],
    ["muted", { ...grounds, "the Unknown tint": c.mutedBg }, 4.5],
    ["link", three, 4.5],
    ["ok", { ...three, "its tint": c.okBg }, 4.5],
    ["warn", { ...three, "its tint": c.warnBg }, 4.5],
    ["err", { ...three, "its tint": c.errBg }, 4.5],
    ["info", { ...three, "its tint": c.infoBg }, 4.5],
    ["onFill", sel, 4.5],
    ["onSelRow", row, 4.5],
    ["onPrimary", { "the primary button": c.primary, "the primary button under the pointer": mix(c.primary, c.sheet, 0.8) }, 4.5],
    ["cardInk", onCard, 4.5],
    ["cardInk2", onCard, 4.5],
    ["onInkOk", row, 4.5],
    ["onInkWarn", row, 4.5],
    ["onInkErr", row, 4.5],
    ["onInkInfo", row, 4.5],
    ["onInkMuted", row, 4.5],
    ["cardOk", { "its tint on the card": c.cardOkBg }, 4.5],
    ["cardWarn", { "its tint on the card": c.cardWarnBg }, 4.5],
    ["cardErr", { "its tint on the card": c.cardErrBg }, 4.5],
    ["cardInfo", { "its tint on the card": c.cardInfoBg }, 4.5],
    ["cardMuted", { "its tint on the card": c.cardMutedBg }, 4.5],
    // parts
    ["cardOk", { "the card": c.card }, 3],
    ["cardWarn", { "the card": c.card }, 3],
    ["cardErr", { "the card": c.card }, 3],
    ["cardInfo", { "the card": c.card }, 3],
    ["cardMuted", { "the card": c.card }, 3],
    ["focus", pagePanel, 3],
    ["fill", pagePanel, 3],
    ["primary", pagePanel, 3],
  ];
}

export interface Failure {
  key: AppKey;
  ratio: number;
  on: string;
  want: number;
}

/** What a side's colours fail, one entry a check. */
export function failures(c: Colours): Failure[] {
  const out: Failure[] = [];
  for (const [key, surfaces, want] of pairs(c)) {
    const w = worst(c[key]!, surfaces);
    if (w.ratio < want) out.push({ key, ratio: w.ratio, on: w.on, want });
  }
  return out;
}

/** The floor: the ratio under which the text a person needs to find their way back cannot be read. */
export const FLOOR = 3;

/** The pairs under the floor: text on the page, the panel and the field, on what is selected, on a primary button, on the card. A theme with any is unreadable. */
export function unreadable(c: Colours): Failure[] {
  const out: Failure[] = [];
  const floor = (key: AppKey, surfaces: Record<string, string>) => {
    const w = worst(c[key]!, surfaces);
    if (w.ratio < FLOOR) out.push({ key, ratio: w.ratio, on: w.on, want: FLOOR });
  };
  floor("ink", { "the page": c.paper, "the panel": c.sheet, "the field": c.field });
  floor("onFill", { "what is pressed": c.fill });
  floor("onSelRow", { "a selected row": c.selRow });
  floor("onPrimary", { "the primary button": c.primary });
  floor("cardInk", { "the card": c.card });
  return out;
}

/** A row's figure: its lowest ratio against what it needs, over every check of its keys. */
export function rowFigure(c: Colours, keys: readonly AppKey[]): Failure | null {
  let low: Failure | null = null;
  for (const [key, surfaces, want] of pairs(c)) {
    if (!keys.includes(key)) continue;
    const w = worst(c[key]!, surfaces);
    if (!low || w.ratio / want < low.ratio / low.want) low = { key, ratio: w.ratio, on: w.on, want };
  }
  return low;
}

/** The sentence under a low row: *3.7:1 on the inset. Text needs 4.5:1.* or *2.2:1 on the panel. A part needs 3:1.* */
export function lowSentence(f: Failure): string {
  return `${ratioText(f.ratio)} on ${f.on}. ${f.want === 3 ? "A part needs 3:1." : "Text needs 4.5:1."}`;
}

/** A filled pair's fix moves its ground: these words keep their colour, and the key named moves away from them. */
const GROUND: Partial<Record<AppKey, AppKey>> = { onFill: "fill", onPrimary: "primary", onSelRow: "selRow" };

/** The fix's button: *Darken to 4.5:1*, *Lighten to 3:1*, or for a filled pair *Darken the button to 4.5:1*. */
export function fixWords(kind: ThemeKind, c: Colours, f: Failure): string {
  const mark = f.want === 3 ? "3:1" : "4.5:1";
  const ground = GROUND[f.key];
  if (ground) {
    const darker = luminance(c[f.key]!) > luminance(c[ground]!);
    return `${darker ? "Darken" : "Lighten"} the ${ground === "primary" ? "button" : ground === "selRow" ? "row" : "fill"} to ${mark}`;
  }
  const moved = fixOne(kind, {}, c, f)[f.key] ?? c[f.key]!;
  return `${luminance(moved) < luminance(c[f.key]!) ? "Darken" : "Lighten"} to ${mark}`;
}

/** One failing check fixed: the keys to set. A filled pair's ground moves, lightness only; anything else moves itself. */
function fixOne(_kind: ThemeKind, set: AppColours, c: Colours, f: Failure): AppColours {
  const ground = GROUND[f.key];
  if (ground) {
    // A filled control's words keep their side (white stays white): its ground moves away from them.
    const text = c[f.key]!;
    const lighter = luminance(text) < luminance(c[ground]!);
    let g = c[ground]!;
    const low = (v: string) => Math.min(contrast(text, v), f.key === "onPrimary" ? contrast(text, mix(v, c.sheet, 0.8)) : Infinity);
    for (let i = 0; i < 60 && low(g) < 4.5; i += 1) g = shift(g, lighter ? 0.01 : -0.01);
    if (low(g) >= 4.5) return { [ground]: g };
    return { [f.key]: contrast("#FFFFFF", c[ground]!) >= contrast("#000000", c[ground]!) ? "#FFFFFF" : "#000000" };
  }
  const surfaces = pairs(c).find(([k, , w]) => k === f.key && w === f.want)![1];
  return { [f.key]: fix(set[f.key] ?? c[f.key]!, surfaces, f.want) };
}

/** One row's fix: the theme's set colours with the row's failing checks fixed. */
export function fixRow(kind: ThemeKind, set: AppColours, keys: readonly AppKey[]): AppColours {
  let next = { ...set };
  for (let round = 0; round < 6; round += 1) {
    const { colours } = derive(kind, next);
    const bad = failures(colours).filter((f) => keys.includes(f.key));
    if (bad.length === 0) break;
    for (const f of bad) next = { ...next, ...fixOne(kind, next, colours, f) };
  }
  return next;
}

/** Every failing colour fixed by lightness alone, hue kept, up to six rounds, since one fix can move another row's ground. */
export function fixAll(kind: ThemeKind, set: AppColours): AppColours {
  let next = { ...set };
  for (let round = 0; round < 6; round += 1) {
    const { colours } = derive(kind, next);
    const bad = failures(colours);
    if (bad.length === 0) break;
    for (const f of bad) next = { ...next, ...fixOne(kind, next, colours, f) };
  }
  return next;
}

// ---------- states told apart ----------

/** How far apart two states must be, in OKLab, to read as two colours side by side. */
export const APART = 0.08;

/**
 * The five states a reader tells apart: the four tones, and Unknown, whose grey is also the window's quiet text, so it
 * stays where it is (a Fable ruling in the final suite, 2026-11-22: a state pill comes in five tones, and colour-blind
 * mode names a pair with Unknown too).
 */
export const STATES = [...TONES, "muted"] as const;
export type State = (typeof STATES)[number];

export const TONE_NAME: Record<State, string> = { ok: "Passed", warn: "Attention", err: "Failed", info: "Running", muted: "Unknown" };

export interface Closest {
  delta: number;
  under: "full colour vision" | Vision;
  a: State;
  b: State;
}

/** The closest two of the five states, over full colour vision and Machado's three simulations. */
export function closestStates(c: Pick<Colours, State>): Closest {
  let low: Closest = { delta: Infinity, under: "full colour vision", a: "ok", b: "warn" };
  for (let i = 0; i < STATES.length; i += 1)
    for (let j = i + 1; j < STATES.length; j += 1) {
      const a = apart(c[STATES[i]!], c[STATES[j]!]);
      if (a.delta < low.delta) low = { ...a, a: STATES[i]!, b: STATES[j]! };
    }
  return low;
}

/** The line under the States rows. */
export function apartLine(c: Pick<Colours, State>): string {
  const cs = closestStates(c);
  const who = cs.under === "full colour vision" ? "everyone" : cs.under === "tritan" ? "blue-yellow colour blindness" : "red-green colour blindness";
  const d = cs.delta.toFixed(2);
  return cs.delta >= APART
    ? `Told apart: the closest two, ${TONE_NAME[cs.a]} and ${TONE_NAME[cs.b]}, are ${d} apart.`
    : `${TONE_NAME[cs.a]} and ${TONE_NAME[cs.b]} are ${d} apart for ${who}. 0.08 reads as two colours.`;
}

/**
 * Passed, Attention and Failed moved, and Running in lightness only, as little as makes every two of the five states
 * {@link APART} or more under full colour vision and the three simulations, each still 4.5:1 on the page, the panel and
 * the inset. Unknown's grey stays: it is the window's quiet text too. A bounded search: Passed over nine hues (its own,
 * and 11, 21, 31 and 41 degrees either way), Attention and Failed over three (10 degrees either way), Running over its
 * own hue, each over up to six lightnesses of the band where it reads, at most 0.14 from where it is. Where nothing
 * reaches the mark, the furthest apart found.
 */
export function tellApart(c: Colours): { ok: string; warn: string; err: string; info: string; reached: number } {
  const now = closestStates(c).delta;
  if (now >= APART) return { ok: c.ok, warn: c.warn, err: c.err, info: c.info, reached: now };
  const surfaces = { paper: c.paper, sheet: c.sheet, sheet2: c.sheet2 };
  const rad = (d: number) => (d * Math.PI) / 180;
  type Cand = { hex: string; moved: number; sim: Triple[] };
  const sims = (hex: string) => [hex, ...CVD_KINDS.map((k) => simulate(hex, k))].map(toOklab);
  const cand = (key: "ok" | "warn" | "err" | "info", hues: number[]): Cand[] => {
    const [L, C, H] = toOklch(c[key]);
    const from = toOklab(c[key]);
    const out: Cand[] = [];
    for (const dh of hues) {
      const reads = (l: number) => worst(fromOklch([l, C, H + rad(dh)]), surfaces).ratio >= 4.5;
      const steps: number[] = [];
      for (let l = L - 0.14; l <= L + 0.1401; l += 0.02) if (l > 0.05 && l < 0.98 && reads(l)) steps.push(l);
      if (steps.length === 0) continue;
      const pick = steps.length <= 6 ? steps : Array.from({ length: 6 }, (_, i) => steps[Math.round((i * (steps.length - 1)) / 5)]!);
      for (const l of pick) {
        const hex = fromOklch([l, C, H + rad(dh)]);
        const lab = toOklab(hex);
        out.push({ hex, moved: Math.hypot(lab[0] - from[0], lab[1] - from[1], lab[2] - from[2]), sim: sims(hex) });
      }
    }
    return out;
  };
  const oks = cand("ok", [0, 11, -11, 21, -21, 31, -31, 41, -41]);
  const warns = cand("warn", [0, 10, -10]);
  const errs = cand("err", [0, 10, -10]);
  // Running keeps its hue and chroma: its lightness alone moves, and not at all while it already stands apart.
  const infos = [{ hex: c.info, moved: 0, sim: sims(c.info) }, ...cand("info", [0])];
  const muted = { sim: sims(c.muted) };
  const gap = (a: { sim: Triple[] }, b: { sim: Triple[] }) => {
    let low = Infinity;
    for (let i = 0; i < 4; i += 1) low = Math.min(low, Math.hypot(a.sim[i]![0] - b.sim[i]![0], a.sim[i]![1] - b.sim[i]![1], a.sim[i]![2] - b.sim[i]![2]));
    return low;
  };
  // Every distance once, before the search: the search itself is then minima and sums.
  const table = (xs: Cand[], ys: { sim: Triple[] }[]) => xs.map((x) => ys.map((y) => gap(x, y)));
  const ow = table(oks, warns);
  const oe = table(oks, errs);
  const we = table(warns, errs);
  const toMuted = (xs: Cand[]) => xs.map((x) => gap(x, muted));
  const [om, wm, em] = [toMuted(oks), toMuted(warns), toMuted(errs)];
  let best: { score: number; low: number; ok: string; warn: string; err: string; info: string } | null = null;
  for (const n of infos) {
    const nm = gap(n, muted);
    const [on, wn, en] = [oks, warns, errs].map((xs) => xs.map((x) => gap(x, n))) as [number[], number[], number[]];
    for (let i = 0; i < oks.length; i += 1) {
      const lo = Math.min(nm, on[i]!, om[i]!);
      for (let j = 0; j < warns.length; j += 1) {
        const lw = Math.min(lo, ow[i]![j]!, wn[j]!, wm[j]!);
        for (let k = 0; k < errs.length; k += 1) {
          const low = Math.min(lw, oe[i]![k]!, we[j]![k]!, en[k]!, em[k]!);
          const moved = oks[i]!.moved + warns[j]!.moved + errs[k]!.moved + n.moved;
          const score = low >= APART ? 10 - moved : low;
          if (!best || score > best.score) best = { score, low, ok: oks[i]!.hex, warn: warns[j]!.hex, err: errs[k]!.hex, info: n.hex };
        }
      }
    }
  }
  if (!best) return { ok: c.ok, warn: c.warn, err: c.err, info: c.info, reached: closestStates(c).delta };
  return { ok: best.ok, warn: best.warn, err: best.err, info: best.info, reached: best.low };
}

/** A colour's words in a low row: its name. */
export const nameOf = (key: AppKey) => NAME[key];
