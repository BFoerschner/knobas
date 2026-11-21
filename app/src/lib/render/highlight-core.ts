/**
 * The highlight worker's core (spec #2234 step 5, #2254; ADR-0051 and the highlighter re-ruling of 2026-11-21), the
 * part that runs in the web worker (`highlight.worker.ts`). Kept apart from the worker so a node test drives it whole.
 *
 * - **A token carries a role, never a colour**: each engine's own token names become one of `roles.ts`'s roles, drawn
 *   by class (`.tf-hl-<role>`, `tf/css/code.css`) in a variable a theme sets, so a theme change recolours what is drawn
 *   without asking the worker again.
 * - **The engine is the setting's** (`open` carries it, as a worker has no `localStorage`): the chosen one draws every
 *   language it has, the next in its order what it lacks or throws on (`engines.ts`). An engine is imported the first
 *   time a job needs it, never before; one whose import fails is marked failed for the session, said once (`failed`),
 *   and skipped from then on. A throw starts the job again on the next engine from line 0, overwriting what was sent;
 *   it is warned once per engine and language.
 * - A text in no language any engine knows, over 100,000 lines or 8 MB, gets `plain`: the viewer keeps its text.
 *
 * This file imports no engine's package (a test reads its imports): the engines are `engines/*.ts`.
 */
import { languageOf, languageOfFence, nameIn, walk, type EngineId } from "./engines";
import type { Engine } from "./engines/engine";
import type { HighlightMessage, HighlightReply, LineTokens } from "./port";

export { CHUNK } from "./engines/engine";
export { languageOf, languageOfFence } from "./engines";

/** Lines over which a text is answered plain: at a million tokens and more the page would hold too much. */
export const HIGHLIGHT_MAX_LINES = 100_000;
/** Characters over which a text is answered plain (8 MB). */
export const HIGHLIGHT_MAX_CHARS = 8 * 1024 * 1024;

/** Each engine's module, imported on demand (a chunk of its own in the build). */
export type EngineLoaders = Readonly<Record<EngineId, () => Promise<Engine>>>;

export const LOADERS: EngineLoaders = {
  pierre: () => import("./engines/pierre").then((m) => m.createPierre()),
  tanstack: () => import("./engines/tanstack").then((m) => m.createTanstack()),
  shiki: () => import("./engines/shiki").then((m) => m.createShiki()),
};

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
 * The worker's side: takes the port's messages, `post`s its replies. `pause` yields between chunks, so a focus or a
 * cancel is read in between. `engines` replaces an engine's loader (tests count imports and stub throws).
 */
export function createHighlightCore({
  post,
  engines = {},
  pause = () => new Promise<void>((resolve) => setTimeout(resolve, 0)),
  warn = (text: string) => console.warn(text),
}: {
  post: (reply: HighlightReply) => void;
  engines?: Partial<EngineLoaders>;
  pause?: () => Promise<void>;
  warn?: (text: string) => void;
}): HighlightCore {
  const jobs = new Map<number, Job>();
  const loaded = new Map<EngineId, Promise<Engine>>();
  const failed = new Set<EngineId>();
  const warned = new Set<string>();

  /** The engine, imported once; a failed import marks it failed and says so once. */
  async function engine(id: EngineId): Promise<Engine | null> {
    let got = loaded.get(id);
    if (!got) {
      got = (engines[id] ?? LOADERS[id])();
      loaded.set(id, got);
    }
    try {
      return await got;
    } catch {
      if (!failed.has(id)) {
        failed.add(id);
        post({ type: "failed", engine: id });
      }
      return null;
    }
  }

  async function pass(job: Job, text: string, lang: string | null, chosen: EngineId): Promise<void> {
    if (lang === null || text.length > HIGHLIGHT_MAX_CHARS || text.split("\n", HIGHLIGHT_MAX_LINES + 2).length > HIGHLIGHT_MAX_LINES) {
      return post({ type: "plain", id: job.id });
    }
    for (const id of walk(chosen, lang, failed)) {
      const name = nameIn(id, lang);
      if (name === undefined || failed.has(id)) continue;
      const run = await engine(id);
      if (job.cancelled) return;
      if (run === null) continue;
      try {
        await run.run(text, name, lang, {
          post: (from: number, lines: LineTokens[]) => {
            if (!job.cancelled) post({ type: "lines", id: job.id, from, lines });
          },
          pause,
          cancelled: () => job.cancelled,
          takeFocus: () => {
            const focus = job.focus;
            job.focus = null;
            return focus;
          },
        });
        if (!job.cancelled) post({ type: "done", id: job.id });
        return;
      } catch (error) {
        if (job.cancelled) return;
        const key = `${id} ${lang}`;
        if (!warned.has(key)) {
          warned.add(key);
          warn(`highlight: ${id} failed on ${lang}, the next highlighter draws it: ${String(error)}`);
        }
      }
    }
    if (!job.cancelled) post({ type: "plain", id: job.id });
  }

  async function run(job: Job, text: string, lang: string | null, chosen: EngineId) {
    try {
      await pass(job, text, lang, chosen);
    } finally {
      jobs.delete(job.id);
    }
  }

  return {
    handle(message) {
      if (message.type === "open") {
        const job: Job = { id: message.id, cancelled: false, focus: message.focus ?? null };
        jobs.set(job.id, job);
        const lang = message.lang !== undefined ? languageOfFence(message.lang) : languageOf(message.path ?? "");
        void run(job, message.text, lang, message.engine ?? "pierre");
      } else if (message.type === "warm") {
        if (message.retry && failed.delete(message.engine)) loaded.delete(message.engine);
        void engine(message.engine);
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
