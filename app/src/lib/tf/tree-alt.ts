/**
 * Shared Alt+click (Option+click on macOS) recursive tree expand/collapse helpers (#2521).
 *
 * Every collapsible tree in knobas uses the same gesture check ({@link isAltClick}) and the same bounded breadth-first
 * traversal caps so expanding a deep or lazy/uncloned remote tree never triggers runaway network or IPC requests.
 */

/** Maximum total nodes opened in one recursive Alt+click expansion across any tree. */
export const TREE_EXPAND_CAP = 500;

/** Maximum uncached remote/lazy fetches triggered by one recursive Alt+click expansion. */
export const LAZY_FETCH_CAP = 8;

/** Maximum uncached local-checkout or worker fetches triggered by one recursive Alt+click expansion. */
export const LOCAL_FETCH_CAP = 100;

/** Whether a click carries Alt (Option) alone on the primary button. */
export function isAltClick(event: MouseEvent): boolean {
  return event.altKey && event.button === 0 && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}

/**
 * Breadth-first collection of `roots` and their descendants, cycle-safe and bounded to `cap` nodes.
 */
export function collectDescendants<T>(roots: readonly T[], childrenOf: (node: T) => readonly T[], cap = TREE_EXPAND_CAP): T[] {
  const out: T[] = [];
  const seen = new Set<T>();
  const queue: T[] = [];
  for (const root of roots) {
    if (seen.has(root)) continue;
    seen.add(root);
    queue.push(root);
  }
  for (let i = 0; i < queue.length && out.length < cap; i += 1) {
    const node = queue[i]!;
    out.push(node);
    for (const child of childrenOf(node)) {
      if (seen.has(child)) continue;
      seen.add(child);
      queue.push(child);
    }
  }
  return out;
}

/**
 * Synchronous breadth-first collection for trees where some nodes still require a lazy read when opened (such as Docs).
 * Already-held nodes expand freely up to `nodeCap`; lazy nodes are included up to `fetchCap` and not descended into.
 */
export function collectLazySync<T>(
  roots: readonly T[],
  opts: {
    isLazy: (node: T) => boolean;
    childrenOf: (node: T) => readonly T[];
    nodeCap?: number;
    fetchCap?: number;
  },
): T[] {
  const nodeCap = opts.nodeCap ?? TREE_EXPAND_CAP;
  const fetchCap = opts.fetchCap ?? LAZY_FETCH_CAP;
  const out: T[] = [];
  const seen = new Set<T>();
  const queue: T[] = [];
  let lazy = 0;
  for (const root of roots) {
    if (seen.has(root)) continue;
    seen.add(root);
    queue.push(root);
  }
  for (let i = 0; i < queue.length && out.length < nodeCap; i += 1) {
    const node = queue[i]!;
    if (opts.isLazy(node)) {
      if (lazy >= fetchCap) continue;
      lazy += 1;
      out.push(node);
      continue;
    }
    out.push(node);
    for (const child of opts.childrenOf(node)) {
      if (seen.has(child)) continue;
      seen.add(child);
      queue.push(child);
    }
  }
  return out;
}

export interface LazyExpandOptions<K> {
  /** Max total nodes opened (cached + fetched). Defaults to {@link TREE_EXPAND_CAP}. */
  nodeCap?: number;
  /** Max uncached lazy fetches allowed. Defaults to {@link LAZY_FETCH_CAP}. */
  fetchCap?: number;
  /** Max depth below `start` (0 = `start` alone). Defaults to Infinity. */
  depthCap?: number;
  /** Abort check (e.g. branch changed or user collapsed while in flight). */
  alive?: () => boolean;
  /** Children already held in memory for `key`, or `undefined` if not yet fetched. */
  cached: (key: K) => readonly K[] | undefined;
  /** Fetch children for `key` when not cached; returns child keys (or `undefined` on failure/abort). */
  fetch?: (key: K) => Promise<readonly K[] | undefined>;
  /** Mark `key` as open (called as each node is visited, up to caps). */
  open: (key: K) => void;
}

/**
 * Bounded breadth-first recursive expansion for async/lazy trees. Already-cached nodes open with zero fetches; uncached
 * nodes fetch up to `fetchCap`. When every node is already cached, runs synchronously without yielding.
 */
export async function expandLazy<K>(start: K, opts: LazyExpandOptions<K>): Promise<void> {
  const nodeCap = opts.nodeCap ?? TREE_EXPAND_CAP;
  const fetchCap = opts.fetchCap ?? LAZY_FETCH_CAP;
  const depthCap = opts.depthCap ?? Number.POSITIVE_INFINITY;
  const queue: { key: K; depth: number }[] = [{ key: start, depth: 0 }];
  const seen = new Set<K>([start]);
  let opened = 0;
  let fetched = 0;

  for (let i = 0; i < queue.length && opened < nodeCap; i += 1) {
    if (opts.alive !== undefined && !opts.alive()) return;
    const { key, depth } = queue[i]!;
    let kids = opts.cached(key);
    if (kids === undefined) {
      if (opts.fetch === undefined || fetched >= fetchCap) {
        if (depth === 0) opts.open(key);
        continue;
      }
      fetched += 1;
      opts.open(key);
      opened += 1;
      kids = await opts.fetch(key);
      if (opts.alive !== undefined && !opts.alive()) return;
      if (kids === undefined) continue;
    } else {
      opts.open(key);
      opened += 1;
    }
    if (depth >= depthCap) continue;
    for (const child of kids) {
      if (seen.has(child)) continue;
      seen.add(child);
      queue.push({ key: child, depth: depth + 1 });
    }
  }
}

/** Every key in `keys` equal to `root` or strictly nested under `${root}${sep}` (or all keys when `root === ""`). */
export function prefixDescendants(keys: Iterable<string>, root: string, sep = "/"): string[] {
  const prefix = root === "" ? "" : `${root}${sep}`;
  const out: string[] = [];
  for (const key of keys) {
    if (root === "" || key === root || key.startsWith(prefix)) out.push(key);
  }
  return out;
}
