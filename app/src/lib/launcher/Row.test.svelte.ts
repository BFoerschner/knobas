/**
 * The launcher row (#2421): three cells, the monogram coloured by the state's
 * tone, and everything drawn as text.
 *
 * Mounted directly; the cell rule per kind is `cells.ts`'s and is witnessed
 * through the launcher in `Launcher.test.svelte.ts`.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import Row from "./Row.svelte";

function render(over: Record<string, unknown> = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Row, {
    target,
    props: {
      monogram: "PG",
      kindName: "Page",
      cell: "Engineering",
      title: "SEPA payout retry design",
      id: "opt-0",
      selected: false,
      onopen: vi.fn(),
      onhover: vi.fn(),
      ...over,
    },
  });
  flushSync();
  return {
    target,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a row is the monogram, the key cell and the title, and nothing else", () => {
  const { target, done } = render();
  const row = target.querySelector(".tf-ln-res")!;
  expect([...row.children].map((cell) => cell.textContent)).toEqual(["Pg", "Engineering", "SEPA payout retry design"]);
  done();
});

test("a cell or title that looks like markup is drawn as text", () => {
  const { target, done } = render({ cell: "<b>ENG</b>", title: '<img src=x onerror="alert(1)">' });
  expect(target.textContent).toContain("<b>ENG</b>");
  expect(target.querySelectorAll("b, img")).toHaveLength(0);
  done();
});

test("the monogram carries the tone it is given, and none without one", () => {
  const toned = render({ tone: "ok", kindName: "Pull request: merged" });
  const mg = toned.target.querySelector<HTMLElement>(".tf-ln-mg")!;
  expect(mg.dataset["tone"]).toBe("ok");
  expect(mg.getAttribute("aria-label")).toBe("Pull request: merged");
  toned.done();

  const plain = render();
  expect(plain.target.querySelector(".tf-ln-mg")!.hasAttribute("data-tone")).toBe(false);
  plain.done();
});

test("only an identifier's cell is drawn in the mono face", () => {
  const key = render({ cell: "PAY-231", mono: true });
  expect(key.target.querySelector(".tf-ln-k")!.classList.contains("tf-ln-mono")).toBe(true);
  key.done();
  const place = render();
  expect(place.target.querySelector(".tf-ln-k")!.classList.contains("tf-ln-mono")).toBe(false);
  place.done();
});

test("a forge's issue key is cut to its repository and number, as a card cuts it, whole on hover (the live run, 2026-11-09)", () => {
  const issue = render({ cell: "tidewater/payout-service/issues/37", mono: true, title: "(reserved)" });
  const cell = issue.target.querySelector<HTMLElement>(".tf-ln-k")!;
  expect(cell.textContent).toBe("payout-service#37");
  expect(cell.title).toBe("tidewater/payout-service/issues/37");
  issue.done();
  const key = render({ cell: "PAY-231", mono: true });
  expect(key.target.querySelector<HTMLElement>(".tf-ln-k")!.hasAttribute("title")).toBe(false);
  key.done();
  // A place's cell is words, never a key.
  const place = render({ cell: "tidewater/payout-service/issues/37" });
  expect(place.target.querySelector(".tf-ln-k")!.textContent).toBe("tidewater/payout-service/issues/37");
  place.done();
});

/** A `Highlight` and `CSS.highlights` the test can read, which jsdom has neither of. */
function stubHighlights() {
  class FakeHighlight extends Set<Range> {}
  const registry = new Map<string, FakeHighlight>();
  vi.stubGlobal("Highlight", FakeHighlight);
  vi.stubGlobal("CSS", { highlights: registry });
  return {
    ranges: () => [...(registry.get("tf-pick") ?? [])].map((range) => range.toString()),
    restore: () => vi.unstubAllGlobals(),
  };
}

