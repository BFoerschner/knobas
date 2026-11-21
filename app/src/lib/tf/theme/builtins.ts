/**
 * The themes that come with knobas (ADR-0051; `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`).
 * Read-only. A role a theme leaves out draws in ink.
 *
 * The knobas pair is the Code area mockup's six colours spread over the seventeen roles (`tf/css/tokens.css` holds the
 * same colours as its fallbacks). The other five are MIT-licensed VS Code themes (primer/github-vscode-theme,
 * Binaryify/OneDark-Pro, dracula/visual-studio-code, catppuccin/vscode), read onto the roles from the copies Shiki ships
 * and changed where a role fell under 4.5:1 on knobas's surfaces: GitHub Light's comment, keyword, tag and operator
 * darker, and the comment of GitHub Dark, One Dark and Dracula and One Dark's red lighter. None sets diff colours of its
 * own: they take knobas's tints.
 */
import type { Theme } from "./themes";

export const BUILTINS: readonly Theme[] = [
  { id: "knobas-light", name: "knobas Light", kind: "light", syntax: { com: "#5F6975", kw: "#7A3EB1", str: "#1F7444", esc: "#8A5600", num: "#8A5600", con: "#8A5600", fn: "#2F55D4", ty: "#A2461B", key: "#A2461B", tag: "#A2461B", attr: "#8A5600", head: "#7A3EB1", link: "#2F55D4" } },
  { id: "knobas-dark", name: "knobas Dark", kind: "dark", syntax: { com: "#94A0AC", kw: "#C79BF2", str: "#6FCF97", esc: "#E5B04D", num: "#E5B04D", con: "#E5B04D", fn: "#8AA6FF", ty: "#F0A274", key: "#F0A274", tag: "#F0A274", attr: "#E5B04D", head: "#C79BF2", link: "#8AA6FF" } },
  { id: "github-light", name: "GitHub Light", kind: "light", syntax: { com: "#5E6771", kw: "#C3243A", str: "#032F62", esc: "#005CC5", num: "#005CC5", con: "#005CC5", fn: "#6F42C1", ty: "#6F42C1", key: "#005CC5", tag: "#04762B", attr: "#6F42C1", op: "#C3243A", head: "#005CC5", link: "#032F62" } },
  { id: "github-dark", name: "GitHub Dark", kind: "dark", syntax: { com: "#8D97A1", kw: "#F97583", str: "#9ECBFF", esc: "#79B8FF", num: "#79B8FF", con: "#79B8FF", fn: "#B392F0", ty: "#B392F0", key: "#79B8FF", tag: "#85E89D", attr: "#B392F0", op: "#F97583", head: "#79B8FF", link: "#DBEDFF" } },
  { id: "one-dark", name: "One Dark", kind: "dark", syntax: { com: "#9096A0", kw: "#C678DD", str: "#98C379", esc: "#56B6C2", num: "#D19A66", con: "#D19A66", fn: "#61AFEF", ty: "#E5C07B", prop: "#E6727A", key: "#ABB2BF", tag: "#E6727A", attr: "#D19A66", var: "#E6727A", op: "#ABB2BF", head: "#E6727A", link: "#98C379" } },
  { id: "dracula", name: "Dracula", kind: "dark", syntax: { com: "#8395C9", kw: "#FF79C6", str: "#F1FA8C", esc: "#FF79C6", num: "#BD93F9", con: "#BD93F9", fn: "#50FA7B", ty: "#8BE9FD", key: "#8BE9FD", tag: "#FF79C6", attr: "#50FA7B", var: "#FFB86C", op: "#FF79C6", head: "#BD93F9", link: "#8BE9FD" } },
  { id: "catppuccin-mocha", name: "Catppuccin Mocha", kind: "dark", syntax: { com: "#9399B2", kw: "#CBA6F7", str: "#A6E3A1", esc: "#F5C2E7", num: "#FAB387", con: "#F38BA8", fn: "#89B4FA", ty: "#F9E2AF", key: "#F9E2AF", tag: "#89B4FA", attr: "#F9E2AF", var: "#EBA0AC", op: "#94E2D5", punct: "#9399B2", link: "#89B4FA" } },
];

/** The theme each slot holds until a person picks another. */
export const DEFAULT_THEME = { light: "knobas-light", dark: "knobas-dark" } as const;
