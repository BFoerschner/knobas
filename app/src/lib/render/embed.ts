/**
 * A docs embed's frame (#2440, spec #2432 *Embeds in the shell*): the HTML a
 * page's author wrote, run as written, where it cannot reach knobas.
 *
 * Three walls, each holding if another fails:
 *
 * - **The sandbox**, {@link EMBED_SANDBOX}: scripts, forms and popups, and
 *   no `allow-same-origin`, so the document runs in an opaque origin with no
 *   storage, no cookies and no handle on the app's window; no top navigation,
 *   so it cannot replace the app.
 * - **Its own origin and policy**: under the app's bridge the document is
 *   served from `knobas-render://localhost/embed.<document>`
 *   (`knobas_app::renderers::EMBED_CSP`): scripts, styles, images, fonts,
 *   media and frames from any `http(s)` host, connections to any, and
 *   nothing else, so not the app's `ipc:` scheme. A `srcdoc` frame would
 *   inherit the app's policy instead, and no script in it would run.
 * - **No Tauri in it**: Tauri's globals are injected into the app's own
 *   document only, never a subframe's.
 *
 * knobas puts {@link REPORTER} before the author's HTML. It tells the parent
 * the document's height, and hands every link the reader follows and every
 * `window.open` to the parent, which opens it through the external-open path
 * and its scheme check (`shell/open-external.ts`), behind the reader's own
 * click only ({@link openVerdict}). The parent takes a message only from its
 * own frame ({@link embedMessage} reads it).
 *
 * A URL embed (#2441) is a sandboxed frame with the page as its `src`
 * ({@link embedUrl}), framed by the app directly (its policy's `frame-src`
 * names `http:` and `https:`). The page is another origin's: knobas puts
 * nothing in it, so it reports no height and hands no link up. It keeps that
 * origin and gets a referrer ({@link EMBED_URL_SANDBOX}, #2460); the app's
 * own origin is never framed that way.
 */
import { base64Of } from "./bytes";
import { FRAME_SCHEME, type FrameBridge } from "./frame";

/** The embed frame's sandbox: an opaque origin that runs scripts, forms and popups. */
export const EMBED_SANDBOX = "allow-scripts allow-forms allow-popups";

/** The path prefix the backend serves an embed's document under (`renderers::EMBED_PREFIX`). */
const EMBED_PREFIX = "embed.";

/** Past this a document does not fit a URL; the frame says it did not load (a `srcdoc` would run no script). */
const URL_DOCUMENT_CAP = 1_500_000;

/**
 * The size reporter and link handler, plain ES5 run with `window` as its one
 * argument (the frame's own; a test passes another). It posts
 * `{knobas: "embed", height}` when the document loads and whenever it
 * resizes, and `{knobas: "embed", open}` for a link the reader follows or a
 * `window.open`. A link to a place in the document stays the frame's.
 */
export const REPORTER = `(function (w) {
  var d = w.document;
  var post = function (m) { m.knobas = "embed"; w.parent.postMessage(m, "*"); };
  var last = -1;
  var size = function () {
    var root = d.documentElement, body = d.body;
    var h = Math.ceil(Math.max(root ? root.getBoundingClientRect().height : 0, body ? body.scrollHeight : 0));
    if (h !== last) { last = h; post({ height: h }); }
  };
  var absolute = function (u) { try { return new w.URL(String(u), w.location.href).href; } catch (e) { return String(u); } };
  w.addEventListener("load", size);
  if (w.ResizeObserver) new w.ResizeObserver(size).observe(d.documentElement);
  d.addEventListener("click", function (e) {
    var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a) return;
    var href = a.getAttribute("href") || "";
    if (href.charAt(0) === "#") return;
    e.preventDefault();
    post({ open: a.href });
  }, true);
  w.open = function (u) { if (u) post({ open: absolute(u) }); return null; };
})`;

/** The frame's document: knobas' reporter, then the author's HTML exactly as written. */
export function embedDocument(html: string): string {
  return `<!doctype html><html><head><meta charset="utf-8"><script>${REPORTER}(window);</script></head><body>${html}</body></html>`;
}

/**
 * Where the frame draws `html` from: the backend's `knobas-render` scheme
 * under the app's bridge, a `srcdoc` without it (dev and tests, where there
 * is no policy to inherit), and `null` for a document too long for a URL.
 */
