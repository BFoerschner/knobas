/**
 * The check's sets kept whole (the schema session cache, 2026-11-24): a document opened again, or another document on
 * the same schema, takes the set the last check read, so the check skips the reads, the rounds and the compile.
 *
 * - **Kept only where the reads are pinned.** The page says what its reads stand on (`KeepAt`): the repository and the
 *   commit, or the working tree's head and git's list's stamp. Where they move (no commit), nothing is kept.
 * - **The key** is that pin, the entry (its path and text: the document's folder, namespace and prefixes), the files
 *   pointed to (a download is named by its SHA-256), the namespace written bare, and the bundled schemas' version.
 *   Every file the set read is read at the pin, so the pin stands for them.
 * - **What moves under the pin is asked again** before a kept set is served: another repository's file (read at its
 *   branch's tip) and the list of other repositories that declare a namespace. An answer that differs reads the set
 *   again, and the stale set is dropped.
 * - **Kept only whole**: nothing still to read, and every file read. A file that could not be read may answer next time.
 * - **Bounded**: {@link KEEP_SETS} sets and {@link KEEP_BYTES} of their files, the least recently used dropped first,
 *   never one a session still holds. A commit that moves drops the sets kept for the commit before it.
 */
import type { Bundled, XsdSession } from "./xsd-answer";

/** What the page's reads stand on: `scope` the repository and the entity it is read at, `at` the commit (or stamp). */
export interface KeepAt {
  readonly scope: string;
  readonly at: string;
}

/** How many sets are kept. */
export const KEEP_SETS = 4;
/** How much of their files' text is kept, in bytes (two a character, as the engine holds them). */
export const KEEP_BYTES = 32 * 1024 * 1024;

/** A set kept whole. */
export interface Kept {
  readonly key: string;
  readonly scope: string;
  readonly at: string;
  /** The session as it stood whole: never changed, each session that takes it holds its own copy. */
  readonly base: XsdSession;
  /** The sessions holding it. */
  refs: number;
  readonly bytes: number;
  /** What moves under the pin, asked again before it is served: `@repo/…` paths and `?elsewhere:<namespace>`. */
  readonly verify: readonly string[];
}

/** The kept sets in the order they were last used, the least recent first. */
export type KeptSets = Map<string, Kept>;

const stores = new WeakMap<object, KeptSets>();
/** The kept sets of one place sets are held (the worker's sessions, or a test's). */
export function keptOf(owner: object): KeptSets {
  let store = stores.get(owner);
  if (store === undefined) {
    store = new Map();
    stores.set(owner, store);
  }
  return store;
}

const versions = new WeakMap<Bundled, string>();
/** The bundled schemas' version: a hash of their paths and texts (FNV-1a), made once. */
export function bundledVersion(bundled: Bundled): string {
  let v = versions.get(bundled);
  if (v !== undefined) return v;
  let h = 0x811c9dc5;
  for (const [path, text] of [...bundled.texts].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    const s = `${path}\u0000${text}\u0000`;
    for (let i = 0; i < s.length; i += 1) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  }
  v = `${bundled.texts.size}:${(h >>> 0).toString(16)}`;
  versions.set(bundled, v);
  return v;
}

/** What an open asks for, as far as the key reads it. */
export interface OpenFacts {
  readonly entry: string;
  readonly text: string;
  readonly paths: readonly string[] | null;
  readonly pointed: readonly unknown[];
  readonly home?: string;
  readonly lean?: boolean;
}

/** The key a set is kept by. */
export function keyOf(at: KeepAt, open: OpenFacts, bundled: Bundled): string {
  return JSON.stringify([at.scope, at.at, open.entry, open.text, open.home ?? null, open.lean === true, open.paths, open.pointed, bundledVersion(bundled)]);
}

/** Whether a session's set is whole: nothing to read, every file read, nothing still reading. */
export function whole(s: XsdSession): boolean {
  const set = s.set;
  if (set === null || set.need.length > 0) return false;
  for (const t of s.texts.values()) if (t === null) return false;
  return !set.docs.some((d) => d.reading || d.refs.some((r) => r.reading));
}

