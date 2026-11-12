// A forge in front of a forge, for `just open-budget --forge`: forwards bytes between knobas and a live product on
// loopback and holds the first chunk of each request for the delay asked, as `crates/knobas-app/tests/slow_proxy`
// does for the Rust rigs (PR #2479's smoothness passes). The inner-pages pass (2026-11-12) points the measured
// profile's Gitea source at this proxy once, so the source's address, and so its held reads (ADR-0042), are the same
// whether the forge is live (`--delay 0`) or slowed (`--delay 1000`).
//
//   node testenv/perf/slow-forge.mjs --listen 3900 --upstream 127.0.0.1:3000 [--delay 1000] [--control 3901]
//
// --control  a loopback port whose `POST /delay?ms=<n>` sets the delay while it runs, and `GET /count` says how many
//            requests passed (each a request's first chunk), so a measured open can say how many reached the forge
// Every byte goes on unchanged; nothing is written anywhere.
import net from "node:net";
import http from "node:http";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const listen = Number(opt("--listen"));
const [host, port] = (opt("--upstream") ?? "").split(":");
let delay = Number(opt("--delay", "0"));
const control = opt("--control");
if (!listen || !host || !port) { console.log("slow-forge: --listen <port> --upstream <host:port> are required"); process.exit(2); }
let requests = 0;

net.createServer((client) => {
  const server = net.connect(Number(port), host);
  let fresh = true;
  let chain = Promise.resolve();
  client.on("data", (chunk) => {
    const first = fresh;
    fresh = false;
    if (first) requests++;
    const hold = first && delay > 0 ? delay : 0;
    client.pause();
    chain = chain.then(() => new Promise((res) => setTimeout(res, hold))).then(() => { server.write(chunk); client.resume(); });
  });
  server.on("data", (chunk) => { fresh = true; client.write(chunk); });
  const end = () => { client.destroy(); server.destroy(); };
  client.on("end", () => server.end());
  server.on("end", () => client.end());
  client.on("error", end);
  server.on("error", end);
}).listen(listen, "127.0.0.1", () => console.log(`slow-forge: 127.0.0.1:${listen} -> ${host}:${port}, ${delay} ms a request`));

if (control) {
  http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    if (req.method === "POST" && url.pathname === "/delay") { delay = Number(url.searchParams.get("ms") ?? "0"); res.end(`${delay}\n`); return; }
    if (url.pathname === "/count") { res.end(`${requests}\n`); return; }
    res.statusCode = 404; res.end();
  }).listen(Number(control), "127.0.0.1");
}
