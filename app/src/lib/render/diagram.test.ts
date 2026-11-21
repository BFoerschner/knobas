/**
 * A diagram keeps its room (#2510, `docs/decisions/2026-11-20-a-diagram-keeps-its-room.md`): the room a diagram takes
 * before its drawing is in, what a drawing does that lands after the page under it drew, and the warm-up. The engine
 * is a fake whose answers land when the test says; jsdom lays nothing out, so a fence's box is given by the test.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { WARM_DIAGRAMS, diagramWarm, drawDiagram, forgetDiagrams, warmDiagrams, type DiagramRoom } from "./diagram";
import { diagramKey, forgetDiagramSizes, keepDiagramSize, keptDiagramSize } from "./diagram-sizes";
import type { DiagramAnswer, DiagramRequest, RenderPort } from "./port";

const FLOW = "flowchart TB\n  a --> b\n  b --> c";

/** An engine whose answers land when the test settles them, in the order asked. */
function engine() {
  const asked: DiagramRequest[] = [];
  const waiting: { resolve(answer: DiagramAnswer): void; reject(error: Error): void }[] = [];
  const port: RenderPort = {
    highlight: () => ({ focus() {}, cancel() {} }),
    diagram(request) {
      asked.push(request);
      return new Promise((resolve, reject) => waiting.push({ resolve, reject }));
    },
  };
  const settle = async () => {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
  };
  return {
    port,
    asked,
    /** The oldest diagram still waiting is drawn `w` by `h`. */
    async land(w: number, h: number) {
      waiting.shift()!.resolve({ svg: `<svg viewBox="0 0 ${w} ${h}"><text>drawn</text></svg>` });
      await settle();
    },
    /** The oldest diagram still waiting gets `answer`. */
    resolve(answer: DiagramAnswer) {
      waiting.shift()!.resolve(answer);
    },
    async refuse(message: string) {
      waiting.shift()!.resolve({ error: message });
      await settle();
    },
    async fail(message: string) {
      waiting.shift()!.reject(new Error(message));
      await settle();
    },
    /** One diagram drawn and gone, so the engine has answered in this session. */
    async warm() {
      const scratch = document.createElement("div");
      const stop = drawDiagram("mermaid", "flowchart LR\n  warm --> up", scratch, port);
      await this.land(10, 10);
      stop();
      asked.length = 0;
    },
  };
}

let host: HTMLElement;
const stops: (() => void)[] = [];

beforeEach(() => {
  forgetDiagramSizes();
  host = document.createElement("div");
  document.body.append(host);
});

afterEach(() => {
  for (const stop of stops.splice(0)) stop();
  host.remove();
  forgetDiagramSizes();
});

/** A Markdown fence as `markdown.ts` builds it, holding `code`: its header with *Copy*, and its body. */
function fence(code = FLOW) {
  const block = document.createElement("div");
  block.className = "tf-fence";
  block.innerHTML = '<header><span>mermaid</span><button type="button" class="tf-fence-copy">Copy</button></header><div class="tf-fence-body"><pre><code></code></pre></div>';
  block.querySelector("code")!.textContent = code;
  host.append(block);
  const body = block.querySelector<HTMLElement>(".tf-fence-body")!;
  const header = block.querySelector<HTMLElement>("header")!;
  return { block, body, header, frame: () => body.querySelector<HTMLIFrameElement>("iframe"), full: () => header.querySelector<HTMLButtonElement>(".tf-dfull") };
}

const boxAt = (top: number, height: number, width = 900, left = 0) =>
  ({ x: left, y: top, top, left, width, height, right: left + width, bottom: top + height, toJSON: () => ({}) }) as DOMRect;

/**
 * Gives `body` a laid-out box, its top and height, in a window 800 px high, and puts a paragraph under its fence,
 * 16 px below it, as a text that goes on. Answers the paragraph.
 */
function layout(body: HTMLElement, top: number, height: number, width = 900): HTMLElement {
  vi.stubGlobal("innerHeight", 800);
  body.getBoundingClientRect = () => boxAt(top, height, width);
  Object.defineProperty(body, "clientWidth", { value: width, configurable: true });
  const under = document.createElement("p");
  under.textContent = "Under it.";
  under.getBoundingClientRect = () => boxAt(top + height + 16, 24, width);
  body.parentElement!.after(under);
  return under;
}

