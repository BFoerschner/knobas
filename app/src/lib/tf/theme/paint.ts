/**
 * Small pieces Settings › Appearance › Code colours draws with: custom properties on an element through the CSSOM (the app's policy
 * refuses a `style="…"` in markup), and a line's tokens cut at a range a diff marks as a changed word.
 */
import type { Attachment } from "svelte/attachments";

import type { LineTokens } from "../../render/port";

/** Sets each of `values()` on the element's style, and removes what a later run no longer sets. */
export function cssVars(values: () => Record<string, string>): Attachment<HTMLElement> {
  return (node) => {
    const set = values();
    for (const [property, value] of Object.entries(set)) node.style.setProperty(property, value);
    return () => {
      for (const property of Object.keys(set)) node.style.removeProperty(property);
    };
  };
}

/** A piece of a line: its text, its role (`""` plain), and whether it is inside the marked range. */
export interface Piece {
  text: string;
  kind: string;
  mark: boolean;
}

/** A line's tokens, or its plain text where there are none yet, cut at `[from, to)`, which is marked. */
export function marked(text: string, tokens: LineTokens | undefined, from = -1, to = -1): Piece[] {
  const runs = tokens ?? [[text, ""] as const];
  const out: Piece[] = [];
  let at = 0;
  for (const [part, kind] of runs) {
    const inner = [from - at, to - at].filter((c) => c > 0 && c < part.length);
    const cuts = [...new Set([0, ...inner.sort((a, b) => a - b), part.length])];
    for (let i = 0; i + 1 < cuts.length; i += 1) {
      const start = at + cuts[i]!;
      out.push({ text: part.slice(cuts[i], cuts[i + 1]), kind, mark: start >= from && start < to });
    }
    at += part.length;
  }
  return out;
}
