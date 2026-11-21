/**
 * Every colour a theme leaves out, worked out from the ones it sets (ADR-0055; the decisions' 3.4).
 *
 * `derive(kind, set)` answers all 51 colours and which of them were derived. A colour the theme leaves out is
 * knobas's own value for that side wherever every colour it is worked out from is knobas's own too, so knobas Light
 * and knobas Dark are the empty set, and a copy of knobas Light with a purple Passed gets a purple tint, a purple word
 * mark and a purple on a filled row and keeps knobas's page, borders and card to the digit. Otherwise each key has a
 * rule in OKLCH, fitted to knobas's own palette: from knobas's panel and text alone the other colours land a mean
 * OKLab distance of 0.006 from knobas's real ones.
 *
 * Runs in the page, writes plain `#RRGGBB`, about 0.2 ms a theme.
 */
import type { AppKey } from "./app-roles";
import { contrast, fix, fromOklch, luminance, mix, shift, toOklch } from "./colour";
import type { ThemeKind } from "./themes";

export type AppColours = Partial<Record<AppKey, string>>;
/** A side's colours, every key but `selection`, which is never derived. */
export type Colours = Record<Exclude<AppKey, "selection">, string> & { selection?: string };

/** knobas's own two themes, as `tokens.css` holds them for its fallbacks (`app-derive.test.ts` holds the two equal). */
export const KNOBAS: Record<ThemeKind, Colours> = {
  light: {
    paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", field: "#FFFFFF", ink: "#17212B", ink2: "#4B5663", muted: "#5F6975",
    rule: "#CBD2D9", rule2: "#DDE2E7", primary: "#17212B", onPrimary: "#F7F8F9", fill: "#17212B", onFill: "#F7F8F9", focus: "#2F55D4", link: "#2F55D4", mark: "#F6E08F",
    ok: "#1F7444", warn: "#8A5600", err: "#B0262A", info: "#2F55D4",
    okBg: "#DDEFE4", warnBg: "#F6EBD3", errBg: "#F7DEDD", infoBg: "#DFE6FA", mutedBg: "#E6E9EC",
    card: "#F2D157", card2: "#E8C440", cardInk: "#2A230B", cardInk2: "#5A4D1E",
    onInkOk: "#6FCF97", onInkWarn: "#E5B04D", onInkErr: "#FF8A80", onInkInfo: "#8AA6FF", onInkMuted: "#94A0AC",
    cardOk: "#1F7444", cardWarn: "#8A5600", cardErr: "#B0262A", cardInfo: "#2F55D4", cardMuted: "#5F6975",
    cardOkBg: "#DDEFE4", cardWarnBg: "#F6EBD3", cardErrBg: "#F7DEDD", cardInfoBg: "#DFE6FA", cardMutedBg: "#E6E9EC",
    cardMark: "#F6E08F", okWord: "#B4DEC3", errWord: "#EFB9B6", scroll: "#CBD2D9", cardShade: "#000000", cardLift: "#FFFFFF",
  },
  dark: {
    paper: "#12171C", sheet: "#1A2027", sheet2: "#212932", field: "#12171C", ink: "#E4E9EE", ink2: "#A0ABB7", muted: "#94A0AC",
    rule: "#323C47", rule2: "#28313A", primary: "#A9B3BE", onPrimary: "#1A2027", fill: "#A9B3BE", onFill: "#1A2027", focus: "#8AA6FF", link: "#8AA6FF", mark: "#5C4E17",
    ok: "#6FCF97", warn: "#E5B04D", err: "#FF8A80", info: "#8AA6FF",
    okBg: "#1B3427", warnBg: "#3A2E14", errBg: "#3F1F1F", infoBg: "#1F2A4A", mutedBg: "#262E37",
    card: "#D9B941", card2: "#C9A930", cardInk: "#1D1806", cardInk2: "#3F3511",
    onInkOk: "#0F3F24", onInkWarn: "#4A2E00", onInkErr: "#6E1014", onInkInfo: "#17297A", onInkMuted: "#2C333B",
    cardOk: "#1F7444", cardWarn: "#8A5600", cardErr: "#B0262A", cardInfo: "#2F55D4", cardMuted: "#5D6773",
    cardOkBg: "#DDEFE4", cardWarnBg: "#F6EBD3", cardErrBg: "#F7DEDD", cardInfoBg: "#DFE6FA", cardMutedBg: "#E6E9EC",
    cardMark: "#F6E08F", okWord: "#2C6243", errWord: "#6E3333", scroll: "#323C47", cardShade: "#000000", cardLift: "#FFFFFF",
  },
};

export const TONES = ["ok", "warn", "err", "info"] as const;
export type Tone = (typeof TONES)[number];
const cap = <T extends string>(s: T) => `${s[0]!.toUpperCase()}${s.slice(1)}` as Capitalize<T>;