afterEach(() => vi.unstubAllGlobals());

const draw = (body: HTMLElement, port: RenderPort, room: DiagramRoom | null, code = FLOW) => {
  const stop = drawDiagram("mermaid", code, body, port, room);
  stops.push(stop);
  return stop;
};

/** The sides the frame's document draws its image at. */
const drawnAt = (frame: HTMLIFrameElement) => {
  const img = new DOMParser().parseFromString(frame.srcdoc, "text/html").querySelector("img")!;
  return [img.getAttribute("width"), img.getAttribute("height")];
};

test("a diagram seen before stands as an empty frame of the size it drew at, and its drawing lands in that frame", async () => {
  const { port, land } = engine();
  keepDiagramSize(diagramKey("mermaid", FLOW, ""), { w: 200, h: 226 });
  const { body, header, frame } = fence();
  layout(body, 100, 125);
  const held = vi.fn(() => () => {});
  draw(body, port, { hold: held, actions: header });
  // At once, in the task the text is put on the page in: the source is gone and the room is the drawing's.
  const room = frame()!;
  expect([room.width, room.height, room.hasAttribute("srcdoc")]).toEqual(["200", "226", false]);
  expect(body.querySelector("pre")).toBeNull();
  expect(held).not.toHaveBeenCalled();
  await land(200, 226);
  expect(frame()).toBe(room);
  expect(room.srcdoc).toContain("<img");
  expect(body.classList.contains("tf-roomkept")).toBe(false);
  expect(body.style.height).toBe("");
});

test("a drawing made earlier in the session is drawn at once at its own size, and no engine is asked again", async () => {
  const { port, asked, land } = engine();
  const one = fence();
  draw(one.body, port, { actions: one.header });
  await land(200, 226);
  expect(asked).toHaveLength(1);
  const two = fence();
  // In view with the page under it: a first draw still takes its own room, since nothing of the text is drawn yet.
  layout(two.body, 100, 125);
  draw(two.body, port, { actions: two.header });
  expect([two.frame()!.width, two.frame()!.height]).toEqual(["200", "226"]);
  expect(two.frame()!.srcdoc).toContain("<img");
  expect(two.body.style.height).toBe("");
  expect(asked).toHaveLength(1);
  // Another engine's drawings are its own.
  const other = engine();
  const three = fence();
  draw(three.body, other.port, { actions: three.header });
  expect(other.asked).toHaveLength(1);
});

test("the session keeps its latest 64 drawings, and no more of them than 6 million characters", async () => {
  const many = engine();
  for (let n = 0; n <= 64; n += 1) {
    const f = fence(`flowchart LR\n  n${n}`);
    draw(f.body, many.port, null, `flowchart LR\n  n${n}`);
    await many.land(100, 40);
    f.block.remove();
  }
  expect(many.asked).toHaveLength(65);
  // The newest is drawn from the session; the oldest went to make room and is asked for again.
  for (const [n, asked] of [[64, 65], [0, 66]] as const) {
    const f = fence(`flowchart LR\n  n${n}`);
    draw(f.body, many.port, null, `flowchart LR\n  n${n}`);
    expect(many.asked, `n${n}`).toHaveLength(asked);
  }

  const large = engine();
  const big = async (code: string) => {
    const f = fence(code);
    draw(f.body, large.port, null, code);
    large.resolve({ svg: `<svg viewBox="0 0 10 10"><text>${"x".repeat(1_900_000)}</text></svg>` });
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
  };
  for (const code of ["big a", "big b", "big c"]) await big(code);
  expect(large.asked).toHaveLength(3);
  for (const [code, asked] of [["big c", 3], ["big b", 3], ["big a", 4]] as const) {
    const f = fence(code);
    draw(f.body, large.port, null, code);
    expect(large.asked, code).toHaveLength(asked);
  }
});

