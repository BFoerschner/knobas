import { expect, test } from "vitest";

import { frameDocument, frameSource, PAGE_SCROLLS, svgFrame } from "./frame";

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

test("the sized image floats (align=left), so it makes no line box taller than the frame", () => {
  // An inline <img> sits on a text baseline: the line box is a few px taller than the image, the frame sized to the
  // image overflows, and with classic macOS scrollbars WKWebView draws both bars and clips the drawing (#2380).
  // A floated image (`align`, a presentational attribute no CSP refuses) makes no line box.
  const img = new DOMParser().parseFromString(svgFrame(HOSTILE)!.srcdoc, "text/html").querySelector("img")!;
  expect(img.getAttribute("align")).toBe("left");
});

test("a frame's document can draw the drawing at another size, and a drawing with no box of its own gets one so it scales (#2510)", () => {
  const frame = svgFrame('<svg width="300" height="120"><rect width="300" height="120"/></svg>')!;
  expect([frame.width, frame.height]).toEqual([300, 120]);
  const at = (doc: string) => {
    const img = new DOMParser().parseFromString(doc, "text/html").querySelector("img")!;
    return [img.getAttribute("width"), img.getAttribute("height"), img.getAttribute("align")];
  };
  expect(at(frame.srcdoc)).toEqual(["300", "120", "left"]);
  expect(at(frame.at(100, 40))).toEqual(["100", "40", "left"]);
  // Without a `viewBox` an image of the drawing at another size would crop it.
  expect(imageIn(frame.at(100, 40))).toContain('viewBox="0 0 300 120"');
  // One that has a box keeps it.
  expect(imageIn(svgFrame('<svg viewBox="10 20 200 80"><rect/></svg>')!.srcdoc)).toContain('viewBox="10 20 200 80"');
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

// -- A command renderer's HTML from its own origin (#2304) --------------------

const bridge = { isTauri: () => true, convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}` };
const decoded = (src: string) => {
  const b64 = decodeURIComponent(src.slice("knobas-render://localhost/".length)).replace(/-/g, "+").replace(/_/g, "/");
  return new TextDecoder().decode(Uint8Array.from(atob(b64 + "=".repeat((4 - (b64.length % 4)) % 4)), (c) => c.charCodeAt(0)));
};

test("under the app's bridge, HTML is served from knobas-render with its styles kept and its script gone", () => {
  const html = `<style>h1 { color: red }</style><h1 style="font-size: 2em">Title é</h1><script>parent.x = 1</script>`;
  const frame = frameSource("html", html, bridge);

  expect("srcdoc" in frame).toBe(false);
  const src = (frame as { src: string }).src;
  expect(src.startsWith("knobas-render://localhost/")).toBe(true);
  const doc = decoded(src);
  expect(doc).toContain("<style>h1 { color: red }</style>");
  expect(doc).toContain('style="font-size: 2em"');
  expect(doc).toContain("Title é");
  expect(doc).not.toMatch(/<script/i);
});

test("without the bridge (dev, tests) and for SVG, the frame is a srcdoc as before", () => {
  const html = "<p>x</p>";
  expect(frameSource("html", html, { isTauri: () => false, convertFileSrc: bridge.convertFileSrc })).toEqual({ srcdoc: frameDocument("html", html) });
  const svg = '<svg viewBox="0 0 2 2"><rect width="2" height="2"/></svg>';
  expect(frameSource("svg", svg, bridge)).toEqual({ srcdoc: frameDocument("svg", svg) });
});

/**
 * A page that hides its own overflow still scrolls (2026-11-17, Björn: "I can't scroll it even though it's longer
 * than the page"): the frame's head makes the root scroll and the body clip nothing, in a layer named before anything
 * of the page's, and the page's own styles and words stay. An SVG is an image and gets none. (That the layer wins in a
 * browser is the press trips' `frame-scroll`: jsdom has no cascade layers.)
 */
test("an HTML page scrolls in its frame even where it hides its own overflow", () => {
  const page = `<style>html, body { overflow: hidden !important; height: 100vh; }</style><h1>Long</h1>`;
  const doc = new DOMParser().parseFromString(frameDocument("html", page), "text/html");
  const [ours, theirs] = [...doc.querySelectorAll("style")].map((s) => s.textContent ?? "");
  expect(doc.head.contains(doc.querySelector("style"))).toBe(true);
  expect(ours).toBe("@layer knobas-frame{:root{overflow:auto!important}:root>body{overflow:visible!important}}");
  expect(ours).toBe(PAGE_SCROLLS.replace(/<\/?style>/g, ""));
  expect(theirs).toContain("overflow: hidden !important");
  expect(doc.querySelector("h1")?.textContent).toBe("Long");
  // The page can't take it away: a <style> or <html> of its own lands in the body, after the head's.
  const hostile = frameDocument("html", `<html style="overflow:hidden"><head><style>@layer a{:root{overflow:hidden!important}}</style></head><body>x</body></html>`);
  expect(hostile.indexOf(PAGE_SCROLLS)).toBeLessThan(hostile.indexOf("@layer a"));
  expect(hostile.indexOf(PAGE_SCROLLS)).toBeLessThan(hostile.indexOf("<body>"));
  expect(hostile).not.toContain('style="overflow:hidden"');
  expect(frameDocument("svg", "<svg viewBox='0 0 4 4'></svg>")).not.toContain(PAGE_SCROLLS);
});
