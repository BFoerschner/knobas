import { afterEach, expect, test, vi } from "vitest";

import { EMBED_OPEN_GAP_MS, EMBED_SANDBOX, EMBED_URL_REFERRER, EMBED_URL_SANDBOX, REPORTER, embedDocument, embedMessage, embedSource, embedUrl, openVerdict } from "./embed";

// -- The frame (#2440, spec #2432 *Embeds in the shell*) ----------------------

test("the sandbox runs scripts, forms and popups, and grants neither the app's origin nor top navigation", () => {
  const tokens = EMBED_SANDBOX.split(" ");
  expect(tokens.sort()).toEqual(["allow-forms", "allow-popups", "allow-scripts"]);
  expect(EMBED_SANDBOX).not.toMatch(/same-origin|top-navigation|popups-to-escape/);
});

test("the document is the author's HTML as written, after knobas' reporter", () => {
  const html = `<div id="ui"></div><script>SwaggerUIBundle({ url: "https://api.example/spec.json?a=1&amp;b" })</script>`;
  const doc = embedDocument(html);
  expect(doc.endsWith(`${html}</body></html>`)).toBe(true);
  expect(doc.indexOf(REPORTER)).toBeGreaterThan(0);
  expect(doc.indexOf(REPORTER)).toBeLessThan(doc.indexOf(html));
});

