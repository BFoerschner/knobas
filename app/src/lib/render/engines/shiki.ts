/**
 * Shiki 4.5.0 as an engine (the highlighter re-ruling, 2026-11-21): the one file that imports it, loaded the first time a
 * job needs it. Today's highlighting moved here whole (spec #2234 step 5, #2254):
 *
 * - **Grammars load lazily**: a language's grammar is imported the first time a file in it is opened.
 * - **Its one theme colours a scope `var(--hl-<role>)`** ({@link ROLE_SCOPES}), read back as the role.
 * - **Lines in view first**: a file is tokenized from the top in chunks, carrying the grammar's state, so a comment
 *   opened on line 3 still colours line 40,000. Before that pass reaches the lines in view, they are tokenized on their
 *   own, from a fresh state, and sent first; the pass overwrites them when it gets there.
 */
import { createHighlighterCore, type GrammarState, type HighlighterCore, type LanguageRegistration, type ThemedToken } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { bundledLanguages } from "shiki/langs";

import type { LineTokens } from "../port";
import { tokenRole, type TokenRole } from "../roles";
import { CHUNK, joined, LONGEST_LINE, type Engine } from "./engine";

/**
 * Shiki's scopes onto the roles (`roles.ts`). A scope takes the role of the longest selector it starts with, so
 * `keyword.operator.new` is a keyword though `keyword.operator` is an operator, Java's, Kotlin's and Groovy's
 * `storage.type.<lang>` (a class name there) is a type though `storage.type` is a keyword, and Kotlin's
 * `entity.name.type.annotation` is an annotation though `entity.name.type` is a type. Rust's `storage.type.rust` is
 * `let`, `struct` and `enum`, so it stays a keyword.
 */
export const ROLE_SCOPES: readonly (readonly [role: TokenRole, scopes: readonly string[]])[] = [
  ["com", ["comment", "punctuation.definition.comment"]],
  ["str", ["string", "punctuation.definition.string", "constant.character", "markup.inline.raw", "markup.fenced_code", "markup.raw", "string.unquoted"]],
  [
    "esc",
    [
      "constant.character.escape",
      "string.regexp",
      "constant.other.placeholder",
      "punctuation.definition.template-expression",
      "punctuation.section.embedded",
      "punctuation.section.interpolation",
      "constant.character.format.placeholder",
      "punctuation.definition.variable",
      "keyword.control.anchor.regexp",
    ],
  ],
  ["num", ["constant.numeric"]],
  ["con", ["constant.language", "constant.other", "support.constant", "variable.other.constant", "variable.other.enummember", "constant.other.symbol"]],
  [
    "kw",
    [
      "keyword",
      "storage",
      "storage.type",
      "storage.modifier",
      "keyword.control",
      "variable.language",
      "keyword.operator.new",
      "keyword.operator.expression",
      "keyword.operator.logical.python",
      "keyword.operator.instanceof",
      "keyword.operator.wordlike",
      "keyword.other",
      "storage.type.primitive",
      "storage.type.function",
      "storage.type.class",
      "constant.language.import-export-all",
    ],
  ],
  ["op", ["keyword.operator", "punctuation.separator.key-value", "storage.type.function.arrow"]],
  ["punct", ["punctuation", "meta.brace"]],
  [
    "fn",
    ["entity.name.function", "support.function", "meta.function-call.generic", "variable.function", "entity.name.command", "support.function.builtin"],
  ],
  [
    "ty",
    [
      "entity.name.type",
      "entity.name.class",
      "entity.name.struct",
      "entity.name.enum",
      "entity.name.trait",
      "entity.name.namespace",
      "support.type",
      "support.class",
      "entity.other.inherited-class",
      "storage.type.java",
      "storage.type.generic.java",
      "storage.type.object.array.java",
      "storage.type.kotlin",
      "entity.name.type.kotlin",
      "storage.type.groovy",
    ],
  ],
  [
    "prop",
    ["variable.other.property", "variable.other.object.property", "meta.object-literal.key", "variable.other.member", "support.variable.property", "entity.name.variable.field"],
  ],
  // TOML names a key `variable.other.key` and dotenv `variable.key` (the highlighter re-ruling, 2026-11-21).
  ["key", ["support.type.property-name", "entity.name.tag.yaml", "keyword.other.definition.ini", "variable.other.key", "variable.key"]],
  ["tag", ["entity.name.tag", "punctuation.definition.tag"]],
  [
    "attr",
    [
      "entity.other.attribute-name",
      "storage.type.annotation",
      "entity.name.type.annotation",
      "punctuation.definition.annotation",
      "meta.declaration.annotation",
      "entity.name.function.decorator",
      "meta.decorator",
      "meta.attribute.rust",
      "punctuation.definition.attribute.rust",
      "entity.name.function.macro",
      "entity.name.function.preprocessor",
      "meta.preprocessor",
      "keyword.control.directive",
      "comment.line.shebang",
      "meta.tag.preprocessor.xml",
      "meta.tag.sgml.doctype",
    ],
  ],
  ["var", ["variable.parameter"]],
  ["head", ["markup.heading", "punctuation.definition.heading", "entity.name.section"]],
  ["link", ["markup.underline.link", "string.other.link"]],
  ["strong", ["markup.bold"]],
  ["emph", ["markup.italic"]],
];

