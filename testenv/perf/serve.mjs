// Serves a built app folder on 127.0.0.1 only, for as long as it runs: what run.mjs opens where there is no Python
// (Windows). Loopback only, so Windows asks no firewall question.
//
//   node testenv/perf/serve.mjs <folder> <port>
import { createServer } from "node:http";
import { existsSync, readFileSync, statSync } from "node:fs";
import { extname, join, normalize } from "node:path";

const [dir, port] = process.argv.slice(2);
const root = normalize(dir);
const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".woff2": "font/woff2", ".wasm": "application/wasm", ".map": "application/json", ".bcmap": "application/octet-stream", ".ttf": "font/ttf", ".pfb": "application/octet-stream" };
createServer((req, res) => {
  let path = normalize(join(root, decodeURIComponent(new URL(req.url, "http://x").pathname)));
  if (!path.startsWith(root)) { res.writeHead(403).end(); return; }
  if (existsSync(path) && statSync(path).isDirectory()) path = join(path, "index.html");
  if (!existsSync(path)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": TYPES[extname(path)] ?? "application/octet-stream", "cache-control": "no-store" });
  res.end(readFileSync(path));
}).listen(Number(port), "127.0.0.1", () => console.log(`serving ${root} on http://127.0.0.1:${port}/`));
