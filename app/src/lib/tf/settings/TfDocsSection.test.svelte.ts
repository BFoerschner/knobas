/** Settings › Docs (PR #2479, 2026-11-17): where Docs opens, and a box per space for whether Docs shows it. */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocsChoices, DocsTree } from "../../ipc/entity";
import { createDocsChoices, type Choices } from "../docs/docs-choices.svelte";
import TfDocsSection from "./TfDocsSection.svelte";

const TREE: DocsTree = {
  spaces: [
    { source_id: "confluence", source_name: "Confluence", key: "ENG", name: "Engineering", pages: [], page_count: 12 },
    { source_id: "confluence", source_name: "Confluence", key: "ARC", name: "Archive 2019", pages: [], page_count: 3 },
  ],
  last_read: null,
  for_you: [],
};

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;
let choices: Choices;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  choices = createDocsChoices();
  choices.ports = {
    read: vi.fn(),
    setHome: vi.fn(async (home): Promise<DocsChoices> => ({ home, hidden: [...choices.hidden] })),
    setHidden: vi.fn(async (): Promise<DocsChoices> => ({ home: choices.home, hidden: [...choices.hidden] })),
  };
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function draw(): Promise<void> {
  view = mount(TfDocsSection, { target, props: { outline: async () => TREE, choices } });
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
}

const select = () => target.querySelector<HTMLSelectElement>("#tf-docs-opens")!;
const boxes = () => [...target.querySelectorAll<HTMLLabelElement>(".tf-stg-spaces .tf-ck")];

test("Docs opens on lists Docs' own page, then every space, a hidden one said so", async () => {
  choices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw();
  expect([...select().options].map((o) => o.textContent)).toEqual(["Docs' home, every space", "Engineering", "Archive 2019 (hidden)"]);
  expect(select().value).toBe("");
});

test("a space picked becomes Docs' home, and Docs' own page picked clears it", async () => {
  await draw();
  select().value = "space:confluence/ENG";
  select().dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(choices.home).toEqual({ kind: "space", id: "confluence/ENG", title: "Engineering" });
  expect(choices.ports.setHome).toHaveBeenCalledWith({ kind: "space", id: "confluence/ENG", title: "Engineering" });
  select().value = "";
  select().dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(choices.home).toBeNull();
});

test("a page chosen as Docs' home names itself in the list, chosen", async () => {
  choices.hold({ home: { kind: "page", id: "confluence:9", title: "Eng home" }, hidden: [] });
  await draw();
  expect(select().selectedOptions[0]?.textContent).toBe("Eng home (a page)");
});

test("a space's box cleared hides it, ticked shows it again, and the count says how many are shown", async () => {
  await draw();
  expect(boxes().map((b) => b.textContent?.replace(/\s+/g, " ").trim())).toEqual(["Engineering 12", "Archive 2019 3"]);
  expect(target.textContent).toContain("2 of 2 spaces shown.");
  const archive = boxes()[1]!.querySelector("input")!;
  archive.click();
  flushSync();
  expect(choices.hidden.has("confluence/ARC")).toBe(true);
  expect(choices.ports.setHidden).toHaveBeenCalledWith("confluence/ARC", true);
  expect(target.textContent).toContain("1 of 2 spaces shown.");
  archive.click();
  flushSync();
  expect(choices.hidden.has("confluence/ARC")).toBe(false);
});
