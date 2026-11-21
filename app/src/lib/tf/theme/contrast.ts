/**
 * A theme's code colours against every surface code is drawn on (ADR-0051, section 9 of the decisions): the file
 * viewer and a diff on `--sheet`, a Docs code block on `--sheet-2`, a Markdown fence on `--paper`, and a diff's added
 * and removed lines. A role's figure is its lowest WCAG ratio over those; under 4.5:1 the editor names the surface and
 * offers a fix that moves the colour's lightness alone.
 *
 * The surfaces are those of the window theme drawn for that side (ADR-0055): `colours` is `themes.colours(kind)`, or a
 * theme's own window colours where the preview shows it on them. The maths is `colour.ts`'s.
 */
import type { Role } from "../../render/roles";
import type { Colours } from "./app-derive";
import { fix, ratioText, worst } from "./colour";
import type { ThemeKind } from "./themes";

export { contrast, fix, luminance, mix as mixOklab, toOklch, worst } from "./colour";

/** The ratio text needs. */
export const TEXT_RATIO = 4.5;

export type Surface = "sheet" | "sheet-2" | "paper" | "added line" | "removed line";

/** A window theme's code surfaces: its panel, inset and page, and a diff's added and removed lines (the states' tints). */
export function surfacesOf(c: Colours): Record<Surface, string> {
  return { sheet: c.sheet, "sheet-2": c.sheet2, paper: c.paper, "added line": c.okBg, "removed line": c.errBg };
}

/** The surfaces `role` is checked on, with `added`/`removed` the theme's own tints where it sets them. A comment draws `--ink-2` on a diff's lines. */
export function surfacesFor(c: Colours, role: Role | null, tints: { added?: string; removed?: string } = {}): Record<string, string> {
  const s: Record<string, string> = { ...surfacesOf(c) };
  if (tints.added) s["added line"] = tints.added;
  if (tints.removed) s["removed line"] = tints.removed;
  if (role === "com") {
    delete s["added line"];
    delete s["removed line"];
  }
  return s;
}

/** A role's figure in the editor: its lowest ratio, and whether it is under 4.5:1. A role left out draws in the window's ink. */
export function roleContrast(c: Colours, role: Role, value: string | undefined, tints: { added?: string; removed?: string } = {}): { ratio: number; on: string; low: boolean } {
  const { ratio, on } = worst(value ?? c.ink, surfacesFor(c, role, tints));
  return { ratio, on, low: ratio < TEXT_RATIO };
}

/** The sentence under a low role: *2.5:1 on a removed line. Text needs 4.5:1.* A figure is cut, never rounded up. */
export function lowSentence(ratio: number, on: string): string {
  const where = on === "sheet" ? "the sheet" : on === "sheet-2" ? "a Docs code block" : on === "paper" ? "the paper" : `an ${on}`.replace("an removed", "a removed");
  return `${ratioText(ratio)} on ${where}. Text needs 4.5:1.`;
}

/** A window theme's diff colours, what a code theme that sets none draws (`--ok-bg`, `--err-bg`, `--ok-word`, `--err-word`). */
export function diffDefault(c: Colours): { added: string; removed: string; addedWord: string; removedWord: string } {
  return { added: c.okBg, removed: c.errBg, addedWord: c.okWord, removedWord: c.errWord };
}

/** `role`'s colour, fixed to 4.5:1 where it falls under on `c`'s surfaces, or `null` where it passes already. Darker on a light theme, lighter on a dark one. */
export function fixedRole(c: Colours, kind: ThemeKind, role: Role, value: string | undefined, tints: { added?: string; removed?: string } = {}): string | null {
  const now = value ?? c.ink;
  const surfaces = surfacesFor(c, role, tints);
  if (worst(now, surfaces).ratio >= TEXT_RATIO) return null;
  return fix(now, surfaces, TEXT_RATIO, kind === "dark" ? "lighter" : "darker");
}
