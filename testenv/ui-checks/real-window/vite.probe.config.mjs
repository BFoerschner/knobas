// testenv/ui-checks/real-window.sh's dev server: the app's own vite config, plus a probe: the page in the real Tauri window
// switches itself to ?fake-ipc (the mock corpus) and answers `POST /__probe`
// with the value of the JavaScript in the body, over vite's HMR socket.
// Nothing here synthesises an event: the events come from CGEvent.
import { fileURLToPath } from "node:url";

import { mergeConfig } from "../../../app/node_modules/vite/dist/node/index.js";
import base from "../../../app/vite.config.ts";

const appRoot = fileURLToPath(new URL("../../../app", import.meta.url));

const PROBE_ID = "/@rw-probe";
// RW_LIVE=1 (`real-window.sh start --live`): the page keeps the real bridge
// and talks to the demo profile's sources, so the window shows live data.
const live = process.env.RW_LIVE === "1";

const probeSource = `
if (${!live} && !new URLSearchParams(location.search).has("fake-ipc")) {
  location.replace("/?fake-ipc" + (location.hash || "#/inbox"));
}
const log = (window.__rwLog = []);
const push = (e) => { log.push(e); if (log.length > 400) log.shift(); };
const name = (el) => !el || el === document ? "none" : el === document.body ? "body" :
  el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (typeof el.className === "string" && el.className ? "." + el.className.trim().split(/\\s+/).slice(0, 3).join(".") : "");
window.__rwName = name;
for (const t of ["keydown"]) addEventListener(t, (e) => push({ t, key: e.key, meta: e.metaKey, target: name(e.target), at: performance.now() | 0, prevented: e.defaultPrevented }), true);
addEventListener("keydown", (e) => { const last = log.findLast((x) => x.t === "keydown" && x.key === e.key); if (last) last.preventedAfter = e.defaultPrevented; }, false);
let moves = 0, lastMove = null;
addEventListener("pointermove", (e) => { moves++; lastMove = { x: e.clientX, y: e.clientY, mx: e.movementX, my: e.movementY, trusted: e.isTrusted }; }, true);
window.__rwMoves = () => ({ moves, lastMove });
for (const t of ["pointerdown", "pointerup", "click", "wheel", "focusin"]) addEventListener(t, (e) => push({ t, target: name(e.target), trusted: e.isTrusted, mx: e.movementX, dy: e.deltaY, at: performance.now() | 0 }), true);
if (import.meta.hot) {
  import.meta.hot.on("rw:eval", async ({ id, code }) => {
    let value, error;
    try { value = await (0, eval)(code); } catch (e) { error = String(e && e.stack || e); }
    try { JSON.stringify(value); } catch { value = String(value); }
    import.meta.hot.send("rw:result", { id, value, error, href: location.href });
  });
  import.meta.hot.send("rw:hello", { href: location.href });
}
`;

const pending = new Map();
let seq = 0;
let lastHello = null;

// The real window's bridge is non-configurable, so ?fake-ipc can't replace
// it. Every served module reads a renamed global instead, and the fake installs
// there: the page then talks to the mock corpus only.
const rename = {
  name: "rw-rename-bridge",
  enforce: "pre",
  transform(code, id) {
    if (!/__TAURI_(EVENT_PLUGIN_)?INTERNALS__/.test(code)) return null;
    return { code: code.replace(/__TAURI_(EVENT_PLUGIN_)?INTERNALS__/g, (m) => "__RW" + m.slice(1)), map: null };
  },
};

const probe = {
  name: "rw-probe",
  resolveId(id) { return id === PROBE_ID ? "\0rw-probe.js" : null; },
  load(id) { return id === "\0rw-probe.js" ? probeSource : null; },
  transformIndexHtml(html, ctx) {
    if (!ctx.path || ctx.path.startsWith("/index.html") || ctx.path === "/") {
      return html.replace("<head>", `<head><script type="module" src="${PROBE_ID}"></script>`);
    }
    return html;
  },
  configureServer(server) {
    server.ws.on("rw:hello", (d) => { lastHello = { ...d, at: Date.now() }; });
    server.ws.on("rw:result", (d) => { const p = pending.get(d.id); if (p) { pending.delete(d.id); p(d); } });
    server.middlewares.use("/__probe", (req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        if (req.method === "GET") { res.end(JSON.stringify(lastHello)); return; }
        const id = ++seq;
        const timer = setTimeout(() => { pending.delete(id); res.statusCode = 504; res.end(JSON.stringify({ error: "no answer" })); }, 15000);
        pending.set(id, (d) => { clearTimeout(timer); res.setHeader("content-type", "application/json"); res.end(JSON.stringify(d)); });
        server.ws.send({ type: "custom", event: "rw:eval", data: { id, code: body } });
      });
    });
  },
};

export default (env) => mergeConfig(typeof base === "function" ? base(env) : base, { root: appRoot, plugins: live ? [probe] : [rename, probe], optimizeDeps: { exclude: ["@tauri-apps/api", "@tauri-apps/plugin-notification", "@tauri-apps/plugin-opener"] }, cacheDir: `${process.env.RW_STATE ?? "/tmp"}/vite-cache${live ? "-live" : ""}` });
