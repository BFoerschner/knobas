import { expect, test } from "vitest";

import { svgFrame } from "./frame";

/** The SVG inside a frame's `<img>`, decoded. */
function imageIn(srcdoc: string): string {
  const doc = new DOMParser().parseFromString(srcdoc, "text/html");
  const src = doc.querySelector("img")?.getAttribute("src") ?? "";
  expect(src).toMatch(/^data:image\/svg\+xml;base64,/);
  return new TextDecoder().decode(Uint8Array.from(atob(src.split(",")[1]!), (c) => c.charCodeAt(0)));
}

const HOSTILE =
  '<svg viewBox="-8 -8 216.4 96" width="100%" style="max-width: 216px"><script>parent.x = 1</script>' +
  '<a href="javascript:alert(1)"><text onclick="alert(2)">hi</text></a><g onmouseover="alert(3)"><rect width="5" height="5"/></g></svg>';

test("an SVG is drawn as an image in the frame, sized to its viewBox", () => {
  const frame = svgFrame(HOSTILE)!;
  expect([frame.width, frame.height]).toEqual([217, 96]);
  const doc = new DOMParser().parseFromString(frame.srcdoc, "text/html");
  const img = doc.querySelector("img")!;
  expect([img.getAttribute("width"), img.getAttribute("height")]).toEqual(["217", "96"]);
  expect(doc.body.getAttribute("marginwidth")).toBe("0");
  const svg = imageIn(frame.srcdoc);
  expect(svg).toContain('width="217"');
  expect(svg).not.toContain("max-width");
  expect(svg).toContain(">hi<");
});

test("the SVG is sanitized first: no script, no handler, no javascript: link", () => {
  const svg = imageIn(svgFrame(HOSTILE)!.srcdoc);
  expect(svg).not.toMatch(/<script/i);
  expect(svg).not.toMatch(/\bon[a-z]+=/i);
  expect(svg).not.toMatch(/javascript:/i);
});

test("the frame's document takes the drawing's colour scheme", () => {
  expect(svgFrame(HOSTILE, true)!.srcdoc).toContain('<meta name="color-scheme" content="dark">');
  expect(svgFrame(HOSTILE, false)!.srcdoc).toContain('<meta name="color-scheme" content="light">');
});

test("markup with no SVG in it gives no frame", () => {
  expect(svgFrame("<p>not a drawing</p>")).toBeNull();
});
