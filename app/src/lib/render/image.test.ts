/** An image's bytes as a blob an `<img>` may load (#2256): its type checked, an SVG sanitized. */
import { expect, test } from "vitest";

import { dataUrlImage, typedImageBlob } from "./image";

test("a data URL's bytes and type are read back; anything else is not an image", () => {
  expect(dataUrlImage("data:image/PNG;base64,iVA=")).toEqual({ type: "image/png", bytes: Uint8Array.from([0x89, 0x50]) });
  expect(dataUrlImage("https://example.com/a.png")).toBeNull();
  expect(dataUrlImage("data:image/png,raw")).toBeNull();
  expect(dataUrlImage("data:image/png;base64,***")).toBeNull();
});

test("a type that is not a listed image type is loaded as bytes, never as HTML or SVG", async () => {
  expect(typedImageBlob(new TextEncoder().encode("<b>x</b>"), "text/html").type).toBe("application/octet-stream");
  expect(typedImageBlob(new Uint8Array([1]), "image/png").type).toBe("image/png");
});

test("an SVG blob keeps its drawing and loses its script", async () => {
  const blob = typedImageBlob(new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"><script>x()</script><circle r="2"/></svg>'), "image/svg+xml");
  const svg = await blob.text();
  expect(svg).toContain("<circle");
  expect(svg).not.toContain("script");
});
