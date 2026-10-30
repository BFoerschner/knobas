/**
 * Mermaid itself under jsdom (#2258): the real engine, with the one thing
 * jsdom lacks (text measurement) stubbed to a fixed width per character.
 * The drawings' geometry is meaningless here; what is witnessed is what the
 * engine is configured to emit (security level, labels, colours, errors).
 */
import { beforeAll, expect, test } from "vitest";

import { renderMermaid } from "./mermaid";
import type { Palette } from "./port";

const LIGHT: Palette = {
  dark: false, paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", ink: "#17212B", ink2: "#4B5663",
  rule: "#CBD2D9", mutedBg: "#E6E9EC", mark: "#F6E08F",
};
const DARK: Palette = {
  dark: true, paper: "#12171C", sheet: "#1A2027", sheet2: "#212932", ink: "#E4E9EE", ink2: "#A0ABB7",
  rule: "#323C47", mutedBg: "#262E37", mark: "#5C4E17",
};

beforeAll(() => {
  const measure = function (this: SVGElement) {
    return { x: 0, y: 0, width: 8 * (this.textContent?.length ?? 1), height: 16 } as DOMRect;
  };
  Object.assign(SVGElement.prototype, { getBBox: measure, getComputedTextLength: () => 40 });
});

const svgOf = async (code: string, palette = LIGHT) => {
  const answer = await renderMermaid(code, palette);
  if (!("svg" in answer)) throw new Error(`no drawing: ${answer.error}`);
  return answer.svg;
};

test("a flowchart and a sequence diagram draw as SVG with SVG text labels, no HTML ones", async () => {
  const flow = await svgOf("flowchart LR\n  pay[Payment] --> outbox[Outbox]");
  expect(flow).toMatch(/^<svg/);
  expect(flow).toContain("Payment");
  expect(flow).not.toContain("foreignObject");
  const sequence = await svgOf("sequenceDiagram\n  Shop->>API: POST /pay\n  API-->>Shop: 202");
  expect(sequence).toContain("POST /pay");
});

test("the drawing takes the palette's colours, and the dark palette draws dark", async () => {
  const code = "flowchart LR\n  a[One] --> b[Two]";
  const light = await svgOf(code, LIGHT);
  const dark = await svgOf(code, DARK);
  expect(light.toLowerCase()).toContain(LIGHT.sheet2.toLowerCase());
  expect(dark.toLowerCase()).toContain(DARK.sheet2.toLowerCase());
  expect(dark.toLowerCase()).not.toContain(LIGHT.sheet2.toLowerCase());
});

test("a diagram that does not parse answers the parser's error, not a drawing", async () => {
  const answer = await renderMermaid("flowchart TD\n  api -->", LIGHT);
  expect(answer).toEqual({ error: expect.stringMatching(/^Parse error on line \d+:[\s\S]*Expecting/) });
  expect(document.querySelector("[id^=tf-mermaid], [id^=dtf-mermaid]")).toBeNull();
});

test("a click directive binds nothing and its javascript: link is not drawn", async () => {
  const svg = await svgOf('flowchart TD\n  A[Start] --> B[End]\n  click A href "javascript:alert(1)"\n  click B call evil()');
  expect(svg).not.toMatch(/javascript:/i);
  expect(svg).not.toMatch(/\bon[a-z]+=/i);
});
