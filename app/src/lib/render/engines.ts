/**
 * Which highlighter draws a text (the highlighter re-ruling, 2026-11-21, section 2; ADR-0051). A setting picks one
 * (`knobas.highlighter`, `tf/highlighter.svelte.ts`): Pierre Highlights, the default, TanStack Highlight or Shiki. The
 * chosen one draws every language it has. What it lacks, the next in its order draws, and a file it throws on goes to
 * the next too. Pierre never draws SQL while another can: it reads a PostgreSQL function's body as one string.
 *
 * Nothing here imports an engine: the worker imports one the first time a job needs it (`highlight-core.ts`).
 */
import type { EngineId } from "./roles";
import { BY_EXT, BY_FENCE, BY_NAME, LANGUAGES, type LangId } from "./languages";

export type { EngineId } from "./roles";
export const ENGINES: readonly EngineId[] = ["pierre", "tanstack", "shiki"];
export const DEFAULT_ENGINE: EngineId = "pierre";

/** Who draws after the chosen one. Shiki is last under the other two, so the languages only it knows keep their colours. */
export const ORDER: Readonly<Record<EngineId, readonly EngineId[]>> = {
  pierre: ["pierre", "tanstack", "shiki"],
  tanstack: ["tanstack", "pierre", "shiki"],
  shiki: ["shiki", "pierre", "tanstack"],
};

/** Languages an engine draws badly: tried last under any choice, never dropped. A language joins by one line. */
export const WEAK: Readonly<Partial<Record<EngineId, readonly LangId[]>>> = { pierre: ["sql"] };

/** The engines that may draw `lang` under `chosen`, in the order they are tried: those that have it and have not failed, a weak one last. */
export function walk(chosen: EngineId, lang: LangId, failed: ReadonlySet<EngineId> = new Set()): EngineId[] {
  const names = LANGUAGES[lang];
  if (names === undefined) return [];
  const able = ORDER[chosen].filter((engine) => names[engine] !== undefined && !failed.has(engine));
  const weak = (engine: EngineId) => WEAK[engine]?.includes(lang) ?? false;
  return [...able.filter((engine) => !weak(engine)), ...able.filter(weak)];
}

/** `lang`'s name in `engine`, or `undefined` where it has none. */
export function nameIn(engine: EngineId, lang: LangId): string | undefined {
  return LANGUAGES[lang]?.[engine];
}

/** The language for a file's path, or `null` where it has none. */
export function languageOf(path: string): LangId | null {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name]!;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return BY_EXT[name.slice(dot + 1)] ?? null;
}

/** The language for a Markdown fence's language, or `null`. */
export function languageOfFence(lang: string): LangId | null {
  return BY_FENCE[lang.trim().toLowerCase()] ?? null;
}

/** An engine's name for what the setting stores, `pierre` for anything else. */
export function engineOf(value: string | null | undefined): EngineId {
  return value === "tanstack" || value === "shiki" ? value : DEFAULT_ENGINE;
}
