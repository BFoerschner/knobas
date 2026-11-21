#!/usr/bin/env node
// A stand-in for Chrome that speaks just enough of the DevTools protocol for the probes' watchdog tests
// (`watchdog.test.mjs`): run as `KNOBAS_CHROME`, it writes `DevToolsActivePort` into `--user-data-dir` as Chrome does,
// serves `/json/version`, `/json/list`, `/json/new` and `/json/close`, and answers every command on a page's socket
// with `{}`, except as these say:
//   FAKE_CDP_HANG        methods (and events) it never answers (or fires), comma-separated; a `Runtime.evaluate` whose
//                        expression holds `FAKE_CDP_HANG_MATCH` never answers either
//   FAKE_CDP_HANG_SKIP   how many of those it answers before it stops answering (0 by default)
//   FAKE_CDP_HANG_TIMES  how many of those it leaves unanswered, then answers again (all of them by default)
//   FAKE_CDP_NO_UPGRADE  takes a page's socket and never opens it
//   FAKE_CDP_NO_PORT     never writes `DevToolsActivePort`, as a Chrome that hangs on start
//   FAKE_CDP_DIE         a method on which the whole browser exits, as a crashed Chrome does (once: the state folder
//                        remembers it, so the Chrome started again lives)
//   FAKE_CDP_VALUE       the JSON every `Runtime.evaluate` answers (`true` by default)
//   FAKE_CDP_STATE       a folder it logs to: `calls.log` (one line a command: pid and method), `launches.log`
// `Page.navigate` and `Page.reload` fire `Page.loadEventFired` 5 ms later; `Tracing.end` fires `Tracing.tracingComplete`.
import { createHash } from "node:crypto";
import { appendFileSync, existsSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";

const env = process.env;
const hang = new Set((env.FAKE_CDP_HANG ?? "").split(",").filter(Boolean));
const hangMatch = env.FAKE_CDP_HANG_MATCH || null;
let skip = Number(env.FAKE_CDP_HANG_SKIP ?? 0);
let times = env.FAKE_CDP_HANG_TIMES ? Number(env.FAKE_CDP_HANG_TIMES) : Infinity;
const value = JSON.parse(env.FAKE_CDP_VALUE ?? "true");
const state = env.FAKE_CDP_STATE || null;
const log = (file, line) => { if (state) appendFileSync(join(state, file), `${process.pid} ${line}\n`); };
const profile = process.argv.find((a) => a.startsWith("--user-data-dir="))?.slice("--user-data-dir=".length);

/** Whether this call is one never answered (after the first `FAKE_CDP_HANG_SKIP` of them). */
const hung = (method, params) => {
  if (!(hang.has(method) || (hangMatch && method === "Runtime.evaluate" && String(params?.expression ?? "").includes(hangMatch)))) return false;
  if (skip > 0) { skip--; return false; }
  if (times <= 0) return false;
  times--;
  log("hung.log", method);
  return true;
};

let next = 1;
const targets = new Map();
const newTarget = (url = "about:blank") => {
  const id = `T${next++}`;
  const t = { id, type: "page", url, title: url, webSocketDebuggerUrl: `ws://127.0.0.1:${server.address().port}/devtools/page/${id}` };
  targets.set(id, t);
  return t;
};

const server = createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  const json = (body) => { res.writeHead(200, { "content-type": "application/json" }); res.end(JSON.stringify(body)); };
  if (url.pathname === "/json/version") return json({ Browser: "FakeChrome/1" });
  if (url.pathname === "/json/list" || url.pathname === "/json") return json([...targets.values()]);
  if (url.pathname === "/json/new") { if (req.method !== "PUT") { res.writeHead(405); return res.end(); } return json(newTarget(decodeURIComponent(url.search.slice(1)) || "about:blank")); }
  if (url.pathname.startsWith("/json/close/")) { targets.delete(url.pathname.slice("/json/close/".length)); res.writeHead(200); return res.end("Target is closing"); }
  res.writeHead(404);
  res.end();
});

/** One text frame, unmasked, as a server sends it. */
const frame = (text) => {
  const body = Buffer.from(text);
  const head = body.length < 126 ? Buffer.from([0x81, body.length]) : body.length < 65536 ? Buffer.from([0x81, 126, body.length >> 8, body.length & 255]) : (() => { const h = Buffer.alloc(10); h[0] = 0x81; h[1] = 127; h.writeBigUInt64BE(BigInt(body.length), 2); return h; })();
  return Buffer.concat([head, body]);
};

server.on("upgrade", (req, socket) => {
  if (env.FAKE_CDP_NO_UPGRADE) return;
  const key = req.headers["sec-websocket-key"];
  const accept = createHash("sha1").update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  const say = (msg) => { try { socket.write(frame(JSON.stringify(msg))); } catch {} };
  const event = (method, params = {}) => { if (!hung(method)) say({ method, params }); };
  const answer = (msg) => {
    log("calls.log", msg.method);
    if (env.FAKE_CDP_DIE === msg.method && !(state && existsSync(join(state, "died")))) {
      if (state) writeFileSync(join(state, "died"), "");
      process.exit(1);
    }
    if (hung(msg.method, msg.params)) return;
    let result = {};
    if (msg.method === "Runtime.evaluate") result = { result: { type: typeof value, value } };
    if (msg.method === "Page.navigate") result = { frameId: "F", loaderId: "L" };
    if (msg.method === "Page.captureScreenshot") result = { data: "" };
    if (msg.method === "Performance.getMetrics") result = { metrics: [] };
    say({ id: msg.id, result });
    if (msg.method === "Page.navigate" || msg.method === "Page.reload") setTimeout(() => event("Page.loadEventFired", { timestamp: Date.now() / 1000 }), 5);
    if (msg.method === "Tracing.end") setTimeout(() => event("Tracing.tracingComplete", {}), 5);
  };
  let buf = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      if (buf.length < 2) return;
      const op = buf[0] & 15;
      let len = buf[1] & 127, at = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); at = 4; }
      else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); at = 10; }
      const masked = buf[1] & 128;
      if (buf.length < at + (masked ? 4 : 0) + len) return;
      const mask = masked ? buf.subarray(at, at + 4) : null;
      at += masked ? 4 : 0;
      const body = Buffer.from(buf.subarray(at, at + len));
      if (mask) for (let i = 0; i < body.length; i++) body[i] ^= mask[i % 4];
      buf = buf.subarray(at + len);
      if (op === 8) { socket.end(); return; }
      if (op === 1) answer(JSON.parse(body.toString()));
    }
  });
  socket.on("error", () => {});
});

server.listen(0, "127.0.0.1", () => {
  newTarget("about:blank");
  log("launches.log", "start");
  if (profile && !env.FAKE_CDP_NO_PORT) writeFileSync(join(profile, "DevToolsActivePort"), `${server.address().port}\n/devtools/browser/fake\n`);
});
process.on("SIGTERM", () => process.exit(0));
