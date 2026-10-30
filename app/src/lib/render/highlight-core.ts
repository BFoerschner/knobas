/**
 * Highlighting with Shiki (spec #2234 step 5, #2254), the part that runs in
 * the web worker (`highlight.worker.ts`). Kept apart from the worker so a node
 * test drives it whole.
 *
 * - **Grammars load lazily**: a language's grammar is imported the first time
 *   a file in it is opened, never before (`load`), so app start pays nothing.
 * - **The theme is the knobas palette**: each token's colour is a token
 *   variable (`var(--hl-kw)`), sent on as its kind (`kw`) and drawn by class
 *   (`.tf-hl-kw`, `tf/css/code.css`). The light and dark themes are the two
 *   values each `--hl-*` token has in `tf/css/tokens.css`, so a theme switch
 *   recolours what is drawn without asking the worker again.
 * - **Lines in view first**: a file is tokenized from the top in chunks,
 *   carrying the grammar's state, so a comment opened on line 3 still colours
 *   line 40,000. Before that pass reaches the lines in view, they are
 *   tokenized on their own, from a fresh state, and sent first; the pass
 *   overwrites them when it gets there.
 * - A file in no language Shiki knows gets `plain`: the viewer keeps its text.
 */
import { createHighlighterCore, type GrammarState, type HighlighterCore, type LanguageRegistration, type ThemedToken } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { bundledLanguages, bundledLanguagesInfo } from "shiki/langs";

import type { HighlightMessage, HighlightReply, LineTokens, Token } from "./port";

/** Lines per step of the pass from the top: a step is short enough for a focus to cut in. */
export const CHUNK = 400;

/** The kinds a token can be, each a `--hl-<kind>` token and a `.tf-hl-<kind>` class. */
const SCOPES: [kind: string, scopes: string[]][] = [
  ["com", ["comment", "punctuation.definition.comment"]],
  ["str", ["string", "punctuation.definition.string", "constant.character", "markup.inline.raw", "markup.fenced_code", "markup.raw"]],
  ["num", ["constant.numeric", "constant.language", "constant.other", "support.constant", "variable.other.constant"]],
  ["kw", ["keyword", "storage", "storage.type", "storage.modifier", "keyword.control", "markup.heading", "entity.name.section", "markup.bold"]],
  ["fn", ["entity.name.function", "support.function", "meta.function-call.generic", "markup.underline.link", "string.other.link"]],
  ["ty", ["entity.name.type", "entity.name.class", "entity.name.struct", "entity.name.enum", "entity.name.trait", "support.type", "support.class", "entity.name.tag", "entity.other.attribute-name"]],
];
// Keywords that are operators (`=`, `->`) stay ink.
const PLAIN_SCOPES = ["keyword.operator", "punctuation"];

/** The one Shiki theme: a colour per kind, which is a variable, so both palettes are CSS. */
export const THEME = {
  name: "knobas",
  type: "light" as const,
  fg: "var(--ink)",
  bg: "transparent",
  settings: [
    ...SCOPES.map(([kind, scope]) => ({ scope, settings: { foreground: `var(--hl-${kind})` } })),
    { scope: PLAIN_SCOPES, settings: { foreground: "var(--ink)" } },
  ],
};

/** Names Shiki has no extension for. */
const BY_NAME: Record<string, string> = {
  dockerfile: "docker",
  containerfile: "docker",
  makefile: "make",
  justfile: "just",
  "cargo.lock": "toml",
  "go.mod": "go",
  ".gitignore": "shellscript",
  ".env": "dotenv",
};
const BY_EXT: Record<string, string> = { h: "c", hpp: "cpp", cc: "cpp", htm: "html", svg: "xml", kts: "kotlin", mjs: "javascript", cjs: "javascript" };

const KNOWN = new Map<string, string>();
for (const info of bundledLanguagesInfo) {
  KNOWN.set(info.id, info.id);
  for (const alias of info.aliases ?? []) KNOWN.set(alias, info.id);
}

/** The Shiki language for a file's path, or `null` where it has none. */
export function languageOf(path: string): string | null {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name]!;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  const ext = name.slice(dot + 1);
  return BY_EXT[ext] ?? KNOWN.get(ext) ?? null;
}

/** The Shiki language for a Markdown fence's language, or `null`. */
export function languageOfFence(lang: string): string | null {
  return KNOWN.get(lang.trim().toLowerCase()) ?? null;
}

