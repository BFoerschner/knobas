/**
 * The themes that come with knobas (ADR-0051, ADR-0055; `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`,
 * `docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`). Read-only. Each has both parts.
 *
 * The knobas pair is the window's own palette (an empty `app`: every colour is knobas's, `app-derive.ts`) and the Code
 * area mockup's six colours spread over the seventeen roles (`tf/css/tokens.css` holds the same colours as its
 * fallbacks). The other five are MIT-licensed VS Code themes (primer/github-vscode-theme, Binaryify/OneDark-Pro,
 * dracula/visual-studio-code, catppuccin/vscode). Their code colours are read onto the roles from the copies Shiki
 * ships and changed where a role fell under 4.5:1 on knobas's surfaces or their own: GitHub Light's comment, keyword,
 * tag and operator darker, the comment of GitHub Dark, One Dark and Dracula, GitHub Dark's keyword, One Dark's keyword
 * and red lighter. Their window colours are what `appFromVsCode` reads from the same copies, then *Fix all*, with
 * nothing changed by hand (`builtins.test.ts` holds that). Every one keeps knobas's yellow card. None sets diff colours
 * of its own: they take the window's tints.
 */
import type { Theme } from "./themes";

export const BUILTINS: readonly Theme[] = [
  { id: "knobas-light", name: "knobas Light", kind: "light", app: {}, syntax: { com: "#5F6975", kw: "#7A3EB1", str: "#1F7444", esc: "#8A5600", num: "#8A5600", con: "#8A5600", fn: "#2F55D4", ty: "#A2461B", key: "#A2461B", tag: "#A2461B", attr: "#8A5600", head: "#7A3EB1", link: "#2F55D4" } },
  { id: "knobas-dark", name: "knobas Dark", kind: "dark", app: {}, syntax: { com: "#94A0AC", kw: "#C79BF2", str: "#6FCF97", esc: "#E5B04D", num: "#E5B04D", con: "#E5B04D", fn: "#8AA6FF", ty: "#F0A274", key: "#F0A274", tag: "#F0A274", attr: "#E5B04D", head: "#C79BF2", link: "#8AA6FF" } },
  { id: "github-light", name: "GitHub Light", kind: "light", app: { sheet: "#FFFFFF", ink: "#24292E", paper: "#F6F8FA", sheet2: "#EBF0F4", field: "#FAFBFC", ink2: "#666E78", rule: "#E1E4E8", primary: "#006423", onPrimary: "#FFFFFF", focus: "#2188FF", link: "#0366D6", mark: "#FFF2BE", selection: "#DAE9F9", ok: "#007E2A", warn: "#886800", err: "#CC2F41", info: "#0366D6" }, syntax: { com: "#5E6771", kw: "#C3243A", str: "#032F62", esc: "#005CC5", num: "#005CC5", con: "#005CC5", fn: "#6F42C1", ty: "#6F42C1", key: "#005CC5", tag: "#04762B", attr: "#6F42C1", op: "#C3243A", head: "#005CC5", link: "#032F62" } },
  { id: "github-dark", name: "GitHub Dark", kind: "dark", app: { sheet: "#24292E", ink: "#E1E4E8", paper: "#1F2428", sheet2: "#282E34", field: "#2F363D", ink2: "#969EA6", focus: "#1E6FD9", link: "#79B8FF", mark: "#414030", selection: "#284566", ok: "#34D058", warn: "#FFEA7F", err: "#FD5C69", info: "#3F94FF" }, syntax: { com: "#8D97A1", kw: "#FA7684", str: "#9ECBFF", esc: "#79B8FF", num: "#79B8FF", con: "#79B8FF", fn: "#B392F0", ty: "#B392F0", key: "#79B8FF", tag: "#85E89D", attr: "#B392F0", op: "#FA7684", head: "#79B8FF", link: "#DBEDFF" } },
  { id: "one-dark", name: "One Dark", kind: "dark", app: { sheet: "#282C34", ink: "#ABB2BF", paper: "#21252B", sheet2: "#2C313A", field: "#1D1F23", rule: "#181A1F", link: "#61AFEF", ok: "#8CC265", warn: "#D18F52", err: "#F76A74", info: "#4AA5F0" }, syntax: { com: "#9298A2", kw: "#D586ED", str: "#98C379", esc: "#56B6C2", num: "#D19A66", con: "#D19A66", fn: "#61AFEF", ty: "#E5C07B", prop: "#F78189", key: "#ABB2BF", tag: "#F78189", attr: "#D19A66", var: "#F78189", op: "#ABB2BF", head: "#F78189", link: "#98C379" } },
  { id: "dracula", name: "Dracula", kind: "dark", app: { sheet: "#282A36", ink: "#F8F8F2", paper: "#21222C", sheet2: "#353747", field: "#282A36", rule: "#191A21", focus: "#6272A4", mark: "#5E5F68", selection: "#44475A", ok: "#50FA7B", warn: "#F1FA8C", err: "#FF7670", info: "#BD93F9" }, syntax: { com: "#8DA0D4", kw: "#FF79C6", str: "#F1FA8C", esc: "#FF79C6", num: "#BD93F9", con: "#BD93F9", fn: "#50FA7B", ty: "#8BE9FD", key: "#8BE9FD", tag: "#FF79C6", attr: "#50FA7B", var: "#FFB86C", op: "#FF79C6", head: "#BD93F9", link: "#8BE9FD" } },
  { id: "catppuccin-mocha", name: "Catppuccin Mocha", kind: "dark", app: { sheet: "#1E1E2E", ink: "#CDD6F4", paper: "#181825", sheet2: "#282839", field: "#313244", primary: "#CBA6F7", onPrimary: "#11111B", focus: "#CBA6F7", link: "#89B4FA", mark: "#3E5767", selection: "#3B3D4F", ok: "#A6E3A1", warn: "#F9E2AF", err: "#F38BA8", info: "#89B4FA" }, syntax: { com: "#9399B2", kw: "#CBA6F7", str: "#A6E3A1", esc: "#F5C2E7", num: "#FAB387", con: "#F38BA8", fn: "#89B4FA", ty: "#F9E2AF", key: "#F9E2AF", tag: "#89B4FA", attr: "#F9E2AF", var: "#EBA0AC", op: "#94E2D5", punct: "#9399B2", link: "#89B4FA" } },
];

/** The theme each slot holds until a person picks another. */
export const DEFAULT_THEME = { light: "knobas-light", dark: "knobas-dark" } as const;
