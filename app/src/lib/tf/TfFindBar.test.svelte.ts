/** The find bar on a page with no box: the ladder opens it, and `Esc` goes back. jsdom lays nothing out, so where a match lands is the trips' (`find-ladder`). */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { installFindKey } from "./filter-keys";
import { pageContext } from "./kept-pages.svelte";
import { layers } from "./layers.svelte";
import TfFindBar from "./TfFindBar.svelte";

let host: HTMLElement;
let page: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let stopKey = (): void => {};

beforeEach(() => {
  host = document.createElement("div");
  page = document.createElement("div");
  page.innerHTML = "<p>retry once</p><p>retry twice</p><p>and <span>re</span>try again</p><button>retry</button>";
  document.body.append(host, page);
  stopKey = installFindKey();
});

afterEach(() => {
  stopKey();
  if (view) unmount(view);
  view = null;
  document.body.replaceChildren();
});

function draw(visible: () => boolean = () => true, props: Record<string, unknown> = {}): void {
  view = mount(TfFindBar, { target: host, context: pageContext(visible), props: { root: () => page, ...props } });
  flushSync();
}

async function pressFind(): Promise<void> {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true, cancelable: true }));
  await tick();
  await tick();
}

const box = (): HTMLInputElement => document.querySelector<HTMLInputElement>(".tf-findbar input")!;
const count = (): string | null | undefined => document.querySelector(".tf-findn")?.textContent;

function type(words: string): void {
  box().value = words;
  box().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function key(name: string, shiftKey = false): void {
  box().dispatchEvent(new KeyboardEvent("keydown", { key: name, shiftKey, bubbles: true, cancelable: true }));
  flushSync();
}

test("⌘F opens it with focus in its box, pressed again it selects the words, and Enter steps round", async () => {
  draw();
  expect(document.querySelector(".tf-findbar")).toBeNull();
  await pressFind();
  expect(document.activeElement).toBe(box());
  expect(count()).toBe("");
  type("retry");
  expect(count()).toBe("1 of 3");
  box().setSelectionRange(5, 5);
  await pressFind();
  expect(document.querySelectorAll(".tf-findbar")).toHaveLength(1);
  expect([box().selectionStart, box().selectionEnd]).toEqual([0, 5]);
  key("Enter");
  expect(count()).toBe("2 of 3");
  key("Enter", true);
  key("Enter", true);
  expect(count()).toBe("3 of 3");
  type("zebra");
  expect(count()).toBe("none on this page");
});

test("Esc closes it and gives focus back to what held it, a draft too, and comes off the layer stack", async () => {
  const draft = document.createElement("textarea");
  document.body.append(draft);
  draft.focus();
  draw();
  await pressFind();
  flushSync();
  expect(layers.open.map((layer) => layer.kind)).toEqual(["panel"]);
  type("retry");
  key("Escape");
  await tick();
  expect(document.querySelector(".tf-findbar")).toBeNull();
  expect(document.activeElement).toBe(draft);
  expect(layers.open).toEqual([]);
});

test("the selected text starts the box", async () => {
  draw();
  const words = page.querySelector("p")!.firstChild as Text;
  getSelection()!.setBaseAndExtent(words, 0, words, 5);
  await pressFind();
  expect(box().value).toBe("retry");
  expect([box().selectionStart, box().selectionEnd]).toEqual([0, 5]);
  expect(count()).toBe("1 of 3");
});

test("what the page draws next is counted, and the count says what it covers", async () => {
  let held = true;
  draw(() => true, { within: "in open files", more: () => held });
  await pressFind();
  type("retry");
  expect(count()).toBe("3+ in open files");
  held = false;
  page.insertAdjacentHTML("beforeend", "<p>retry drawn later</p>");
  await new Promise((done) => setTimeout(done, 40));
  flushSync();
  expect(count()).toBe("1 of 4 in open files");
});

test("the page going takes the bar with it, moved up to the scroller as it is", async () => {
  const main = document.createElement("main");
  main.className = "tf-main";
  main.style.overflowY = "auto";
  main.append(page);
  document.body.append(main);
  draw();
  await pressFind();
  expect(document.querySelector(".tf-findhold")?.parentElement).toBe(main);
  unmount(view!);
  view = null;
  await tick();
  expect(document.querySelector(".tf-findhold")).toBeNull();
  expect(layers.open).toEqual([]);
});

test("a hidden page's bar takes no key", async () => {
  draw(() => false);
  await pressFind();
  expect(document.querySelector(".tf-findbar")).toBeNull();
});
