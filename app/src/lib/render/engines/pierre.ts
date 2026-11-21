/**
 * Pierre Highlights 0.5.2 as an engine (the highlighter re-ruling, 2026-11-21; decisions section 5): the one file that
 * imports it. Its WebAssembly is compiled when this module is imported, and every lexer is in it, so no language loads
 * anything more.
 *
 * - Up to 20,000 lines: one `codeToTokens` call, then the answer in 400-line slices, the slice holding the focus first,
 *   the rest from the top, with no pause between. Nothing is drawn in a colour that is corrected later.
 * - Over that: a `StreamTokenizer`, 4,000 lines a push, each push's lines answered in slices, a pause between pushes so a
 *   cancel or a new focus is read, `end()` for the last line, `dispose()` in a `finally`.
 * - Its `cssVariables` theme colours a token `var(--hls-<type>)`, read back as the role (`PIERRE_ROLE` in `roles.ts`).
 *   A name it lacks throws `RangeError`, which the worker treats as any throw: the next engine draws.
 */
import { codeToTokens, StreamTokenizer, type ThemedToken } from "@pierre/highlights";

import type { LineTokens } from "../port";
import { tokenRole } from "../roles";
import { joined, LONGEST_LINE, postSlices, type Engine } from "./engine";

/** Lines up to which a text is tokenized in one call. */
export const ONE_CALL = 20_000;
/** Lines a push of the stream holds. */
export const PUSH = 4_000;

/** The theme that colours a token by its type's variable. */
export const KNOBAS = { name: "knobas", appearance: "light", style: {}, cssVariables: true } as const;

const options = (name: string) => ({ lang: name as Parameters<typeof codeToTokens>[1]["lang"], theme: KNOBAS, tokenizeMaxLineLength: LONGEST_LINE });

function toLines(tokens: ThemedToken[][], lang: string): LineTokens[] {
  return tokens.map((line) => joined(line.map((token) => [token.content, tokenRole("pierre", token.color ?? "", lang)] as const)));
}

export function createPierre(): Engine {
  return {
    async run(text, name, lang, ctx) {
      const count = text.split("\n", ONE_CALL + 1).length;
      if (count <= ONE_CALL) {
        postSlices(toLines(codeToTokens(text, options(name)).tokens, lang), ctx);
        return;
      }
      const lines = text.split("\n");
      const stream = new StreamTokenizer(options(name));
      try {
        let at = 0;
        for (let from = 0; from < lines.length && !ctx.cancelled(); from += PUSH) {
          const last = from + PUSH >= lines.length;
          const chunk = lines.slice(from, from + PUSH).join("\n") + (last ? "" : "\n");
          const got = [...stream.pushCode(chunk), ...(last ? stream.end() : [])];
          postSlices(toLines(got, lang), ctx, at);
          at += got.length;
          if (!last) await ctx.pause();
        }
      } finally {
        stream.dispose();
      }
    },
  };
}