/** A state's tint on the panel: its hue a step from the panel's lightness, stepped back until the state and the text read on it. */
export function tint(tone: string, sheet: string, ink: string, dark: boolean): string {
  const [, C, H] = toOklch(tone);
  const sl = toOklch(sheet)[0];
  let step = dark ? 0.055 : -0.047;
  let out = sheet;
  for (let i = 0; i < 8; i += 1) {
    out = fromOklch([sl + step, dark ? Math.min(C * 0.38, 0.06) : Math.min(C * 0.24, 0.034), H]);
    if (contrast(tone, out) >= 4.5 && contrast(ink, out) >= 4.5) break;
    step *= 0.7;
  }
  return out;
}

/** Every colour of a side: what `set` holds, and for each key it leaves out one derived from the ones it holds. */
export function derive(kind: ThemeKind, set: AppColours = {}): { colours: Colours; derived: Set<AppKey> } {
  const dark = kind === "dark";
  const K = KNOBAS[kind];
  const c = {} as Colours;
  const derived = new Set<AppKey>();
  // The keys that hold knobas's own value because the theme set neither them nor anything they are worked out from.
  const own = new Set<AppKey>();
  const has = (k: AppKey) => typeof set[k] === "string";
  const k = (key: Exclude<AppKey, "selection">, inputs: readonly AppKey[], make: () => string) => {
    if (has(key)) {
      c[key] = set[key]!;
      return;
    }
    derived.add(key);
    if (inputs.every((i) => own.has(i))) {
      c[key] = K[key];
      own.add(key);
    } else c[key] = make();
  };

  k("sheet", [], () => K.sheet);
  k("ink", ["sheet"], () => (luminance(c.sheet) > 0.4 ? "#17212B" : "#E4E9EE"));
  // A derived surface is the panel moved a step in lightness. Where the text reads on the panel at 4.5:1, the step is
  // halved until it reads on the surface too, so a theme with soft text keeps it readable on the page and the inset.
  const beside = (by: number) => {
    let out = shift(c.sheet, by);
    if (contrast(c.ink, c.sheet) < 4.5) return out;
    for (let i = 0; i < 6 && contrast(c.ink, out) < 4.5; i += 1) {
      by /= 2;
      out = shift(c.sheet, by);
    }
    return contrast(c.ink, out) >= 4.5 ? out : c.sheet;
  };
  k("paper", ["sheet", "ink"], () => beside(dark ? -0.038 : -0.049));
  k("sheet2", ["sheet", "ink"], () => beside(dark ? 0.037 : -0.022));
  k("field", ["sheet", "paper", "ink"], () => (dark ? c.paper : beside(0.03)));
  const grounds = { paper: c.paper, sheet: c.sheet, "sheet-2": c.sheet2, field: c.field };
  const GROUNDS: AppKey[] = ["paper", "sheet", "sheet2", "field"];
  k("ink2", ["ink", ...GROUNDS], () => fix(mix(c.ink, c.sheet, 0.72), grounds));
  k("mutedBg", ["sheet", "ink"], () => beside(dark ? 0.056 : -0.046));
  k("muted", ["ink", "mutedBg", ...GROUNDS], () => fix(mix(c.ink, c.sheet, 0.62), { ...grounds, "muted tint": c.mutedBg }));
  k("rule", ["ink", "sheet"], () => mix(c.ink, c.sheet, 0.16));
  k("rule2", ["rule", "sheet"], () => mix(c.rule, c.sheet, 0.6));
  k("fill", ["ink", "sheet"], () => {
    // The ink in light, three quarters of the way to it in dark; then as far towards the ink as lets the panel's colour
    // be read on it and on a primary button under the pointer (80 % of it), and 3:1 off the page and the panel.
    let share = dark ? 0.75 : 1;
    const at = (s: number) => (s >= 1 ? c.ink : mix(c.ink, c.sheet, s));
    let f = at(share);
    const reads = (v: string) => contrast(c.sheet, mix(v, c.sheet, 0.8)) >= 4.5;
    while (share < 1 && !reads(f)) {
      share = Math.min(1, share + 0.05);
      f = at(share);
    }
    // Past the ink itself, away from the panel, where the text is too soft for a fill of its own colour.
    for (let i = 0; i < 20 && !reads(f); i += 1) f = shift(f, luminance(c.sheet) > luminance(f) ? -0.02 : 0.02);
    return fix(f, { paper: c.paper, sheet: c.sheet }, 3);
  });
  k("onFill", ["fill", "sheet", "ink"], () => {
    for (const pick of [c.sheet, c.ink]) if (contrast(pick, c.fill) >= 4.5) return pick;
    return contrast("#FFFFFF", c.fill) >= contrast("#000000", c.fill) ? "#FFFFFF" : "#000000";
  });
  // The primary button is the fill until a theme gives it a colour of its own.
  k("primary", ["fill"], () => c.fill);
  k("onPrimary", ["primary", "onFill"], () => {
    if (c.primary === c.fill) return c.onFill;
    for (const pick of ["#FFFFFF", c.sheet, c.ink, "#000000"]) if (contrast(pick, c.primary) >= 4.5) return pick;
    return contrast("#FFFFFF", c.primary) >= contrast("#000000", c.primary) ? "#FFFFFF" : "#000000";
  });
  for (const t of TONES) k(t, ["paper", "sheet", "sheet2"], () => fix(K[t], { paper: c.paper, sheet: c.sheet, "sheet-2": c.sheet2 }));
  for (const t of TONES) k(`${t}Bg`, [t, "sheet", "ink"], () => tint(c[t], c.sheet, c.ink, dark));
  k("focus", ["info", "paper", "sheet"], () => fix(c.info, { paper: c.paper, sheet: c.sheet }, 3));
  k("link", ["info"], () => c.info);
  k("card", [], () => K.card);
  k("card2", ["card"], () => shift(c.card, -0.04));
  const cardLight = luminance(c.card) > 0.3;
  const away = cardLight ? "darker" : "lighter";
  k("cardInk", ["card", "card2"], () => {
    const [, C, H] = toOklch(c.card);
    return fix(fromOklch([cardLight ? 0.26 : 0.95, Math.min(C, 0.05), H]), { card: c.card, "card-2": c.card2 }, 7, away);
  });
  k("cardInk2", ["cardInk", "card", "card2"], () => fix(mix(c.cardInk, c.card, 0.7), { card: c.card, "card-2": c.card2 }, 4.5, away));
  k("mark", ["card", "sheet", "ink"], () => {
    // A highlight behind text in ink: the card's hue, lighter than the card in light and a step off the panel in dark,
    // as knobas's own is; mixed towards the panel until the text reads on it.
    const [cl, cc, ch] = toOklch(c.card);
    let m = dark ? fromOklch([toOklch(c.sheet)[0] + 0.186, cc * 0.54, ch]) : fromOklch([Math.min(0.95, cl + 0.04), cc * 0.73, ch]);
    for (let i = 0; i < 12 && contrast(c.ink, m) < 4.5; i += 1) m = mix(m, c.sheet, 0.8);
    return m;
  });
  // Left out, selected text is the system's own highlight, so it is never derived.
  if (has("selection")) c.selection = set.selection!;
  // The states on a fill (a rail's selected row): as light as knobas Dark's states on a dark fill, as dark as its
  // on-ink ones on a light fill.
  const fillDark = luminance(c.fill) < 0.3;
  for (const t of [...TONES, "muted"] as const)
    k(`onInk${cap(t)}`, [t, "fill"], () => {
      const [, C0, H0] = toOklch(c[t]);
      return fix(fromOklch([fillDark ? 0.78 : 0.33, C0, H0]), { fill: c.fill }, 4.5, fillDark ? "lighter" : "darker");
    });
  // The states on the card: the light theme's where the card is light, the dark theme's where it is dark.
  const C = KNOBAS[cardLight ? "light" : "dark"];
  for (const t of [...TONES, "muted"] as const) {
    k(`card${cap(t)}Bg`, ["card"], () => C[`${t}Bg`]);
    // A dot on the card needs 3:1, a chip's word on its tint 4.5:1.
    k(`card${cap(t)}`, ["card", `card${cap(t)}Bg`], () => fix(fix(C[t], { card: c.card }, 3, away), { tint: c[`card${cap(t)}Bg`] }, 4.5, away));
  }
  k("cardMark", ["card"], () => C.mark);
  // A changed word's mark in a diff: the state's hue, further from the panel than its tint, stepped back until the
  // text reads on it.
  for (const [key, t] of [
    ["okWord", "ok"],
    ["errWord", "err"],
  ] as const)
    k(key, [t, "sheet", "ink"], () => {
      const [, C0, H0] = toOklch(c[t]);
      const sl = toOklch(c.sheet)[0];
      let step = dark ? 0.2 : -0.12;
      let out = c.sheet;
      for (let i = 0; i < 8; i += 1) {
        out = fromOklch([sl + step, dark ? Math.min(C0 * 0.62, 0.09) : Math.min(C0 * 0.5, 0.07), H0]);
        if (contrast(c.ink, out) >= 4.5) break;
        step *= 0.8;
      }
      return out;
    });
  k("scroll", ["rule"], () => c.rule);
  // A plate on the card darkens a light card and lightens a dark one.
  k("cardShade", ["card"], () => (cardLight ? "#000000" : "#FFFFFF"));
  k("cardLift", ["card"], () => (cardLight ? "#FFFFFF" : "#000000"));
  return { colours: c, derived };
}
