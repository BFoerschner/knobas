import { describe, expect, test, vi } from "vitest";

import {
  collectDescendants,
  collectLazySync,
  expandLazy,
  isAltClick,
  LAZY_FETCH_CAP,
  LOCAL_FETCH_CAP,
  prefixDescendants,
  TREE_EXPAND_CAP,
} from "./tree-alt";

describe("isAltClick", () => {
  test("accepts plain Alt+primary click and rejects other modifiers or buttons", () => {
    expect(isAltClick(new MouseEvent("click", { altKey: true, button: 0 }))).toBe(true);
    expect(isAltClick(new MouseEvent("click", { altKey: false, button: 0 }))).toBe(false);
    expect(isAltClick(new MouseEvent("click", { altKey: true, button: 1 }))).toBe(false);
    expect(isAltClick(new MouseEvent("click", { altKey: true, button: 2 }))).toBe(false);
    expect(isAltClick(new MouseEvent("click", { altKey: true, ctrlKey: true }))).toBe(false);
    expect(isAltClick(new MouseEvent("click", { altKey: true, metaKey: true }))).toBe(false);
    expect(isAltClick(new MouseEvent("click", { altKey: true, shiftKey: true }))).toBe(false);
  });
});

describe("collectDescendants", () => {
  type Node = { id: string; kids: Node[] };
  const n = (id: string, ...kids: Node[]): Node => ({ id, kids });

  test("walks breadth-first, stays cycle-safe, and respects node cap", () => {
    const tree = n("a", n("b", n("d"), n("e")), n("c", n("f")));
    expect(collectDescendants([tree], (x) => x.kids).map((x) => x.id)).toEqual(["a", "b", "c", "d", "e", "f"]);
    expect(collectDescendants([tree], (x) => x.kids, 4).map((x) => x.id)).toEqual(["a", "b", "c", "d"]);

    const cycleA = n("x");
    const cycleB = n("y", cycleA);
    cycleA.kids.push(cycleB);
    expect(collectDescendants([cycleA], (x) => x.kids).map((x) => x.id)).toEqual(["x", "y"]);
  });
});

describe("collectLazySync", () => {
  type Node = { id: string; lazy: boolean; kids: Node[] };
  const n = (id: string, lazy: boolean, ...kids: Node[]): Node => ({ id, lazy, kids });

  test("caps lazy nodes while expanding all held nodes up to nodeCap", () => {
    const root = n(
      "root",
      false,
      n("held-1", false, n("lazy-a", true), n("lazy-b", true)),
      n("held-2", false, n("lazy-c", true), n("lazy-d", true)),
    );
    const picked = collectLazySync([root], {
      isLazy: (x) => x.lazy,
      childrenOf: (x) => x.kids,
      fetchCap: 2,
    }).map((x) => x.id);
    expect(picked).toEqual(["root", "held-1", "held-2", "lazy-a", "lazy-b"]);
  });

  test("does not descend into children of a lazy node", () => {
    const root = n("root", true, n("partial-child", false, n("grand", false)));
    const picked = collectLazySync([root], {
      isLazy: (x) => x.lazy,
      childrenOf: (x) => x.kids,
      fetchCap: 2,
    }).map((x) => x.id);
    expect(picked).toEqual(["root"]);
  });
});

describe("expandLazy", () => {
  test("expands cached nodes with zero fetches and caps uncached fetches breadth-first", async () => {
    const remote: Record<string, string[]> = {
      root: ["a", "b", "c", "d"],
      a: ["a/1", "a/2"],
      b: ["b/1"],
      c: ["c/1"],
      d: ["d/1"],
    };
    const cache = new Map<string, string[]>([["root", remote["root"]!]]);
    const opened: string[] = [];
    const fetched: string[] = [];

    await expandLazy("root", {
      fetchCap: 2,
      cached: (k) => cache.get(k),
      fetch: async (k) => {
        fetched.push(k);
        const kids = remote[k] ?? [];
        cache.set(k, kids);
        return kids;
      },
      open: (k) => opened.push(k),
    });

    expect(fetched).toEqual(["a", "b"]);
    expect(opened).toEqual(["root", "a", "b"]);
  });

  test("respects depthCap, nodeCap, and alive cancellation", async () => {
    const tree: Record<string, string[]> = {
      r: ["r/1", "r/2"],
      "r/1": ["r/1/a"],
      "r/1/a": ["r/1/a/deep"],
      "r/2": ["r/2/b"],
    };
    const opened: string[] = [];
    await expandLazy("r", {
      depthCap: 1,
      cached: (k) => tree[k] ?? [],
      open: (k) => opened.push(k),
    });
    expect(opened).toEqual(["r", "r/1", "r/2"]);

    const openedCap: string[] = [];
    await expandLazy("r", {
      nodeCap: 3,
      cached: (k) => tree[k] ?? [],
      open: (k) => openedCap.push(k),
    });
    expect(openedCap).toEqual(["r", "r/1", "r/2"]);

    let live = true;
    const openedAbort: string[] = [];
    const fetch = vi.fn(async (k: string) => {
      if (k === "r/1") live = false;
      return tree[k] ?? [];
    });
    await expandLazy("r", {
      fetchCap: 10,
      alive: () => live,
      cached: (k) => (k === "r" ? tree[k] : undefined),
      fetch,
      open: (k) => openedAbort.push(k),
    });
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(openedAbort).toEqual(["r", "r/1"]);
  });

  test("exports bounded safety defaults", () => {
    expect(LAZY_FETCH_CAP).toBeGreaterThan(0);
    expect(LAZY_FETCH_CAP).toBeLessThanOrEqual(20);
    expect(LOCAL_FETCH_CAP).toBeGreaterThan(LAZY_FETCH_CAP);
    expect(TREE_EXPAND_CAP).toBeGreaterThanOrEqual(LOCAL_FETCH_CAP);
  });
});

describe("prefixDescendants", () => {
  test("matches the root and its path-prefixed descendants without false sibling matches", () => {
    const keys = ["src", "src/a", "src/a/b", "src-other", "lib"];
    expect(prefixDescendants(keys, "src")).toEqual(["src", "src/a", "src/a/b"]);
    expect(prefixDescendants(keys, "lib")).toEqual(["lib"]);
  });
});