/** The one Shiki theme: a colour per role, which is a variable, so every theme is CSS. */
export const THEME = {
  name: "knobas",
  type: "light" as const,
  fg: "var(--ink)",
  bg: "transparent",
  settings: ROLE_SCOPES.map(([role, scope]) => ({ scope: [...scope], settings: { foreground: `var(--hl-${role})` } })),
};

export type Loader = (id: string) => Promise<LanguageRegistration[]>;

/** The grammar of `id`, imported on demand (a chunk of its own in the build). */
export const loadBundled: Loader = async (id) => {
  const load = bundledLanguages[id as keyof typeof bundledLanguages];
  if (!load) throw new Error(`no grammar for ${id}`);
  return (await load()).default;
};

/** Shiki's tokens as the port's: text and role, neighbours of one role joined. */
function toLines(tokens: ThemedToken[][], lang: string): LineTokens[] {
  return tokens.map((line) => joined(line.map((token) => [token.content, tokenRole("shiki", token.color ?? "", lang)] as const)));
}

/**
 * Shiki as an engine. `tokenizeTimeLimit` bounds one line in ms; left out, Shiki's own bound holds.
 */
export function createShiki({ load = loadBundled, tokenizeTimeLimit }: { load?: Loader; tokenizeTimeLimit?: number } = {}): Engine {
  let shiki: Promise<HighlighterCore> | null = null;
  const loaded = new Map<string, Promise<void>>();
  const highlighter = () =>
    (shiki ??= createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) }));

  async function ready(name: string): Promise<HighlighterCore> {
    const h = await highlighter();
    let grammar = loaded.get(name);
    if (!grammar) {
      grammar = load(name).then((registration) => h.loadLanguage(...registration));
      loaded.set(name, grammar);
      grammar.catch(() => loaded.delete(name));
    }
    await grammar;
    return h;
  }

  function tokenize(h: HighlighterCore, name: string, lines: string[], grammarState?: GrammarState) {
    return h.codeToTokens(lines.join("\n"), {
      lang: name,
      theme: THEME.name,
      tokenizeMaxLineLength: LONGEST_LINE,
      ...(tokenizeTimeLimit === undefined ? {} : { tokenizeTimeLimit }),
      ...(grammarState ? { grammarState } : {}),
    });
  }

  return {
    async run(text, name, lang, ctx) {
      const h = await ready(name);
      const lines = text.split("\n");
      let next = 0;
      let state: GrammarState | undefined;
      while (next < lines.length && !ctx.cancelled()) {
        const focus = ctx.takeFocus();
        // The lines in view, ahead of the pass, from a fresh state: the pass corrects them later.
        if (focus !== null) {
          const from = Math.max(focus[0], next);
          const to = Math.min(focus[1], lines.length);
          if (from >= next + CHUNK && from < to) {
            ctx.post(from, toLines(tokenize(h, name, lines.slice(from, to)).tokens, lang));
            await ctx.pause();
            continue;
          }
        }
        const end = Math.min(next + CHUNK, lines.length);
        const result = tokenize(h, name, lines.slice(next, end), state);
        state = result.grammarState;
        ctx.post(next, toLines(result.tokens, lang));
        next = end;
        if (next < lines.length) await ctx.pause();
      }
    },
  };
}
