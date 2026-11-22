/**
 * The themes that come with knobas (ADR-0051, ADR-0055; `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`,
 * `docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`). Read-only. Each has both parts.
 *
 * The knobas pair is the window's own palette (an empty `app`: every colour is knobas's, `app-derive.ts`) and the Code
 * area mockup's six colours spread over the seventeen roles (`tf/css/tokens.css` holds the same colours as its
 * fallbacks). The other five are MIT-licensed VS Code themes (primer/github-vscode-theme, Binaryify/OneDark-Pro,
 * dracula/visual-studio-code, catppuccin/vscode). Their code colours are read onto the roles from the copies Shiki
 * ships and changed where a role fell under 4.5:1 on knobas's surfaces: GitHub Light's comment, keyword, tag and operator
 * darker, and the comment of GitHub Dark, One Dark and Dracula and One Dark's red lighter. On their own window
 * surfaces a few fall under again (Björn, 2026-11-22: keep the theme's own colours); the editor marks them and its
 * *Fix all* lightens them in a copy. Their window colours are what `appFromVsCode` reads from the same copies, then *Fix all*, with
 * nothing changed by hand (`builtins.test.ts` holds that). Each brings its own card, derived from its Attention; knobas's own keep the yellow. A pair for colour blindness
 * follows knobas's two. None sets diff colours
 * of its own: they take the window's tints.
 */
import type { Theme } from "./themes";

