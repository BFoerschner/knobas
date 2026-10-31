/**
 * Graphviz (viz.js, its WASM build) under vitest (#2259): the real engine.
 * Node runs WASM, so what is witnessed is what the app's worker gets back:
 * the drawing, the palette's colours as defaults, and the parser's errors.
 */
import { expect, test } from "vitest";

import { renderGraphviz } from "./graphviz";
import type { Palette } from "./port";

const LIGHT: Palette = {
  dark: false, paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", ink: "#17212B", ink2: "#4B5663",
  rule: "#CBD2D9", mutedBg: "#E6E9EC", mark: "#F6E08F",
};
const DARK: Palette = {
  dark: true, paper: "#12171C", sheet: "#1A2027", sheet2: "#212932", ink: "#E4E9EE", ink2: "#A0ABB7",
  rule: "#323C47", mutedBg: "#262E37", mark: "#5C4E17",
};

const svgOf = async (code: string, palette = LIGHT) => {
  const answer = await renderGraphviz(code, palette);
  if (!("svg" in answer)) throw new Error(`no drawing: ${answer.error}`);
  return answer.svg;
};

test("a digraph draws as SVG with its node and edge labels", async () => {
  const svg = await svgOf('digraph { api -> outbox [label="enqueue"] }');
  expect(svg).toMatch(/<svg[\s>]/);
  expect(svg).toContain(">api<");
  expect(svg).toContain(">enqueue<");
});

test("the drawing takes the palette's colours, and the dark palette draws dark", async () => {
  const code = "digraph { a -> b }";
  const light = (await svgOf(code, LIGHT)).toLowerCase();
  const dark = (await svgOf(code, DARK)).toLowerCase();
  expect(light).toContain(`fill="${LIGHT.sheet2.toLowerCase()}"`);
  expect(light).toContain(`stroke="${LIGHT.ink2.toLowerCase()}"`);
  expect(dark).toContain(`fill="${DARK.sheet2.toLowerCase()}"`);
  expect(dark).not.toContain(LIGHT.sheet2.toLowerCase());
});

test("a colour the source sets wins over the palette's", async () => {
  const svg = (await svgOf('digraph { a [fillcolor="#ff0000"] }')).toLowerCase();
  expect(svg).toContain('fill="#ff0000"');
});

test("a graph that does not parse answers Graphviz's error, not a drawing", async () => {
  expect(await renderGraphviz("digraph {\n  a ->\n", LIGHT)).toEqual({ error: expect.stringMatching(/syntax error in line \d+/) });
});
