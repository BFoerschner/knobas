/** Where Docs opens and the spaces it hides (`docs-choices.svelte.ts`, PR #2479, 2026-11-17), and the rail's split. */
import { expect, test, vi } from "vitest";

import type { DocSpace, DocsChoices } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createDocsChoices, homeHash, spaceIdHash } from "./docs-choices.svelte";
import { railSpaces } from "./docs";

test("the address Docs opens: its own page with no home, a space's page by source and key, a page", () => {
  expect(homeHash(null)).toBe("#/docs");
  expect(homeHash({ kind: "space", id: "confluence/ENG", title: "Engineering" })).toBe("#/docs/space/confluence%3AENG");
  expect(homeHash({ kind: "page", id: "confluence:9", title: "Eng home" })).toBe("#/page/confluence%3A9");
  expect(spaceIdHash("conf-2/A B")).toBe("#/docs/space/conf-2%3AA%20B");
});

test("a change is drawn at once and the answer held; an older answer than the last write is not", async () => {
  const choices = createDocsChoices();
  let first: (value: DocsChoices) => void = () => {};
  choices.ports = {
    read: vi.fn(),
    setHome: vi.fn(),
    setHidden: vi
      .fn()
      .mockImplementationOnce(() => new Promise<DocsChoices>((resolve) => (first = resolve)))
      .mockImplementationOnce(async () => ({ home: null, hidden: [] })),
  };
  const one = choices.setHidden("c/A", "A", true, false);
  expect(choices.hidden.has("c/A")).toBe(true);
  await choices.setHidden("c/A", "A", false, false);
  first({ home: null, hidden: ["c/A"] });
  await one;
  expect(choices.hidden.has("c/A"), "the second press stands").toBe(false);
});

test("Docs' home set says so with Undo, and Undo puts the one before back", async () => {
  const choices = createDocsChoices();
  choices.ports = { read: vi.fn(), setHidden: vi.fn(), setHome: vi.fn(async (home) => ({ home, hidden: [] })) };
  await choices.setHome({ kind: "space", id: "c/ENG", title: "Engineering" });
  expect(choices.start).toBe("#/docs/space/c%3AENG");
  const toast = toasts.items.find((item) => item.text === "Docs opens on Engineering now.")!;
  toast.action!.run();
  await Promise.resolve();
  expect(choices.home).toBeNull();
  expect(choices.start).toBe("#/docs");
});

test("a home whose write fails is put back and said", async () => {
  const choices = createDocsChoices();
  choices.ports = { read: vi.fn(), setHidden: vi.fn(), setHome: vi.fn(async () => Promise.reject({ code: "internal", message: "down", source_id: null })) };
  await choices.setHome({ kind: "page", id: "c:1", title: "One" }, false);
  expect(choices.home).toBeNull();
  expect(toasts.items.some((item) => item.tone === "err" && item.text === "Could not change where Docs opens: down")).toBe(true);
});

test("a read a write overtook holds nothing", async () => {
  const choices = createDocsChoices();
  let answer: (value: DocsChoices) => void = () => {};
  choices.ports = { read: () => new Promise((resolve) => (answer = resolve)), setHome: vi.fn(async (home) => ({ home, hidden: [] })), setHidden: vi.fn() };
  const loading = choices.load();
  await choices.setHome({ kind: "page", id: "c:1", title: "One" }, false);
  answer({ home: null, hidden: [] });
  await loading;
  expect(choices.home?.id).toBe("c:1");
});

const space = (source: string, key: string, name: string): DocSpace => ({ source_id: source, source_name: source === "a" ? "Wiki A" : "Wiki B", key, name, pages: [] });

test("the rail's spaces: the hidden ones apart, both kept to the words by name, source where shared, and key", () => {
  const spaces = [space("a", "ENG", "Engineering"), space("a", "OPS", "Operations"), space("b", "ENG2", "Engineering"), space("a", "ARC", "Archive")];
  const names = (list: DocSpace[]) => list.map((s) => `${s.source_id}/${s.key}`);
  const all = railSpaces(spaces, new Set(["a/ARC"]), "");
  expect(names(all.shown)).toEqual(["a/ENG", "a/OPS", "b/ENG2"]);
  expect(names(all.hidden)).toEqual(["a/ARC"]);
  expect(names(railSpaces(spaces, new Set(), "ops").shown), "by key").toEqual(["a/OPS"]);
  expect(names(railSpaces(spaces, new Set(), "engineering wiki b").shown), "every word, the shared name's source too").toEqual(["b/ENG2"]);
  const narrowed = railSpaces(spaces, new Set(["a/ARC"]), "ARCH");
  expect([names(narrowed.shown), names(narrowed.hidden)]).toEqual([[], ["a/ARC"]]);
});