test("the matched letters are painted over the title's one text node, and a new answer keeps the node", () => {
  const paint = stubHighlights();
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    monogram: "RE",
    kindName: "Repository",
    cell: "acme",
    title: "ADProxy",
    marks: [2, 7] as number[],
    id: "opt-0",
    selected: false,
    onopen: vi.fn(),
    onhover: vi.fn(),
  });
  const app = mount(Row, { target, props });
  flushSync();
  const span = target.querySelector(".tf-ln-t")!;
  const node = span.firstChild;
  expect(paint.ranges()).toEqual(["Proxy"]);
  expect(span.childNodes).toHaveLength(1);

  // A second answer moves the marks, and the span and its text are the same nodes.
  props.marks = [0, 2, 3, 4];
  flushSync();
  expect(paint.ranges()).toEqual(["AD", "r"]);
  expect(target.querySelector(".tf-ln-t")).toBe(span);
  expect(span.firstChild).toBe(node);

  // No marks, none painted; and the row's going takes its ranges with it.
  props.marks = [];
  flushSync();
  expect(paint.ranges()).toEqual([]);
  props.marks = [2, 7];
  flushSync();
  unmount(app);
  expect(paint.ranges()).toEqual([]);
  target.remove();
  paint.restore();
});

test("where the webview has no Highlight API the row draws without marks", () => {
  vi.stubGlobal("Highlight", undefined);
  const { target, done } = render({ title: "ADProxy", marks: [2, 7] });
  expect(target.querySelector(".tf-ln-t")!.textContent).toBe("ADProxy");
  expect(target.querySelectorAll("mark")).toHaveLength(0);
  done();
  vi.unstubAllGlobals();
});

const why = {
  place: "Comment by Jonas Becker",
  segments: [
    { text: "…times out behind ", hit: false },
    { text: "ADProxy", hit: true },
    { text: ", so the retry", hit: false },
  ],
  code: false,
  count: null,
  excerpt: { place: "Comment by Jonas Becker", segments: [] },
};

test("a row with no evidence is one line, and one with it two, the monogram and key on the first (D10)", () => {
  const one = render();
  expect(one.target.querySelector(".tf-ln-why")).toBeNull();
  one.done();

  const two = render({ why });
  const row = two.target.querySelector(".tf-ln-res")!;
  const line = row.querySelector(".tf-ln-why")!;
  expect(line.parentElement).toBe(row);
  // After the title: grid order puts the monogram, key and title on the first line.
  expect([...row.children].map((c) => c.className.split(" ")[0])).toEqual(["tf-ln-mg", "tf-ln-k", "tf-ln-t", "tf-ln-why"]);
  expect(line.querySelector(".tf-ln-why-at")!.textContent).toBe("Comment by Jonas Becker");
  const mark = line.querySelector("mark")!;
  expect(mark.textContent).toBe("ADProxy");
  expect(line.querySelector(".tf-ln-why-t")!.textContent).toBe("Comment by Jonas Becker…times out behind ADProxy, so the retry");
  expect(line.classList.contains("tf-ln-code")).toBe(false);
  expect(line.querySelector(".tf-ln-why-n")).toBeNull();
  two.done();
});

test("a log's line is drawn mono, and a count stands in its own box", () => {
  const { target, done } = render({ why: { ...why, place: "Log line 212", code: true, count: "4 files" } });
  const line = target.querySelector(".tf-ln-why")!;
  expect(line.classList.contains("tf-ln-code")).toBe(true);
  expect(line.querySelector(".tf-ln-why-n")!.textContent).toBe("4 files");
  expect(line.querySelector(".tf-ln-why-t")!.contains(line.querySelector(".tf-ln-why-n"))).toBe(false);
  done();
});

test("an evidence line that looks like markup is drawn as text", () => {
  const { target, done } = render({ why: { ...why, segments: [{ text: "<img src=x>", hit: false }, { text: "<b>x</b>", hit: true }] } });
  expect(target.querySelectorAll("img, b")).toHaveLength(0);
  expect(target.querySelector("mark")!.textContent).toBe("<b>x</b>");
  done();
});
