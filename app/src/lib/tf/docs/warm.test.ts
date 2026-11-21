import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { Fonts, Role, SizeListener } from "../fonts.svelte";
import { MOST_CHARS, PIECE_CHARS, QUIET_MS, documentRoom, piecesOf, warmDocs, type WarmClock } from "./warm";

/** Fonts that only hear: `change(role)` is a size step in Settings › Fonts. */
function fakeFonts() {
  const heard = new Set<SizeListener>();
  const of = { onSize: (one: SizeListener) => (heard.add(one), () => heard.delete(one)) } as unknown as Fonts;
  return { of, change: (role: Role) => heard.forEach((one) => one(role, 1.2)), listeners: () => heard.size };
}

/** A clock the test runs: the waits and the idle callbacks pending, each cancellable. */
function fakeClock() {
  const waits: { go: () => void; ms: number; live: boolean }[] = [];
  const idles: { go: () => void; live: boolean }[] = [];
  const clock: WarmClock = {
    after(go, ms) {
      const one = { go, ms, live: true };
      waits.push(one);
      return () => (one.live = false);
    },
    idle(go) {
      const one = { go, live: true };
      idles.push(one);
      return () => (one.live = false);
    },
  };
  const live = <T extends { live: boolean }>(all: T[]) => all.filter((one) => one.live);
  return {
    clock,
    waiting: () => live(waits).length,
    idling: () => live(idles).length,
    /** Ends every wait pending. */
    wait() {
      for (const one of live(waits)) {
        one.live = false;
        one.go();
      }
    },
    /** Runs one idle callback: one task. */
    task() {
      const one = live(idles)[0];
      if (!one) return false;
      one.live = false;
      one.go();
      return true;
    },
  };
}

/** The shell's main pane with kept pages, the first shown, each a Docs page of `paragraphs` paragraphs of 500 characters. */
function pages(...hidden: boolean[]) {
  const main = document.createElement("main");
  main.className = "tf-main";
  const made = hidden.map((isHidden, at) => {
    const kept = document.createElement("div");
    kept.className = "tf-kept";
    kept.hidden = isHidden;
    kept.innerHTML = `<div class="tf-keptpage"><div class="tf-rd"><section class="tf-scroller tf-rd-main" id="scroller-${at}"><div class="tf-sclip"><div class="tf-doc">
      ${Array.from({ length: 30 }, (_, n) => `<p id="p-${at}-${n}">${"w".repeat(490)} page${at}:${n}</p>`).join("")}
      <section data-sec="1"><h2 id="s-${at}-1">Heading</h2>${Array.from({ length: 12 }, (_, n) => `<p>${"x".repeat(490)} page${at}:${n}<img src="a.png"></p>`).join("")}<iframe src="about:blank"></iframe></section>
      <div class="tf-doc-more"></div>
    </div></div></section><textarea>draft words</textarea></div></div>`;
    main.append(kept);
    return kept;
  });
  document.body.append(main);
  return { main, made };
}

/** Every box the warm put in `<body>`, as it was put there, and whether one is left. */
function watchHosts() {
  const hosts: HTMLElement[] = [];
  const append = document.body.append.bind(document.body);
  const spy = vi.spyOn(document.body, "append").mockImplementation((...nodes: (string | Node)[]) => {
    for (const node of nodes) if (node instanceof HTMLElement && node.classList.contains("tf-warm")) hosts.push(node.cloneNode(true) as HTMLElement);
    append(...nodes);
  });
  return { hosts, left: () => document.querySelectorAll(".tf-warm").length, restore: () => spy.mockRestore() };
}

beforeEach(() => document.body.replaceChildren());
afterEach(() => vi.restoreAllMocks());

test("only Reading's size warms, once it has held still, and only the hidden Docs pages", () => {
  const { of, change } = fakeFonts();
  const time = fakeClock();
  pages(false, true);
  const watch = watchHosts();
  const stop = warmDocs(of, documentRoom, time.clock);
  change("ui");
  change("code");
  change("head");
  expect(time.waiting()).toBe(0);
  change("read");
  expect(time.waiting()).toBe(1);
  time.wait();
  while (time.task());
  expect(watch.hosts.length).toBeGreaterThan(0);
  // Every copy is the hidden page's: the shown one's paragraphs are never copied.
  for (const host of watch.hosts) expect(host.textContent).not.toContain("page0:");
  expect(watch.hosts.some((host) => host.textContent!.includes("page1:"))).toBe(true);
  stop();
});