export function embedSource(html: string, bridge: FrameBridge): { src: string } | { srcdoc: string } | null {
  const document = embedDocument(html);
  if (!bridge.isTauri()) return { srcdoc: document };
  const path = base64Of(new TextEncoder().encode(document)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return path.length > URL_DOCUMENT_CAP ? null : { src: bridge.convertFileSrc(`${EMBED_PREFIX}${path}`, FRAME_SCHEME) };
}

/** What a frame said: its height in whole CSS pixels, or a page to open. */
export type EmbedMessage = { height: number } | { open: string };

/** A message from an embed's frame, or `null` for anything that is not one. */
export function embedMessage(data: unknown): EmbedMessage | null {
  if (typeof data !== "object" || data === null) return null;
  const m = data as { knobas?: unknown; height?: unknown; open?: unknown };
  if (m.knobas !== "embed") return null;
  if (typeof m.height === "number" && Number.isFinite(m.height) && m.height >= 0) return { height: Math.ceil(m.height) };
  if (typeof m.open === "string") return { open: m.open };
  return null;
}

/**
 * A URL embed's sandbox (#2460): {@link EMBED_SANDBOX} plus `allow-same-origin`,
 * so the framed page runs as its own origin, with its own cookies and storage.
 * YouTube's player draws nothing in an opaque origin (headless Chrome, #2460).
 * The page is still another origin's, so the browser keeps it off the app's
 * window, document and storage; {@link embedUrl} refuses every address that
 * could be the app's origin, where `allow-same-origin` with `allow-scripts`
 * would let the frame lift its own sandbox. An HTML embed never takes this:
 * knobas serves the author's HTML itself, so its origin stays opaque.
 */
export const EMBED_URL_SANDBOX = `${EMBED_SANDBOX} allow-same-origin`;

/**
 * A URL embed's referrer (#2460): the app's origin and nothing after it.
 * YouTube's player refuses to start without one ("Error 153"). An HTML
 * embed's frame sends none.
 */
export const EMBED_URL_REFERRER = "strict-origin";

/**
 * Hosts that are, or could be, the app's own origin under some webview:
 * `tauri.localhost`, `ipc.localhost` and `knobas-render.localhost` (WebView2
 * and Android), the dev server's `localhost`, and loopback by any spelling.
 * `tauri:`, `ipc:` and `knobas-render:` themselves fall to the scheme check.
 */
const OWN_HOST = /^(?:localhost|.+\.localhost|127(?:\.\d+){3}|0\.0\.0\.0|\[::1?\]|\[::ffff:7f[0-9a-f]{2}:[0-9a-f]{1,4}\])$/i;

/**
 * Where a URL embed's frame draws from (#2441): the URL itself, in
 * {@link EMBED_URL_SANDBOX}, when it is an absolute `http(s)` URL with a host
 * that cannot be the app's origin; `null` for anything else. The adapter
 * already refuses other schemes (`EmbedContent::url`); this holds for one that
 * does not. `own` is the app window's origin (a test passes another).
 */
export function embedUrl(url: string, own: string = window.location.origin): { src: string } | null {
  const match = /^https?:\/\/[^/?#\s]+/i.exec(url);
  if (match === null || /\s/.test(url)) return null;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.origin === own || OWN_HOST.test(parsed.hostname.replace(/\.$/, ""))) return null;
  return { src: url };
}

/**
 * After an open an embed asked for, the next one from any embed in this window
 * within this gap is asked of the reader (#2454). It spans transient
 * activation's lifetime (5 s in WebKit and Chromium), so one click, anywhere
 * in the window, opens at most one tab: a 1 s gap let a frame's script open
 * one tab a second for as long as a single click stayed active.
 */
export const EMBED_OPEN_GAP_MS = 5_000;

/** When an embed's open last ran without asking, shared by every embed in the window, so two embeds don't each take one click (#2454). */
export const embedOpens = {
  last: -Infinity,
  reset(): void {
    this.last = -Infinity;
  },
};

/**
 * What to do with a page the frame asks to open (#2454). The frame's script
 * can post the message itself, so the frame's word that a link was clicked
 * counts for nothing; the parent's own transient activation does, which a
 * browser that passes a child frame's click up to its ancestors sets.
 *
 * - `open`: the reader clicked just now, and nothing opened in the last
 *   {@link EMBED_OPEN_GAP_MS} from any embed: one tab per click.
 * - `ask`: no click behind it (or a second open on one click): the reader is
 *   asked, and opens it with a click of their own.
 * - `drop`: the reader is being asked already; a loop asks once.
 */
export function openVerdict(at: { activated: boolean; now: number; lastOpen: number; asking: boolean }): "open" | "ask" | "drop" {
  if (at.activated && at.now - at.lastOpen >= EMBED_OPEN_GAP_MS) return "open";
  return at.asking ? "drop" : "ask";
}
