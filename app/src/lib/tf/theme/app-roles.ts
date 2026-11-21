/**
 * The window's part of a theme (ADR-0055): the 51 colours a theme may set for a side, each one of `tokens.css`'s
 * tokens, and how Settings › Appearance › Theme lists them: 22 rows under seven headings, a fold of 19 that knobas
 * works out, and two never shown.
 */

/** Every key in the order the editor lists them, with the custom property it sets. */
export const APP_KEYS = [
  ["paper", "--paper"],
  ["sheet", "--sheet"],
  ["sheet2", "--sheet-2"],
  ["field", "--field"],
  ["ink", "--ink"],
  ["ink2", "--ink-2"],
  ["muted", "--muted"],
  ["rule", "--rule"],
  ["rule2", "--rule-2"],
  ["primary", "--primary"],
  ["onPrimary", "--on-primary"],
  ["fill", "--fill"],
  ["onFill", "--on-fill"],
  ["focus", "--focus"],
  ["link", "--link"],
  ["mark", "--mark"],
  ["selection", "--selection"],
  ["ok", "--ok"],
  ["warn", "--warn"],
  ["err", "--err"],
  ["info", "--info"],
  ["okBg", "--ok-bg"],
  ["warnBg", "--warn-bg"],
  ["errBg", "--err-bg"],
  ["infoBg", "--info-bg"],
  ["mutedBg", "--muted-bg"],
  ["card", "--card"],
  ["card2", "--card-2"],
  ["cardInk", "--card-ink"],
  ["cardInk2", "--card-ink-2"],
  // Fine tuning: derived unless a theme sets them.
  ["onInkOk", "--on-ink-ok"],
  ["onInkWarn", "--on-ink-warn"],
  ["onInkErr", "--on-ink-err"],
  ["onInkInfo", "--on-ink-info"],
  ["onInkMuted", "--on-ink-muted"],
  ["cardOk", "--card-ok"],
  ["cardWarn", "--card-warn"],
  ["cardErr", "--card-err"],
  ["cardInfo", "--card-info"],
  ["cardMuted", "--card-muted"],
  ["cardOkBg", "--card-ok-bg"],
  ["cardWarnBg", "--card-warn-bg"],
  ["cardErrBg", "--card-err-bg"],
  ["cardInfoBg", "--card-info-bg"],
  ["cardMutedBg", "--card-muted-bg"],
  ["cardMark", "--card-mark"],
  ["okWord", "--ok-word"],
  ["errWord", "--err-word"],
  ["scroll", "--scroll"],
  // What the card's translucent plates are mixed from: never shown in the editor.
  ["cardShade", "--card-shade"],
  ["cardLift", "--card-lift"],
] as const;

export type AppKey = (typeof APP_KEYS)[number][0];
export const KEYS: readonly AppKey[] = APP_KEYS.map(([key]) => key);
export const TOKEN: Record<AppKey, string> = Object.fromEntries(APP_KEYS) as Record<AppKey, string>;
export const isAppKey = (key: string): key is AppKey => (KEYS as readonly string[]).includes(key);

/** A colour's name as a person reads it: in a swatch's label, a low row's sentence, the pick menu. */
export const NAME: Record<AppKey, string> = {
  paper: "Page",
  sheet: "Panel",
  sheet2: "Inset",
  field: "Field",
  ink: "Text",
  ink2: "Secondary text",
  muted: "Faint text",
  rule: "Border",
  rule2: "Divider",
  primary: "Primary button",
  onPrimary: "Text on a primary button",
  fill: "Selected, pressed",
  onFill: "Text on what is selected",
  focus: "Focus ring",
  link: "Link",
  mark: "Highlight",
  selection: "Selected text",
  ok: "Passed",
  warn: "Attention",
  err: "Failed",
  info: "Running",
  okBg: "Passed's tint",
  warnBg: "Attention's tint",
  errBg: "Failed's tint",
  infoBg: "Running's tint",
  mutedBg: "Unknown's tint",
  card: "Card",
  card2: "Card, pressed",
  cardInk: "Text on the card",
  cardInk2: "Secondary text on the card",
  onInkOk: "Passed on a selected row",
  onInkWarn: "Attention on a selected row",
  onInkErr: "Failed on a selected row",
  onInkInfo: "Running on a selected row",
  onInkMuted: "Unknown on a selected row",
  cardOk: "Passed on the card",
  cardWarn: "Attention on the card",
  cardErr: "Failed on the card",
  cardInfo: "Running on the card",
  cardMuted: "Unknown on the card",
  cardOkBg: "Passed's tint on the card",
  cardWarnBg: "Attention's tint on the card",
  cardErrBg: "Failed's tint on the card",
  cardInfoBg: "Running's tint on the card",
  cardMutedBg: "Unknown's tint on the card",
  cardMark: "Highlight on the card",
  okWord: "An added word in a diff",
  errWord: "A removed word in a diff",
  scroll: "Scrollbar",
  cardShade: "Card shade",
  cardLift: "Card lift",
};

