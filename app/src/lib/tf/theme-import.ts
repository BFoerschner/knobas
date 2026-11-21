/**
 * *Import…* on Settings › Appearance › Theme and on Code colours (ADR-0055, the decisions' 3.10): one control, one
 * reading. A file's themes join the library; nothing is put in a slot. The toast says what was read and how many of
 * its colours are under their mark, with *Fix all*, which fixes both parts: the window's colours by `fixAll`, each code
 * role by lightness against the window it is drawn on (the theme's own where it has one).
 */
import { ROLES } from "../render/roles";
import { push } from "../shell/toasts.svelte";
import { failures, fixAll } from "./theme/app-check";
import { derive } from "./theme/app-derive";
import { fixedRole } from "./theme/contrast";
import { hasApp, importThemes, type Theme } from "./theme/themes";
import type { Themes } from "./themes.svelte";

/** The window colours a theme's code is checked on: its own where it has them, else those the window draws for its side. */
const codeGround = (themes: Themes, t: Theme) => (hasApp(t) ? derive(t.kind, t.app).colours : themes.colours(t.kind));

/** How many of a theme's colours are under their mark, both parts. */
export function underMark(themes: Themes, t: Theme): number {
  const app = hasApp(t) ? failures(derive(t.kind, t.app).colours).length : 0;
  const ground = codeGround(themes, t);
  const code = ROLES.filter((role) => t.syntax[role] !== undefined && fixedRole(ground, t.kind, role, t.syntax[role], t.diff ?? {}) !== null).length;
  return app + code;
}

/** *Fix all*: both parts of each theme, by lightness. */
export function fixThemes(themes: Themes, ids: readonly string[]): void {
  for (const id of ids)
    themes.update(id, (t) => {
      const next: Theme = { ...t };
      if (hasApp(t)) next.app = fixAll(t.kind, t.app!);
      const ground = codeGround(themes, next);
      const syntax = { ...t.syntax };
      for (const role of ROLES) {
        const fixed = syntax[role] === undefined ? null : fixedRole(ground, t.kind, role, syntax[role], t.diff ?? {});
        if (fixed !== null) syntax[role] = fixed;
      }
      next.syntax = syntax;
      return next;
    });
  themes.flush();
}

/** A file's text read and kept; answers the themes added (the first is the one to show), or `null` where none was. */
export function importText(themes: Themes, text: string): Theme[] | null {
  const result = importThemes(text);
  if ("refused" in result) {
    push({ text: result.refused });
    return null;
  }
  const added: Theme[] = [];
  for (const one of result.themes) {
    const t = themes.add(one);
    if (t) added.push(t);
  }
  if (added.length === 0) {
    push({ text: "knobas keeps 64 themes of your own. Delete one to import another." });
    return null;
  }
  const low = added.reduce((n, t) => n + underMark(themes, t), 0);
  const what = added.length === 1 ? `${added[0]!.name} as a ${added[0]!.kind} theme` : `${added.length} themes`;
  const tail = low === 0 ? "" : low === 1 ? " 1 colour is under its mark." : ` ${low} colours are under their mark.`;
  const include = result.includes ? ` It includes ${result.includes}, which knobas cannot read here. Colours that file sets are worked out.` : "";
  const ids = added.map((t) => t.id);
  if (low === 0) push({ text: `Imported ${what}.${include}` });
  else push({ text: `Imported ${what}.${tail}${include}`, action: { label: "Fix all", run: () => fixThemes(themes, ids) } });
  return added;
}