/** What moves under the pin in a session's set: other repositories' files, and who else declares a namespace. */
export function movingOf(s: XsdSession): string[] {
  return [...[...s.texts.keys()].filter((p) => p.startsWith("@repo/")), ...[...s.elsewhere.keys()].map((ns) => `?elsewhere:${ns}`)];
}

/** Sets kept for another commit in the same scope are dropped: the commit moved. */
export function dropMoved(store: KeptSets, at: KeepAt): void {
  for (const k of [...store.values()]) if (k.scope === at.scope && k.at !== at.at) store.delete(k.key);
}

/** Drops the least recently used sets held by no session until the bounds hold. */
export function evict(store: KeptSets, sets = KEEP_SETS, most = KEEP_BYTES): void {
  let bytes = 0;
  for (const k of store.values()) bytes += k.bytes;
  for (const k of [...store.values()]) {
    if (store.size <= sets && bytes <= most) return;
    if (k.refs > 0) continue;
    store.delete(k.key);
    bytes -= k.bytes;
  }
}

/** Keeps a session's set once it is whole, under the key it opened with; the session holds it. */
export function keepIfWhole(store: KeptSets, s: XsdSession): void {
  const keep = s.keep;
  if (keep === undefined || s.kept !== undefined || store.has(keep.key) || !whole(s)) return;
  let bytes = 0;
  for (const t of s.texts.values()) bytes += (t?.length ?? 0) * 2;
  const base: XsdSession = { ...s, rows: new Map(), tree: new Map() };
  delete base.kept;
  delete base.verifying;
  const kept: Kept = { key: keep.key, scope: keep.scope, at: keep.at, base, refs: 1, bytes, verify: movingOf(s) };
  store.set(keep.key, kept);
  s.kept = kept;
  evict(store);
}

/** A session on a kept set: its own rows, the set and its files shared until it changes them. */
export function attach(store: KeptSets, kept: Kept): XsdSession {
  store.delete(kept.key);
  store.set(kept.key, kept);
  kept.refs += 1;
  return { ...kept.base, rows: new Map(), tree: new Map(), kept };
}

/** A session about to change its set lets go of the kept one: it takes copies of what it shares. */
export function detach(store: KeptSets, s: XsdSession): void {
  const kept = s.kept;
  if (kept === undefined) return;
  delete s.kept;
  delete s.verifying;
  kept.refs -= 1;
  s.texts = new Map(s.texts);
  s.elsewhere = new Map(s.elsewhere);
  if (s.paths !== null) s.paths = new Set(s.paths);
  evict(store);
}

/** A session closed: the kept set it held may go. */
export function release(store: KeptSets, s: XsdSession): void {
  if (s.kept === undefined) return;
  s.kept.refs -= 1;
  delete s.kept;
  evict(store);
}

/**
 * Whether what moves under the pin answers as it did when the set was kept: every item of `verify` answered, and alike.
 * The port answers one namespace a round, so a set that asks two others reads again.
 */
export function sameAnswers(s: XsdSession, verify: readonly string[], files: readonly { path: string; text: string | null }[], elsewhere: { namespace: string; list: readonly unknown[] } | undefined): boolean {
  for (const v of verify) {
    if (v.startsWith("?elsewhere:")) {
      const ns = v.slice("?elsewhere:".length);
      if (elsewhere === undefined || elsewhere.namespace !== ns || JSON.stringify(s.elsewhere.get(ns) ?? null) !== JSON.stringify(elsewhere.list)) return false;
      continue;
    }
    const f = files.find((x) => x.path === v);
    if (f === undefined || !s.texts.has(v) || s.texts.get(v) !== f.text) return false;
  }
  return true;
}

/** *Download again*, a pick, a Settings change: every kept set goes; sessions holding one keep reading it. */
export function forget(store: KeptSets): void {
  store.clear();
}