test("three changes inside the quiet warm once, at the last size; a change mid-warm lays out nothing more of the last", () => {
  const { of, change } = fakeFonts();
  const time = fakeClock();
  pages(false, true);
  const watch = watchHosts();
  warmDocs(of, documentRoom, time.clock);
  change("read");
  change("read");
  change("read");
  expect(time.waiting()).toBe(1);
  time.wait();
  expect(time.task()).toBe(true);
  expect(watch.hosts).toHaveLength(1);
  // A step mid-warm: the warm in flight stops, and the next waits for the quiet again.
  change("read");
  expect(time.idling()).toBe(0);
  expect(time.task()).toBe(false);
  expect(watch.hosts).toHaveLength(1);
  expect(time.waiting()).toBe(1);
});

test("one piece a task, about two thousand characters, the whole capped", () => {
  const { made } = pages(true);
  const { pieces, chars } = piecesOf(made[0]!, MOST_CHARS);
  expect(pieces.length).toBeGreaterThan(5);
  for (const piece of pieces) {
    const size = piece.nodes.reduce((all, node) => all + (node.textContent?.length ?? 0), 0);
    expect(size).toBeLessThan(PIECE_CHARS + 600);
  }
  // The section of twelve long paragraphs is cut into its own paragraphs, under it.
  expect(pieces.some((piece) => piece.within?.matches("section[data-sec]"))).toBe(true);
  expect(chars).toBeLessThanOrEqual(MOST_CHARS);
  // The cap across pages: a small budget takes the first pieces only.
  const few = piecesOf(made[0]!, PIECE_CHARS);
  expect(few.pieces).toHaveLength(1);
  expect(few.chars).toBeLessThan(PIECE_CHARS + 600);

  const { of, change } = fakeFonts();
  const time = fakeClock();
  const watch = watchHosts();
  warmDocs(of, documentRoom, time.clock);
  change("read");
  time.wait();
  let tasks = 0;
  while (time.task()) {
    tasks += 1;
    // One piece a task, and nothing of it left behind.
    expect(watch.hosts).toHaveLength(tasks);
    expect(watch.left()).toBe(0);
  }
  expect(tasks).toBe(pieces.length);
  expect(QUIET_MS).toBe(300);
});

test("a copy holds no id, image, frame or text box, and the kept page is left as it was", () => {
  const { made } = pages(true);
  const page = made[0]!;
  const scroller = page.querySelector<HTMLElement>(".tf-scroller")!;
  scroller.scrollTop = 40;
  const before = page.outerHTML;
  const { of, change } = fakeFonts();
  const time = fakeClock();
  const watch = watchHosts();
  warmDocs(of, documentRoom, time.clock);
  change("read");
  time.wait();
  while (time.task());
  expect(watch.hosts.length).toBeGreaterThan(0);
  for (const host of watch.hosts) {
    expect(host.querySelector("[id], img, iframe, textarea, [hidden]")).toBeNull();
    // Its rules' ancestors, so the page's own CSS draws the copy.
    expect(host.querySelector(".tf-main > .tf-kept .tf-rd-main .tf-doc")).not.toBeNull();
    expect(host.getAttribute("aria-hidden")).toBe("true");
  }
  expect(watch.left()).toBe(0);
  expect(page.outerHTML).toBe(before);
  expect(page.hidden).toBe(true);
  expect(scroller.scrollTop).toBe(40);
  expect(page.querySelector("textarea")!.value).toBe("draft words");
});

test("a page shown, or read again, since the warm started is left alone", () => {
  const { made } = pages(true, true);
  const { of, change } = fakeFonts();
  const time = fakeClock();
  const watch = watchHosts();
  warmDocs(of, documentRoom, time.clock);
  change("read");
  time.wait();
  // The most recently shown page (the last held) is warmed first: show it now.
  made[1]!.hidden = false;
  // The other is read again: its body is new.
  made[0]!.querySelector(".tf-doc")!.replaceChildren(document.createElement("p"));
  while (time.task());
  expect(watch.hosts).toHaveLength(0);
});