test("never seen: a cold engine holds nothing; a warm one holds what stands from the block down until its answer", async () => {
  const cold = engine();
  const a = fence();
  const holdA = vi.fn(() => () => {});
  draw(a.body, cold.port, { hold: holdA, actions: a.header });
  expect(holdA).not.toHaveBeenCalled();
  expect(a.body.querySelector("pre")).not.toBeNull();

  const warm = engine();
  await warm.warm();
  const b = fence();
  const released = vi.fn();
  const holdB = vi.fn(() => released);
  draw(b.body, warm.port, { hold: holdB, actions: b.header });
  expect(holdB.mock.calls).toEqual([[b.body]]);
  expect(released).not.toHaveBeenCalled();
  await warm.land(200, 226);
  expect(released).toHaveBeenCalledTimes(1);

  // A parser's error ends the hold too, and so does the text going away.
  for (const end of ["refuse", "stop"] as const) {
    const c = fence(`${FLOW}\n  ${end}`);
    const over = vi.fn();
    const stop = draw(c.body, warm.port, { hold: () => over, actions: c.header }, `${FLOW}\n  ${end}`);
    if (end === "refuse") await warm.refuse("Parse error");
    else stop();
    expect(over, end).toHaveBeenCalledTimes(1);
  }
});

test("a drawing that lands with the page under it drawn keeps its source's room: whole, scaled down, full size on a press", async () => {
  const { port, land } = engine();
  const { body, header, frame, full } = fence();
  layout(body, 100, 125);
  draw(body, port, { actions: header });
  expect(body.querySelector("pre")).not.toBeNull();
  await land(200, 226);
  // The room is the source's, to the pixel, and the drawing is whole inside it at one half: 113 px of height for 226.
  expect(body.style.height).toBe("125px");
  expect(body.classList.contains("tf-roomkept")).toBe(true);
  expect([frame()!.width, frame()!.height]).toEqual(["100", "113"]);
  expect(drawnAt(frame()!)).toEqual(["100", "113"]);
  expect(frame()!.parentElement!.classList.contains("tf-dfit")).toBe(true);
  // Its size is kept as drawn, not as fitted: the next open gives it its own room.
  expect(keptDiagramSize(diagramKey("mermaid", FLOW, ""))).toEqual({ w: 200, h: 226 });
  // *Show at full size* stands beside *Copy*, before it.
  expect([...header.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Show at full size", "Copy"]);
  full()!.focus();
  full()!.click();
  expect(body.style.height).toBe("");
  expect(body.classList.contains("tf-roomkept")).toBe(false);
  expect([frame()!.width, frame()!.height]).toEqual(["200", "226"]);
  expect(drawnAt(frame()!)).toEqual(["200", "226"]);
  expect(full()).toBeNull();
  expect(document.activeElement).toBe(header.querySelector(".tf-fence-copy"));
});

test("a fence whose room came kept (its text drawn again) stands as it stood: fitted, from the session or when the drawing lands", async () => {
  const { port, asked, land } = engine();
  const one = fence();
  layout(one.body, 100, 125);
  draw(one.body, port, { actions: one.header });
  await land(200, 226);
  // The text is drawn again: a new fence, handed the room the old one was kept at (`keepFenceRooms`).
  const again = () => {
    const f = fence();
    f.body.style.height = "125px";
    f.body.classList.add("tf-roomkept");
    return f;
  };
  const two = again();
  draw(two.body, port, { actions: two.header });
  // At once, from the session's drawing, in the same room and at the same scale: nothing under it moves.
  expect([two.frame()!.width, two.frame()!.height, two.body.style.height]).toEqual(["100", "113", "125px"]);
  expect(two.frame()!.srcdoc).toContain("<img");
  expect(two.full()).not.toBeNull();
  expect(asked).toHaveLength(1);
  two.full()!.click();
  expect([two.frame()!.width, two.frame()!.height, two.body.style.height]).toEqual(["200", "226", ""]);

  // With no drawing in the session (another engine's), the source stays in the kept room until the drawing lands,
  // though the drawing's size is kept: an empty frame of that size would not fit the room.
  const other = engine();
  const three = again();
  layout(three.body, 100, 125);
  draw(three.body, other.port, { actions: three.header });
  expect([three.frame(), three.body.querySelector("pre") !== null, three.body.style.height]).toEqual([null, true, "125px"]);
  await other.land(200, 226);
  expect([three.frame()!.width, three.frame()!.height, three.body.style.height]).toEqual(["100", "113", "125px"]);
});

test("a press on the fitted drawing itself shows it at full size", async () => {
  const { port, land } = engine();
  const { body, header, frame, full } = fence();
  layout(body, 100, 125);
  draw(body, port, { actions: header });
  await land(200, 226);
  frame()!.parentElement!.click();
  expect([frame()!.width, frame()!.height, body.style.height]).toEqual(["200", "226", ""]);
  expect(full()).toBeNull();
});

test("a late drawing that whole would be a speck shows its top at its own size, cut at the room's foot, with Show all", async () => {
  const { port, land } = engine();
  const { body, header, frame, full } = fence();
  // A four-line source's room and a tall drawing: whole it would be 70 px of 500, 14%.
  layout(body, 100, 82);
  draw(body, port, { actions: header });
  await land(128, 500);
  expect(body.style.height).toBe("82px");
  expect([frame()!.width, frame()!.height]).toEqual(["128", "500"]);
  expect(frame()!.parentElement!.className).toBe("tf-diagram tf-dcut");
  expect([...header.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Show all", "Copy"]);
  full()!.click();
  expect([frame()!.width, frame()!.height, body.style.height, full()]).toEqual(["128", "500", "", null]);

  // At the floor it still stands whole: 40% of 250 is the room's 100 px.
  const edge = engine();
  const b = fence("flowchart TB\n  edge");
  layout(b.body, 100, 112);
  draw(b.body, edge.port, { actions: b.header }, "flowchart TB\n  edge");
  await edge.land(100, 250);
  expect([b.frame()!.width, b.frame()!.height, b.frame()!.parentElement!.className, b.full()!.textContent]).toEqual(["40", "100", "tf-diagram tf-dfit", "Show at full size"]);

  // Cut and wider than the room: its own size no more, the room's width.
  const wide = engine();
  const c = fence("flowchart TB\n  wide");
  layout(c.body, 100, 82, 436);
  draw(c.body, wide.port, { actions: c.header }, "flowchart TB\n  wide");
  await wide.land(800, 2000);
  expect([c.frame()!.width, c.frame()!.height, c.frame()!.parentElement!.className]).toEqual(["400", "1000", "tf-diagram tf-dcut"]);
  expect(drawnAt(c.frame()!)).toEqual(["400", "1000"]);
});

test("a late drawing wider than its room is scaled to the room's width", async () => {
  const { port, land } = engine();
  const { body, header, frame } = fence();
  layout(body, 100, 125, 436);
  draw(body, port, { actions: header });
  await land(800, 100);
  // 436 px less the room's sides leaves 400 for 800: half size, though its height would fit.
  expect([frame()!.width, frame()!.height]).toEqual(["400", "50"]);
});

test("a late drawing smaller than its room stands at its own size inside it, with nothing to press", async () => {
  const { port, land } = engine();
  const { body, header, frame, full } = fence();
  layout(body, 100, 125);
  draw(body, port, { actions: header });
  await land(320, 86);
  expect(body.style.height).toBe("125px");
  expect([frame()!.width, frame()!.height]).toEqual(["320", "86"]);
  expect(frame()!.parentElement!.classList.contains("tf-dfit")).toBe(false);
  expect(full()).toBeNull();
});

test("where nothing seen stands under it, a late drawing takes its own size at once", async () => {
  /** `f`'s fence inside a box that scrolls, and a paragraph the page draws under that box. */
  const scrolled = (f: ReturnType<typeof fence>) => {
    layout(f.body, 100, 125).remove();
    const scroller = document.createElement("div");
    scroller.style.overflowY = "auto";
    f.block.replaceWith(scroller);
    scroller.append(f.block);
    const outside = document.createElement("p");
    outside.getBoundingClientRect = () => boxAt(300, 24);
    scroller.after(outside);
  };
  const cases: [string, (f: ReturnType<typeof fence>) => DiagramRoom | null][] = [
    // What stands under it is held unseen (the text's hold, or the page's under the text).
    ["held", (f) => (layout(f.body, 100, 125).classList.add("tf-heldbelow"), { actions: f.header })],
    // What stands under it is below the window.
    ["below", (f) => (layout(f.body, 700, 125), { actions: f.header })],
    // Not laid out: a page kept hidden.
    ["hidden", (f) => (layout(f.body, 0, 0), { actions: f.header })],
    // A diagram file: nothing of the file stands under it.
    ["alone", (f) => (layout(f.body, 100, 125), null)],
    // The last thing of its text, with room left in the window under it; the mark a long text ends in has no height.
    ["last", (f) => ((layout(f.body, 100, 125).getBoundingClientRect = () => boxAt(241, 0)), { actions: f.header })],
    // What comes after it stands beside it, in another column.
    ["beside", (f) => ((layout(f.body, 100, 125, 600).getBoundingClientRect = () => boxAt(300, 24, 200, 640)), { actions: f.header })],
    // What comes after it starts above its bottom: a rail beside it that runs further down.
    ["rail", (f) => ((layout(f.body, 100, 125).getBoundingClientRect = () => boxAt(60, 600)), { actions: f.header })],
    // What stands under the box that scrolls it does not move with it.
    ["scrolled", (f) => (scrolled(f), { actions: f.header })],
  ];
  for (const [name, room] of cases) {
    // Never seen each time: a size kept by the case before would give this one its room from the start.
    forgetDiagramSizes();
    host.replaceChildren();
    const { port, land } = engine();
    const f = fence();
    expect(f.body.querySelector("pre"), name).not.toBeNull();
    draw(f.body, port, room(f));
    await land(200, 226);
    expect([f.frame()!.width, f.frame()!.height, f.body.style.height, f.full()], name).toEqual(["200", "226", "", null]);
  }
});

test("a kept size that proved wrong: the drawing is fitted into the room it was given, and the new size kept", async () => {
  const { port, land } = engine();
  const key = diagramKey("mermaid", FLOW, "");
  keepDiagramSize(key, { w: 120, h: 40 });
  const { body, header, frame, full } = fence();
  // The stand-in's room: 40 px and the holder's 18 above and below.
  layout(body, 100, 76);
  draw(body, port, { actions: header });
  expect([frame()!.width, frame()!.height]).toEqual(["120", "40"]);
  await land(240, 80);
  expect(body.style.height).toBe("76px");
  expect([frame()!.width, frame()!.height]).toEqual(["192", "64"]);
  expect(full()).not.toBeNull();
  expect(keptDiagramSize(key)).toEqual({ w: 240, h: 80 });
});

test("what stands under the text's own box counts: a later comment the page draws under it", async () => {
  const { port, land } = engine();
  const { block, body, header, frame } = fence();
  // The text's own blocks under the fence are held unseen, as inside a hold; the page's next comment is not.
  layout(body, 100, 125).classList.add("tf-heldbelow");
  const text = document.createElement("div");
  block.replaceWith(text);
  text.append(block, ...host.querySelectorAll("p"));
  const next = document.createElement("article");
  next.getBoundingClientRect = () => boxAt(300, 80);
  text.after(next);
  draw(body, port, { actions: header });
  await land(200, 226);
  expect([frame()!.width, frame()!.height, body.style.height]).toEqual(["100", "113", "125px"]);
});

test("a parser's error that lands late stays inside the room; with nothing drawn under it, it shows whole", async () => {
  const late = engine();
  const a = fence();
  layout(a.body, 100, 125);
  draw(a.body, late.port, { actions: a.header });
  await late.refuse("Parse error on line 2");
  expect(a.body.style.height).toBe("125px");
  expect(a.body.classList.contains("tf-roomkept")).toBe(true);
  expect(a.body.querySelector(".tf-stepfail pre")?.textContent).toBe("Parse error on line 2");
  expect(a.body.querySelector("pre code")?.textContent).toBe(FLOW);

  const early = engine();
  const b = fence();
  layout(b.body, 700, 125);
  draw(b.body, early.port, { actions: b.header });
  await early.fail("The diagram worker stopped.");
  expect([b.body.style.height, b.body.classList.contains("tf-roomkept")]).toEqual(["", false]);
  expect(b.body.querySelector(".tf-stepfail pre")?.textContent).toBe("The diagram worker stopped.");
});

test("a theme switch draws again in the room the drawing has: a fitted one stays fitted, one at its own size keeps its frame", async () => {
  const html = document.documentElement;
  const dark = async () => {
    html.style.setProperty("--sheet", "#1A2027");
    html.style.setProperty("--ink", "#E4E9EE");
    html.setAttribute("data-theme", "dark");
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
  };
  const light = () => {
    html.removeAttribute("data-theme");
    html.style.removeProperty("--sheet");
    html.style.removeProperty("--ink");
  };
  try {
    const fitted = engine();
    const a = fence();
    layout(a.body, 100, 125);
    draw(a.body, fitted.port, { actions: a.header });
    await fitted.land(200, 226);
    const own = engine();
    const b = fence();
    draw(b.body, own.port, { actions: b.header });
    await own.land(200, 226);
    const frame = b.frame()!;
    await dark();
    expect([fitted.asked.map((request) => request.palette.dark), own.asked.map((request) => request.palette.dark)]).toEqual([[false, true], [false, true]]);
    await fitted.land(200, 226);
    await own.land(200, 226);
    expect([a.frame()!.width, a.frame()!.height, a.body.style.height]).toEqual(["100", "113", "125px"]);
    expect(a.header.querySelectorAll(".tf-dfull")).toHaveLength(1);
    expect(b.frame()).toBe(frame);
    expect(frame.srcdoc).toContain('content="dark"');
    // The size it drew at in the light theme is its room in the dark one: an engine is handed the theme as colours.
    const next = engine();
    const c = fence();
    draw(c.body, next.port, { actions: c.header });
    expect([c.frame()!.width, c.frame()!.height, c.frame()!.hasAttribute("srcdoc")]).toEqual(["200", "226", false]);
  } finally {
    light();
  }
});

test("a drawing nobody waits for any more is still kept: its size, and the drawing for the session", async () => {
  const { port, asked, land } = engine();
  const a = fence();
  const stop = draw(a.body, port, { actions: a.header });
  stop();
  await land(200, 226);
  expect(a.body.querySelector("pre")).not.toBeNull();
  expect(keptDiagramSize(diagramKey("mermaid", FLOW, ""))).toEqual({ w: 200, h: 226 });
  const b = fence();
  draw(b.body, port, { actions: b.header });
  expect(b.frame()!.srcdoc).toContain("<img");
  expect(asked).toHaveLength(1);
});

test("the warm-up draws one small Mermaid diagram an idle moment, puts nothing on the page, and leaves the engine warm", async () => {
  const { port, asked, land } = engine();
  const idle: (() => void)[] = [];
  const later = (go: () => void) => void idle.push(go);
  const before = document.body.innerHTML;
  expect(diagramWarm(port, "mermaid")).toBe(false);
  warmDiagrams(port, later);
  warmDiagrams(port, later);
  // Nothing until the app is idle, then one diagram, and the next only once that one is in and the app idle again.
  expect([asked.length, idle.length]).toEqual([0, 1]);
  idle.shift()!();
  expect(asked.map((request) => [request.engine, request.code])).toEqual([["mermaid", WARM_DIAGRAMS[0]]]);
  expect(idle).toHaveLength(0);
  await land(10, 10);
  expect(diagramWarm(port, "mermaid")).toBe(true);
  expect([diagramWarm(port, "d2"), diagramWarm(port, "graphviz")]).toEqual([false, false]);
  expect(idle).toHaveLength(1);
  idle.shift()!();
  expect(asked.map((request) => [request.engine, request.code])).toEqual(WARM_DIAGRAMS.map((code) => ["mermaid", code]));
  await land(10, 10);
  expect(idle).toHaveLength(0);
  expect(document.body.innerHTML).toBe(before);

  // An engine that fails to load costs the warmth, never the app: the next kind is still tried, and nothing is warm.
  const broken = engine();
  warmDiagrams(broken.port, later);
  idle.shift()!();
  await broken.fail("no chunk");
  expect(diagramWarm(broken.port, "mermaid")).toBe(false);
  expect(idle).toHaveLength(1);
  forgetDiagrams(port);
  expect(diagramWarm(port, "mermaid")).toBe(false);
});

test("a port with no diagram engine is not warmed, and its diagrams stay source", () => {
  const plain: RenderPort = { highlight: () => ({ focus() {}, cancel() {} }) };
  const idle: (() => void)[] = [];
  warmDiagrams(plain, (go) => void idle.push(go));
  expect(idle).toHaveLength(0);
  const { body } = fence();
  draw(body, plain, null);
  expect(body.querySelector("pre code")?.textContent).toBe(FLOW);
});