/** What a row's example draws. Each kind is one small template in `TfAppThemeEditor.svelte`. */
export type Example = "words" | "text" | "line" | "button" | "segment" | "ring" | "link" | "mark" | "selection" | "chip" | "unknown" | "card";

export interface Row {
  name: string;
  /** The ground first, then its text, where a row holds both. */
  keys: readonly AppKey[];
  example: Example;
  /** The example's words. */
  words: string;
}

export interface Heading {
  name: string;
  /** The small print beside the heading: which figure the rows show. */
  note: string;
  rows: readonly Row[];
}

/** The editor's rows: 30 keys in 22 rows under seven headings (the decisions' table; its text says 21). */
export const HEADINGS: readonly Heading[] = [
  {
    name: "Surfaces",
    note: "",
    rows: [
      { name: "Page", keys: ["paper"], example: "words", words: "Behind everything" },
      { name: "Panel", keys: ["sheet"], example: "words", words: "Bars, cards, dialogs, code" },
      { name: "Inset", keys: ["sheet2"], example: "words", words: "Table heads, a row under the pointer" },
      { name: "Field", keys: ["field"], example: "words", words: "What you type into" },
    ],
  },
  {
    name: "Text",
    note: "lowest contrast on any surface",
    rows: [
      { name: "Text", keys: ["ink"], example: "text", words: "A ticket's title" },
      { name: "Secondary text", keys: ["ink2"], example: "text", words: "synced 4 min ago" },
      { name: "Faint text", keys: ["muted"], example: "text", words: "Filter tickets" },
    ],
  },
  {
    name: "Lines",
    note: "",
    rows: [
      { name: "Border", keys: ["rule"], example: "line", words: "Around panels and fields" },
      { name: "Divider", keys: ["rule2"], example: "line", words: "Between rows" },
    ],
  },
  {
    name: "Controls",
    note: "",
    rows: [
      { name: "Primary button", keys: ["primary", "onPrimary"], example: "button", words: "Create ticket" },
      { name: "Selected, pressed", keys: ["fill", "onFill"], example: "segment", words: "Board" },
      { name: "Focus ring", keys: ["focus"], example: "ring", words: "The keyboard's" },
      { name: "Link", keys: ["link"], example: "link", words: "release notes" },
    ],
  },
  {
    name: "Marks",
    note: "",
    rows: [
      { name: "Highlight", keys: ["mark"], example: "mark", words: "retry" },
      { name: "Selected text", keys: ["selection"], example: "selection", words: "selected" },
    ],
  },
  {
    name: "States",
    note: "a state and its tint",
    rows: [
      { name: "Passed", keys: ["ok", "okBg"], example: "chip", words: "Passed" },
      { name: "Attention", keys: ["warn", "warnBg"], example: "chip", words: "Stale" },
      { name: "Failed", keys: ["err", "errBg"], example: "chip", words: "Failed" },
      { name: "Running", keys: ["info", "infoBg"], example: "chip", words: "Running" },
      { name: "Unknown", keys: ["mutedBg"], example: "unknown", words: "Unknown" },
    ],
  },
  {
    name: "Current ticket",
    note: "",
    rows: [
      { name: "Card", keys: ["card", "cardInk"], example: "card", words: "PAY-231" },
      { name: "Card, pressed", keys: ["card2", "cardInk2"], example: "card", words: "Settlement" },
    ],
  },
];

/** The fold: *Fine tuning: 19 colours knobas works out from the ones above*. */
export const FOLD: readonly AppKey[] = [
  "onInkOk",
  "onInkWarn",
  "onInkErr",
  "onInkInfo",
  "onInkMuted",
  "cardOk",
  "cardWarn",
  "cardErr",
  "cardInfo",
  "cardMuted",
  "cardOkBg",
  "cardWarnBg",
  "cardErrBg",
  "cardInfoBg",
  "cardMutedBg",
  "cardMark",
  "okWord",
  "errWord",
  "scroll",
];

/** Never shown: what the card's plates are mixed from. */
export const HIDDEN: readonly AppKey[] = ["cardShade", "cardLift"];

/** The row keys, in the order the editor lists them. */
export const ROW_KEYS: readonly AppKey[] = HEADINGS.flatMap((h) => h.rows.flatMap((r) => r.keys));

/** The eight chips a slot's card shows. */
export const CHIPS: readonly AppKey[] = ["paper", "sheet", "ink", "primary", "ok", "warn", "err", "card"];
