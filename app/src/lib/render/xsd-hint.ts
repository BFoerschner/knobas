/**
 * Which files a schema set read, kept across restarts (the XML view's first open, 2026-11-25, Fable's ruling): the first
 * check of a document after knobas starts reads its set's files in one round instead of one round a level, so its
 * verdict waits on fewer trips through the page's thread.
 *
 * - **A hint only fetches bytes sooner.** It never says where a reference is found or what is in the set: every file is
 *   read fresh at the pin by the port's own reads, and the schema worker takes a hinted file only when its own walk asks
 *   for that path (`xsd-answer.ts`). A hint that is right, stale, too wide or garbage answers as no hint does.
 * - **Paths only**, never a text, a parse or a compile. Only the shapes the walk itself makes are taken: a path in the
 *   repository, `@repo/…`, `@download/…`, `@knobas/…`. Anything else read back (the store is the webview's, and
 *   editable) is dropped, so a hint never names an address (ADR-0058: the set makes no connection).
 * - **Keyed** by the repository, the entry's path and a hash of the entry's text (the document's folder, namespace,
 *   location and prefixes); no commit and no branch, since a hint that went stale costs only a read.
 * - **Bounded**: {@link HINT_ENTRIES} entries, the least recently written dropped first, at most {@link HINT_PATHS}
 *   paths each (a larger set is not hinted), and an entry not written for {@link HINT_DAYS} days goes.
 * - **Written only where the set was kept whole**: read at a pin, every file read.
 * - It is also what says this profile checks XML (`checkedLately`): the schema worker and libxml2 are warmed at idle
 *   only where a document was checked lately (`xml-warm.ts`).
 */
import { keptValue, windowStorage, type KeptStorage, type KeptValue } from "../tf/kept-value";

/** How many entries are kept. */
export const HINT_ENTRIES = 32;
/** The most paths an entry names: a set that reads more is not hinted. */
export const HINT_PATHS = 32;
/** How long an entry not written again is kept, in days. */
export const HINT_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;
/** The longest path taken back. */
const PATH_MAX = 1024;
/** Where the hints are kept: one versioned key. */
export const HINT_KEY = "knobas.xsd-set-hint.v1";

/** One entry: its key, the paths its set read, when it was last written (ms since the epoch). */
interface Entry {
  readonly k: string;
  readonly p: readonly string[];
  readonly t: number;
}

/** The hints' store: the webview's, or a test's. */
export function hintStore(storage: () => KeptStorage | null = windowStorage): KeptValue {
  return keptValue(storage, HINT_KEY);
}

/** FNV-1a over a text, as hex. */
function hashOf(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193);
  return (h >>> 0).toString(16);
}

/** The key a set's hint is kept by: the repository, the entry's path and its text's hash. */
export function hintKey(repo: string, entryPath: string, entryText: string): string {
  return JSON.stringify([repo, entryPath, hashOf(entryText), entryText.length]);
}

/** Whether a path is one the walk itself makes: in the repository, or `@repo/…`, `@download/…`, `@knobas/…`. */
export function hintable(path: unknown): path is string {
  if (typeof path !== "string" || path.length === 0 || path.length > PATH_MAX) return false;
  // No control character, no backslash, nothing the walk would read as an address or a question.
  if (/[\u0000-\u001f\u007f\\]/.test(path) || path.startsWith("/") || path.startsWith("?") || /^[a-z][a-z0-9+.-]*:/i.test(path)) return false;
  const parts = path.split("/");
  if (parts.some((p) => p === "" || p === "." || p === "..")) return false;
  if (path.startsWith("@")) return /^@repo\/[^/]+\/.+/.test(path) || /^@download\/[^/]+/.test(path) || /^@knobas\/[^/]+/.test(path);
  return true;
}

/** The entries kept, taken back whole: anything malformed, expired or past the bounds is left out. */
function entriesOf(raw: string | null, now: number): Entry[] {
  if (raw === null) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: Entry[] = [];
  for (const e of parsed) {
    if (e === null || typeof e !== "object") continue;
    const { k, p, t } = e as { k?: unknown; p?: unknown; t?: unknown };
    if (typeof k !== "string" || typeof t !== "number" || !Number.isFinite(t) || !Array.isArray(p)) continue;
    if (now - t > HINT_DAYS * DAY_MS || t > now + DAY_MS) continue;
    if (p.length > HINT_PATHS || !p.every(hintable)) continue;
    out.push({ k, p: p as string[], t });
  }
  return out.slice(-HINT_ENTRIES);
}

/** The paths a set read last time, for its key; none where nothing (or nothing good) is kept. */
export function hintedPaths(store: KeptValue, key: string, now = Date.now()): string[] {
  return [...(entriesOf(store.read(), now).find((e) => e.k === key)?.p ?? [])];
}

/**
 * Keeps the paths a set read whole under its key, the newest last; the least recently written go past the bound. A set
 * that read more than {@link HINT_PATHS} files, or a path the walk does not make, keeps no hint.
 */
export function keepHint(store: KeptValue, key: string, paths: readonly string[], now = Date.now()): void {
  const own = [...new Set(paths)];
  if (own.length === 0 || own.length > HINT_PATHS || !own.every(hintable)) return;
  const kept = entriesOf(store.read(), now).filter((e) => e.k !== key);
  kept.push({ k: key, p: own, t: now });
  store.write(JSON.stringify(kept.slice(-HINT_ENTRIES)));
}

/** Whether this profile checked a document lately: an entry written in the last {@link HINT_DAYS} days. */
export function checkedLately(store: KeptValue, now = Date.now()): boolean {
  return entriesOf(store.read(), now).length > 0;
}