export const BUILTINS: readonly Theme[] = [
  { id: "knobas-light", name: "knobas Light", kind: "light", app: {}, syntax: { com: "#5F6975", kw: "#7A3EB1", str: "#1F7444", esc: "#8A5600", num: "#8A5600", con: "#8A5600", fn: "#2F55D4", ty: "#A2461B", key: "#A2461B", tag: "#A2461B", attr: "#8A5600", head: "#7A3EB1", link: "#2F55D4" } },
  { id: "knobas-dark", name: "knobas Dark", kind: "dark", app: {}, syntax: { com: "#94A0AC", kw: "#C79BF2", str: "#6FCF97", esc: "#E5B04D", num: "#E5B04D", con: "#E5B04D", fn: "#8AA6FF", ty: "#F0A274", key: "#F0A274", tag: "#F0A274", attr: "#E5B04D", head: "#C79BF2", link: "#8AA6FF" } },
  // knobas for colour blindness (Björn, 2026-11-22): Passed, Attention and Failed moved by *Tell them apart* until every
  // two states are 0.08 apart under Machado (2009)'s three simulations, each still 4.5:1. The card stays knobas's yellow.
  { id: "knobas-light-cb", name: "knobas Light for colour blindness", kind: "light", app: { ok: "#00604F", warn: "#905C0C", err: "#9B0618", card: "#F2D157", card2: "#E8C440", cardInk: "#2A230B", cardInk2: "#5A4D1E", mark: "#F6E08F" }, syntax: { com: "#5F6975", kw: "#7A3EB1", str: "#00604F", esc: "#8A5600", num: "#8A5600", con: "#8A5600", fn: "#2F55D4", ty: "#A2461B", key: "#A2461B", tag: "#A2461B", attr: "#8A5600", head: "#7A3EB1", link: "#2F55D4" } },
  { id: "knobas-dark-cb", name: "knobas Dark for colour blindness", kind: "dark", app: { ok: "#52D8BD", warn: "#F2BD5B", card: "#D9B941", card2: "#C9A930", cardInk: "#1D1806", cardInk2: "#3F3511", mark: "#5C4E17" }, syntax: { com: "#94A0AC", kw: "#C79BF2", str: "#52D8BD", esc: "#F2BD5B", num: "#F2BD5B", con: "#F2BD5B", fn: "#8AA6FF", ty: "#F0A274", key: "#F0A274", tag: "#F0A274", attr: "#F2BD5B", head: "#C79BF2", link: "#8AA6FF" } },
  { id: "github-light", name: "GitHub Light", kind: "light", app: { sheet: "#FFFFFF", ink: "#24292E", paper: "#F6F8FA", sheet2: "#EBF0F4", field: "#FAFBFC", ink2: "#666E78", rule: "#E1E4E8", primary: "#006423", onPrimary: "#FFFFFF", focus: "#2188FF", link: "#0366D6", mark: "#FFF2BE", selection: "#DAE9F9", ok: "#007E2A", warn: "#886800", err: "#CC2F41", info: "#0366D6" }, syntax: { com: "#5E6771", kw: "#C3243A", str: "#032F62", esc: "#005CC5", num: "#005CC5", con: "#005CC5", fn: "#6F42C1", ty: "#6F42C1", key: "#005CC5", tag: "#04762B", attr: "#6F42C1", op: "#C3243A", head: "#005CC5", link: "#032F62" } },
  { id: "github-dark", name: "GitHub Dark", kind: "dark", app: { sheet: "#24292E", ink: "#E1E4E8", paper: "#1F2428", sheet2: "#282E34", field: "#2F363D", ink2: "#969EA6", focus: "#1E6FD9", link: "#79B8FF", mark: "#414030", selection: "#284566", ok: "#34D058", warn: "#FFEA7F", err: "#FD5C69", info: "#3F94FF" }, syntax: { com: "#8D97A1", kw: "#F97583", str: "#9ECBFF", esc: "#79B8FF", num: "#79B8FF", con: "#79B8FF", fn: "#B392F0", ty: "#B392F0", key: "#79B8FF", tag: "#85E89D", attr: "#B392F0", op: "#F97583", head: "#79B8FF", link: "#DBEDFF" } },
  { id: "one-dark", name: "One Dark", kind: "dark", app: { sheet: "#282C34", ink: "#ABB2BF", paper: "#21252B", sheet2: "#2C313A", field: "#1D1F23", rule: "#181A1F", link: "#61AFEF", ok: "#8CC265", warn: "#D18F52", err: "#F76A74", info: "#4AA5F0" }, syntax: { com: "#9096A0", kw: "#C678DD", str: "#98C379", esc: "#56B6C2", num: "#D19A66", con: "#D19A66", fn: "#61AFEF", ty: "#E5C07B", prop: "#E6727A", key: "#ABB2BF", tag: "#E6727A", attr: "#D19A66", var: "#E6727A", op: "#ABB2BF", head: "#E6727A", link: "#98C379" } },
  { id: "dracula", name: "Dracula", kind: "dark", app: { sheet: "#282A36", ink: "#F8F8F2", paper: "#21222C", sheet2: "#353747", field: "#282A36", rule: "#191A21", focus: "#6272A4", mark: "#5E5F68", selection: "#44475A", ok: "#50FA7B", warn: "#F1FA8C", err: "#FF7670", info: "#BD93F9" }, syntax: { com: "#8395C9", kw: "#FF79C6", str: "#F1FA8C", esc: "#FF79C6", num: "#BD93F9", con: "#BD93F9", fn: "#50FA7B", ty: "#8BE9FD", key: "#8BE9FD", tag: "#FF79C6", attr: "#50FA7B", var: "#FFB86C", op: "#FF79C6", head: "#BD93F9", link: "#8BE9FD" } },
  { id: "catppuccin-mocha", name: "Catppuccin Mocha", kind: "dark", app: { sheet: "#1E1E2E", ink: "#CDD6F4", paper: "#181825", sheet2: "#282839", field: "#313244", primary: "#CBA6F7", onPrimary: "#11111B", focus: "#CBA6F7", link: "#89B4FA", mark: "#3E5767", selection: "#3B3D4F", ok: "#A6E3A1", warn: "#F9E2AF", err: "#F38BA8", info: "#89B4FA" }, syntax: { com: "#9399B2", kw: "#CBA6F7", str: "#A6E3A1", esc: "#F5C2E7", num: "#FAB387", con: "#F38BA8", fn: "#89B4FA", ty: "#F9E2AF", key: "#F9E2AF", tag: "#89B4FA", attr: "#F9E2AF", var: "#EBA0AC", op: "#94E2D5", punct: "#9399B2", link: "#89B4FA" } },
];

/** The theme each slot holds until a person picks another. */
export const DEFAULT_THEME = { light: "knobas-light", dark: "knobas-dark" } as const;
