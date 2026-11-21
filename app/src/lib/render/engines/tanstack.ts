/**
 * TanStack Highlight 1.2.12 as an engine (the highlighter re-ruling, 2026-11-21, section 3): the one file that imports
 * it. One highlighter with all its 79 languages, made the first time a job needs it, then one `tokenize` call for the
 * whole text: it has no state to carry, so the input is never cut.
 *
 * **It answers `plaintext` for a language it lacks and does not throw**, so the answer's `lang` is checked against the
 * name asked for, and a difference throws: the next engine draws. Its tokens run over the whole text; they are cut into
 * lines here, a line of 4,000 characters or more made one plain token, and answered in 400-line slices, the focus first.
 */
import { createHighlighter, type Highlighter, type HighlightToken } from "@tanstack/highlight/core";
import * as languages from "@tanstack/highlight/languages";

import type { LineTokens } from "../port";
import { tokenRole, type TokenRole } from "../roles";
import { joined, LONGEST_LINE, postSlices, type Engine } from "./engine";

/** A token's runs cut at its line breaks, into lines of `[text, role]`. */
export function linesOf(tokens: readonly HighlightToken[], lang: string): LineTokens[] {
  const lines: (readonly [string, TokenRole])[][] = [[]];
  for (const token of tokens) {
    const role = token.className === undefined ? "" : tokenRole("tanstack", token.className, lang);
    const parts = token.value.split("\n");
    parts.forEach((part, i) => {
      if (i > 0) lines.push([]);
      if (part !== "") lines[lines.length - 1]!.push([part, role]);
    });
  }
  return lines.map((runs) => {
    const length = runs.reduce((n, [text]) => n + text.length, 0);
    return length >= LONGEST_LINE ? [[runs.map(([text]) => text).join(""), ""] as const] : joined(runs);
  });
}

export function createTanstack(): Engine {
  let highlighter: Highlighter | null = null;
  return {
    async run(text, name, lang, ctx) {
      highlighter ??= createHighlighter({ languages: Object.values(languages) });
      const result = highlighter.tokenize(text, { lang: name });
      if (result.lang !== name) throw new Error(`TanStack Highlight has no ${name}: it answered ${result.lang}`);
      postSlices(linesOf(result.tokens, lang), ctx);
    },
  };
}