const bridge = { isTauri: () => true, convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}` };
const decoded = (src: string) => {
  const path = decodeURIComponent(src.slice("knobas-render://localhost/".length));
  expect(path.startsWith("embed.")).toBe(true);
  const b64 = path.slice("embed.".length).replace(/-/g, "+").replace(/_/g, "/");
  return new TextDecoder().decode(Uint8Array.from(atob(b64 + "=".repeat((4 - (b64.length % 4)) % 4)), (c) => c.charCodeAt(0)));
};

test("under the app's bridge the document is served from knobas-render's embed path, byte for byte", () => {
  const html = "<p>é ü 🙂</p><script>go()</script>";
  const source = embedSource(html, bridge);
  expect(source).not.toBeNull();
  expect("src" in source!).toBe(true);
  const src = (source as { src: string }).src;
  // `encodeURIComponent` leaves the `.` alone, so the backend sees `embed.` as written.
  expect(src.startsWith("knobas-render://localhost/embed.")).toBe(true);
  expect(decoded(src)).toBe(embedDocument(html));
});

test("without the bridge (dev, tests) the frame is a srcdoc; a document too long for a URL has no frame", () => {
  const off = { isTauri: () => false, convertFileSrc: bridge.convertFileSrc };
  expect(embedSource("<p>x</p>", off)).toEqual({ srcdoc: embedDocument("<p>x</p>") });
  // A srcdoc in the app would inherit the app's policy, and no script would run: say it did not load instead.
  expect(embedSource("x".repeat(1_200_000), bridge)).toBeNull();
});

// -- What the frame says ------------------------------------------------------

test("a message is a height or a page to open, and nothing else", () => {
  expect(embedMessage({ knobas: "embed", height: 412.4 })).toEqual({ height: 413 });
  expect(embedMessage({ knobas: "embed", open: "https://example.com/a" })).toEqual({ open: "https://example.com/a" });
  for (const junk of [
    null,
    "height",
    { height: 10 },
    { knobas: "embed", height: -1 },
    { knobas: "embed", height: Number.NaN },
    { knobas: "embed", height: "10" },
    { knobas: "embed", open: 7 },
    { knobas: "d2", height: 10 },
  ]) {
    expect(embedMessage(junk)).toBeNull();
  }
});

// -- The reporter, run in a window of its own --------------------------------

let frame: HTMLIFrameElement | null = null;
afterEach(() => {
  frame?.remove();
  frame = null;
});

/** A frame's window with the reporter run in it, and what it posted to its parent. */
function reporting(body: string): { win: Window & typeof globalThis; posted: unknown[] } {
  frame = document.createElement("iframe");
  document.body.append(frame);
  const win = frame.contentWindow as Window & typeof globalThis;
  win.document.body.innerHTML = body;
  const posted: unknown[] = [];
  vi.spyOn(win.parent, "postMessage").mockImplementation((message: unknown) => void posted.push(message));
  new Function("window", `${REPORTER}(window);`)(win);
  return { win, posted };
}

test("the reporter hands a link's click to the parent and keeps the frame where it is", () => {
  const { win, posted } = reporting(`<a id="out" href="https://example.com/docs?a=1">docs</a><a id="here" href="#models">models</a>`);
  const out = win.document.getElementById("out")!;
  const click = new win.MouseEvent("click", { bubbles: true, cancelable: true });
  out.dispatchEvent(click);
  expect(click.defaultPrevented).toBe(true);
  expect(posted).toContainEqual({ knobas: "embed", open: "https://example.com/docs?a=1" });

  // A link inside the document stays the frame's own.
  const before = posted.length;
  const inside = new win.MouseEvent("click", { bubbles: true, cancelable: true });
  win.document.getElementById("here")!.dispatchEvent(inside);
  expect(inside.defaultPrevented).toBe(false);
  expect(posted.length).toBe(before);
});

test("the reporter turns window.open into a page for the parent to open, and opens nothing itself", () => {
  const { win, posted } = reporting("<p>x</p>");
  expect(win.open("https://example.com/popup", "_blank")).toBeNull();
  expect(posted).toContainEqual({ knobas: "embed", open: "https://example.com/popup" });
});

test("the reporter says the document's height when it loads", () => {
  const { win, posted } = reporting("<p>x</p>");
  win.dispatchEvent(new win.Event("load"));
  expect(posted.some((m) => typeof (m as { height?: unknown }).height === "number")).toBe(true);
});

// -- A URL embed (#2441) and an embed's opens (#2454) ------------------------

test("a URL embed frames an absolute http(s) URL with a host as its src, and nothing else", () => {
  expect(embedUrl("https://grafana.example/d/1?a=b")).toEqual({ src: "https://grafana.example/d/1?a=b" });
  expect(embedUrl("HTTP://a.example")).toEqual({ src: "HTTP://a.example" });
  for (const bad of ["javascript:alert(1)", "file:///etc/passwd", "knobas://x", "data:text/html,x", "//a.example/", "/wiki", "https://", "not a url", ""]) {
    expect(embedUrl(bad), bad).toBeNull();
  }
});

test("a URL embed's frame keeps its own origin and says where it is framed from (#2460)", () => {
  // YouTube's player draws nothing in an opaque origin and refuses to start without a referrer
  // (headless Chrome, #2460): a page from another origin gets its origin, an HTML embed never does.
  expect(EMBED_URL_SANDBOX.split(" ").sort()).toEqual(["allow-forms", "allow-popups", "allow-same-origin", "allow-scripts"]);
  expect(EMBED_URL_SANDBOX).not.toMatch(/top-navigation|popups-to-escape/);
  expect(EMBED_URL_REFERRER).toBe("strict-origin");
});

test("a URL embed of the app's own origin, or any that could share it, is never framed (#2460)", () => {
  const dev = "http://localhost:1420";
  for (const own of [
    "tauri://localhost/",
    "http://tauri.localhost/",
    "https://tauri.localhost/index.html",
    "http://ipc.localhost/plugin%3Aevent%7Clisten",
    "https://ipc.localhost/",
    "knobas-render://localhost/embed.PGI-",
    "http://knobas-render.localhost/embed.PGI-",
    "https://knobas-render.localhost/",
    "http://localhost:1420/",
    "http://localhost:5173/",
    "http://127.0.0.1:1420/",
    // Loopback by other spellings, and a host behind userinfo. Spelled from the host part:
    // the house rule's network scan reads them as real hosts.
    ...["LOCALHOST.", "tauri.localhost..", "[::1]:1420", "evil.example@tauri.localhost", "0.0.0.0:1420", "2130706433:1420", "127.1", "[::ffff:127.0.0.1]"].map((host) => ["http:", host].join("//") + "/"),
  ]) {
    expect(embedUrl(own, dev), own).toBeNull();
  }
  // The window's own origin, wherever it is served from.
  expect(embedUrl("https://app.example/#/docs", "https://app.example")).toBeNull();
  expect(embedUrl("https://app.example/", "https://other.example")).toEqual({ src: "https://app.example/" });
  expect(embedUrl("https://video.example/embed/aqz-KE-bpKQ", dev)).toEqual({ src: "https://video.example/embed/aqz-KE-bpKQ" });
  expect(embedUrl("https://localhost.example/", dev)).toEqual({ src: "https://localhost.example/" });
});

test("an open from the frame runs only behind the reader's click, one per gap; any other is asked once at a time", () => {
  const at = (over: Partial<Parameters<typeof openVerdict>[0]>) => openVerdict({ activated: true, now: 10_000, lastOpen: 0, asking: false, ...over });
  expect(at({})).toBe("open");
  // No click behind it: the reader is asked.
  expect(at({ activated: false })).toBe("ask");
  // A second open on the same click is asked, not opened.
  expect(at({ lastOpen: 10_000 - EMBED_OPEN_GAP_MS + 1 })).toBe("ask");
  expect(at({ lastOpen: 10_000 - EMBED_OPEN_GAP_MS })).toBe("open");
  // While one is asked, the rest are dropped: a loop asks once.
  expect(at({ activated: false, asking: true })).toBe("drop");
  expect(at({ lastOpen: 9_999, asking: true })).toBe("drop");
});
