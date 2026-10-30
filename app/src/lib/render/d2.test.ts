/**
 * D2 (its official WASM build) under vitest (#2259): the real compiler and
 * renderer. The package's browser build starts a `blob:` worker, which Node
 * has not, so the test hands `d2Renderer` the package's Node build, the same
 * WASM behind the same `compile`/`render` API. That build asks
 * `typeof window` while it starts, and under jsdom it would listen on its
 * worker the browser way and never hear an answer, so `window` is hidden
 * until it is ready.
 */
import { beforeAll, expect, test, vi } from "vitest";

import { d2Renderer, type D2Engine } from "./d2";
import type { Palette } from "./port";

const LIGHT: Palette = {
  dark: false, paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", ink: "#17212B", ink2: "#4B5663",
  rule: "#CBD2D9", mutedBg: "#E6E9EC", mark: "#F6E08F",
};
const DARK: Palette = {
  dark: true, paper: "#12171C", sheet: "#1A2027", sheet2: "#212932", ink: "#E4E9EE", ink2: "#A0ABB7",
  rule: "#323C47", mutedBg: "#262E37", mark: "#5C4E17",
};

/** The Node build ships no types of its own; its API is the browser build's. */
const NODE_BUILD = "../../../node_modules/@terrastruct/d2/dist/node-esm/index.js";

let node: D2Engine;
beforeAll(async () => {
  vi.stubGlobal("window", undefined);
  try {
    const { D2 } = (await import(/* @vite-ignore */ NODE_BUILD)) as typeof import("@terrastruct/d2");
    node = new D2() as unknown as D2Engine;
    await (node as unknown as { ready: Promise<void> }).ready;
  } finally {
    vi.unstubAllGlobals();
  }
});

const renderD2 = d2Renderer(() => node);

const svgOf = async (code: string, palette = LIGHT) => {
  const answer = await renderD2(code, palette);
  if (!("svg" in answer)) throw new Error(`no drawing: ${answer.error}`);
  return answer.svg;
};

test("a D2 diagram draws as SVG with its labels", async () => {
  const svg = await svgOf("api -> outbox: enqueue");
  expect(svg).toMatch(/^<svg[\s>]/);
  expect(svg).toContain(">api<");
  expect(svg).toContain(">enqueue<");
});

test("the drawing takes the palette's colours, and the dark palette draws dark", async () => {
  const code = "a -> b";
  const light = (await svgOf(code, LIGHT)).toLowerCase();
  const dark = (await svgOf(code, DARK)).toLowerCase();
  expect(light).toContain(`fill:${LIGHT.sheet2.toLowerCase()}`);
  expect(light).toContain(`fill:${LIGHT.ink.toLowerCase()}`);
  expect(dark).toContain(`fill:${DARK.sheet2.toLowerCase()}`);
  expect(dark).not.toContain(LIGHT.sheet2.toLowerCase());
});

test("a theme override the source sets wins over the palette's", async () => {
  const svg = (await svgOf('vars: { d2-config: { theme-overrides: { B6: "#112233" } } }\na -> b')).toLowerCase();
  expect(svg).toContain("fill:#112233");
});

test("renders queue one at a time on the one instance, and each gets its own drawing", async () => {
  const [one, two] = await Promise.all([svgOf("first -> x"), svgOf("second -> y")]);
  expect(one).toContain(">first<");
  expect(two).toContain(">second<");
});

test("a diagram that does not parse answers D2's errors by line, not a drawing", async () => {
  expect(await renderD2("a -> {", LIGHT)).toEqual({
    error: "line 1:1: connection missing destination\nline 1:6: maps must be terminated with }",
  });
  // The instance still draws after an error.
  expect(await svgOf("after -> error")).toContain(">after<");
});

test("an engine that never starts answers an error, and the next render starts it again", async () => {
  let starts = 0;
  const stuck = d2Renderer(() => {
    starts += 1;
    return starts === 1 ? new Promise<D2Engine>(() => {}) : node;
  }, 50);
  expect(await stuck("a -> b", LIGHT)).toEqual({ error: "D2 did not start within 0.05 s." });
  expect(await stuck("again -> b", LIGHT)).toEqual({ svg: expect.stringContaining(">again<") });
  expect(starts).toBe(2);
});
