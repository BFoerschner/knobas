// A static server for a built knobas front end with a probe, for measuring in any engine.
//
//   node serve-probe.mjs <dist folder> <port>
//
// - Serves the folder on 127.0.0.1:<port> and localhost:<port>.
// - index.html gets <script src="/__probe.js"> first in <head>, before the app's modules, so the probe's
//   recorders are in place before any app code runs.
// - POST /__probe with JavaScript in the body answers {value|error} with what the page evaluated it to
//   (the page long-polls /__probe/next and posts to /__probe/result). Awaited if it is a promise.
// - GET /__probe answers the last hello (the page's href and user agent), or null.
//
// The probe only reads. It makes no event unless the posted JavaScript does.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const [dist, portArg] = process.argv.slice(2);
if (!dist || !portArg) {
  console.error("usage: node serve-probe.mjs <dist folder> <port>");
  process.exit(2);
}
const port = Number(portArg);
const here = fileURLToPath(new URL(".", import.meta.url));

const types = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png",
  ".woff2": "font/woff2", ".woff": "font/woff", ".ttf": "font/ttf", ".wasm": "application/wasm", ".map": "application/json",
  ".ico": "image/x-icon", ".webp": "image/webp", ".jpg": "image/jpeg", ".gif": "image/gif", ".txt": "text/plain", ".bcmap": "application/octet-stream",
};

let seq = 0;
let hello = null;
const queue = []; // commands waiting for the page
const waiters = []; // the page's long polls
const pending = new Map(); // id -> resolve

function deliver() {
  while (queue.length && waiters.length) {
    const res = waiters.shift();
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify(queue.shift()));
  }
}

function body(req) {
  return new Promise((resolve) => {
    let text = "";
    req.on("data", (c) => (text += c));
    req.on("end", () => resolve(text));
  });
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${port}`);
  res.setHeader("cache-control", "no-store");
  if (url.pathname === "/__probe.js") {
    res.setHeader("content-type", types[".js"]);
    res.end(await readFile(join(here, "probe-page.js")));
    return;
  }
  if (url.pathname === "/__probe/next") {
    waiters.push(res);
    req.on("close", () => { const i = waiters.indexOf(res); if (i >= 0) waiters.splice(i, 1); });
    deliver();
    return;
  }
  if (url.pathname === "/__probe/hello") {
    hello = { ...JSON.parse(await body(req)), at: Date.now() };
    res.end("{}");
    return;
  }
  if (url.pathname === "/__probe/result") {
    const d = JSON.parse(await body(req));
    const p = pending.get(d.id);
    if (p) { pending.delete(d.id); p(d); }
    res.end("{}");
    return;
  }
  if (url.pathname === "/__probe") {
    if (req.method === "GET") { res.setHeader("content-type", "application/json"); res.end(JSON.stringify(hello)); return; }
    const code = await body(req);
    const id = ++seq;
    const wait = Number(url.searchParams.get("wait") ?? 120000);
    const timer = setTimeout(() => { pending.delete(id); res.statusCode = 504; res.end(JSON.stringify({ error: "no answer" })); }, wait);
    pending.set(id, (d) => { clearTimeout(timer); res.setHeader("content-type", "application/json"); res.end(JSON.stringify(d)); });
    queue.push({ id, code });
    deliver();
    return;
  }
  let path = normalize(decodeURIComponent(url.pathname));
  if (path.endsWith("/")) path += "index.html";
  const file = join(dist, path);
  if (!file.startsWith(dist)) { res.statusCode = 403; res.end(); return; }
  try {
    const info = await stat(file);
    if (info.isDirectory()) throw new Error("dir");
    let data = await readFile(file);
    if (path === "/index.html") data = Buffer.from(String(data).replace("<head>", `<head><script src="/__probe.js"></script>`));
    res.setHeader("content-type", types[extname(file)] ?? "application/octet-stream");
    res.end(data);
  } catch {
    res.statusCode = 404;
    res.end("not found");
  }
});
server.listen(port, () => console.log(`serving ${dist} on http://localhost:${port}`));