export type Loader = (id: string) => Promise<LanguageRegistration[]>;

/** The grammar of `id`, imported on demand (a chunk of its own in the build). */
export const loadBundled: Loader = async (id) => {
  const load = bundledLanguages[id as keyof typeof bundledLanguages];
  if (!load) throw new Error(`no grammar for ${id}`);
  return (await load()).default;
};

/** A token's kind from its colour: `var(--hl-kw)` is `kw`, anything else plain. */
function kindOf(token: ThemedToken): string {
  const color = token.color ?? "";
  return color.startsWith("var(--hl-") ? color.slice("var(--hl-".length, -1) : "";
}

/** Shiki's tokens as the port's: text and kind, neighbours of one kind joined. */
function toLines(tokens: ThemedToken[][]): LineTokens[] {
  return tokens.map((line) => {
    const out: Token[] = [];
    for (const token of line) {
      const kind = kindOf(token);
      const last = out[out.length - 1];
      if (last && last[1] === kind) out[out.length - 1] = [last[0] + token.content, kind];
      else out.push([token.content, kind]);
    }
    return out;
  });
}

interface Job {
  id: number;
  cancelled: boolean;
  /** The lines in view the viewer last named, not yet sent. */
  focus: [from: number, to: number] | null;
}

export interface HighlightCore {
  handle(message: HighlightMessage): void;
}

/**
 * The worker's side: takes the port's messages, `post`s its replies.
 * `pause` yields between chunks, so a focus or a cancel is read in between.
 */
export function createHighlightCore({
  post,
  load = loadBundled,
  pause = () => new Promise<void>((resolve) => setTimeout(resolve, 0)),
}: {
  post: (reply: HighlightReply) => void;
  load?: Loader;
  pause?: () => Promise<void>;
}): HighlightCore {
  let shiki: Promise<HighlighterCore> | null = null;
  const loaded = new Map<string, Promise<void>>();
  const jobs = new Map<number, Job>();

  const highlighter = () =>
    (shiki ??= createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) }));

  async function ready(lang: string): Promise<HighlighterCore> {
    const h = await highlighter();
    let grammar = loaded.get(lang);
    if (!grammar) {
      grammar = load(lang).then((registration) => h.loadLanguage(...registration));
      loaded.set(lang, grammar);
    }
    await grammar;
    return h;
  }

  function tokenize(h: HighlighterCore, lang: string, lines: string[], grammarState?: GrammarState) {
    return h.codeToTokens(lines.join("\n"), { lang, theme: THEME.name, grammarState, tokenizeMaxLineLength: 4000 });
  }

  async function run(job: Job, text: string, lang: string | null) {
    if (lang === null) return post({ type: "plain", id: job.id });
    let h: HighlighterCore;
    try {
      h = await ready(lang);
    } catch {
      return post({ type: "plain", id: job.id });
    }
    const lines = text.split("\n");
    let next = 0;
    let state: GrammarState | undefined;
    while (next < lines.length && !job.cancelled) {
      const focus = job.focus;
      job.focus = null;
      // The lines in view, ahead of the pass, from a fresh state: the pass corrects them later.
      if (focus !== null) {
        const from = Math.max(focus[0], next);
        const to = Math.min(focus[1], lines.length);
        if (from >= next + CHUNK && from < to) {
          post({ type: "lines", id: job.id, from, lines: toLines(tokenize(h, lang, lines.slice(from, to)).tokens) });
          await pause();
          continue;
        }
      }
      const end = Math.min(next + CHUNK, lines.length);
      const result = tokenize(h, lang, lines.slice(next, end), state);
      state = result.grammarState;
      post({ type: "lines", id: job.id, from: next, lines: toLines(result.tokens) });
      next = end;
      if (next < lines.length) await pause();
    }
    if (!job.cancelled) post({ type: "done", id: job.id });
    jobs.delete(job.id);
  }

  return {
    handle(message) {
      if (message.type === "open") {
        const job: Job = { id: message.id, cancelled: false, focus: message.focus ?? null };
        jobs.set(job.id, job);
        const lang = message.lang !== undefined ? languageOfFence(message.lang) : languageOf(message.path ?? "");
        void run(job, message.text, lang);
      } else if (message.type === "focus") {
        const job = jobs.get(message.id);
        if (job) job.focus = [message.from, message.to];
      } else {
        const job = jobs.get(message.id);
        if (job) job.cancelled = true;
        jobs.delete(message.id);
      }
    },
  };
}
