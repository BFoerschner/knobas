/** The find bar on a page with no box: the ladder opens it, and `Esc` goes back. jsdom lays nothing out, so where a match lands is the trips' (`find-ladder`). */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { arrival } from "./arrival.svelte";
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

// -- opening at the match (the everything-search decisions, D12) --

async function settle(): Promise<void> {
  flushSync();
  await tick();
  await tick();
  flushSync();
}

test("a launcher row's arrival opens the bar on the match in the quoted passage", async () => {
  location.hash = "#/ticket/t1";
  page.innerHTML = "<p>The retry budget is fine.</p><p>Comment: it times out behind the proxy, so the retry budget stops.</p><p>retry</p>";
  arrival.set("#/ticket/t1", { words: ["retry"], passage: "times out behind the proxy, so the retry budget stops." });
  draw();
  await settle();
  expect(box().value).toBe("retry");
  expect(count()).toBe("2 of 3");
  // Taken: the page shown again opens nothing.
  expect(arrival.waiting("#/ticket/t1")).toBe(false);
});

test("an arrival whose passage the page does not draw opens on the first match", async () => {
  location.hash = "#/ticket/t2";
  arrival.set("#/ticket/t2", { words: ["retry"], passage: "a passage this page never drew, with retry in it" });
  draw();
  await settle();
  expect(count()).toBe("1 of 3");
});

test("words the page holds only apart are found by the first of them", async () => {
  location.hash = "#/ticket/t3";
  arrival.set("#/ticket/t3", { words: ["twice", "once"], passage: "retry twice" });
  draw();
  await settle();
  expect(box().value).toBe("twice");
  expect(count()).toBe("1 of 1");
});

test("an arrival for another address, or none, opens nothing", async () => {
  location.hash = "#/ticket/t4";
  arrival.set("#/ticket/other", { words: ["retry"], passage: "retry once" });
  draw();
  await settle();
  expect(document.querySelector(".tf-findbar")).toBeNull();
  arrival.clear();
});

test("a passage drawn after the bar opened becomes current, until the reader steps", async () => {
  location.hash = "#/ticket/t5";
  arrival.set("#/ticket/t5", { words: ["retry"], passage: "the late retry paragraph is here" });
  draw();
  await settle();
  expect(count()).toBe("1 of 3");
  const late = document.createElement("p");
  late.textContent = "the late retry paragraph is here";
  page.append(late);
  await new Promise((done) => setTimeout(done, 40));
  flushSync();
  expect(count()).toBe("4 of 4");
});
