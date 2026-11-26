// Uptime Kuma at Björn's scale: SCALE_MONITORS (200) monitors named
// `scale-...`, in twelve groups, beside the six of monitors.json. Run by
// seed-scale.sh kuma inside the kuma-seed container, as kuma-monitor.sh runs
// its helper (Kuma v2 configures over socket.io only; kuma-seed.mjs's header
// names the events and their traps).
//
// Every monitor checks something on the compose network, so nothing leaves
// the machine: Gitea's health endpoint and its scale repositories over http,
// a keyword on a page, Gitea's port over tcp, a ping of the loopback -- and
// about one in twenty a Gitea path that answers 404, so the estate has some
// monitors down. Intervals of 2 to 10 minutes keep the load under one check a
// second. Idempotent by name. `./seed-kuma.sh` owns the monitor list but
// leaves every `scale-...` monitor alone, so they stay until the Kuma volume is
// reset (testenv/README.md, "Monitors" and "Björn's scale").
import { io } from "socket.io-client";

const KUMA_URL = process.env.KUMA_URL ?? "http://uptime-kuma:3001";
const N = Number(process.env.SCALE_MONITORS || "200");
const log = (...a) => console.error("kuma-scale:", ...a);
const die = (m) => { log("FAILED:", m); process.exit(1); };
const WAIT_MS = 120000;
const socket = io(KUMA_URL, { reconnection: false, timeout: WAIT_MS });
const call = (event, ...args) => new Promise((resolve) => {
  const t = setTimeout(() => die(`${event}: no callback`), WAIT_MS);
  socket.emit(event, ...args, (res) => { clearTimeout(t); resolve(res); });
});
const listen = (event, trigger) => new Promise((resolve) => {
  const t = setTimeout(() => die(`${event}: not pushed`), WAIT_MS);
  socket.once(event, (p) => { clearTimeout(t); resolve(p); });
  trigger();
});
const ok = (res, what) => { if (!res || res.ok !== true) die(`${what}: ${res?.msg ?? JSON.stringify(res)}`); return res; };

await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("connect_error", (e) => reject(e));
}).catch((e) => die(`cannot reach ${KUMA_URL}: ${e.message}`));
ok(await call("login", { username: process.env.KUMA_USER ?? "knobas", password: process.env.KUMA_PASS ?? "knobas-dev" }), "login");

// A seeded generator, so a rerun plans the same monitors.
let s = 20261110;
const rnd = () => { s = (s * 1103515245 + 12345) % 2147483648; return s / 2147483648; };
const pick = (a) => a[Math.floor(rnd() * a.length)];
const GROUPS = ["payments", "ledger", "checkout", "catalog", "search", "data-pipeline", "mobile-api", "web-edge",
  "identity", "notifications", "internal-tools", "partners"];
const REPOS = ["scale-platform/auth-gateway", "scale-platform/service-mesh", "scale-commerce/checkout-web",
  "scale-commerce/cart-service", "scale-data/warehouse-loader", "scale-data/event-bus"];
const ENVS = ["prod", "staging", "dev"];

const base = (m) => ({
  interval: m.interval, retryInterval: m.interval, resendInterval: 0, maxretries: 1, timeout: 16,
  accepted_statuscodes: ["200-299"], active: true, expiryNotification: false, ignoreTls: false,
  upsideDown: false, notificationIDList: {}, conditions: [], ...m,
});
const plan = [];
for (const g of GROUPS) plan.push({ name: `scale-${g}`, type: "group", interval: 300 });
for (let i = 0; plan.length < N + GROUPS.length; i++) {
  const g = GROUPS[i % GROUPS.length];
  const env = pick(ENVS);
  const interval = pick([120, 180, 300, 300, 600]);
  const x = rnd();
  const name = `scale-${g}-${env}-${i + 1}`;
  if (x < 0.05) plan.push({ name, group: g, type: "http", interval, url: `http://gitea:3000/scale-missing/${g}-${i}`, method: "GET", maxredirects: 10 });
  else if (x < 0.45) plan.push({ name, group: g, type: "http", interval, url: "http://gitea:3000/api/healthz", method: "GET", maxredirects: 10 });
  else if (x < 0.7) plan.push({ name, group: g, type: "http", interval, url: `http://gitea:3000/${pick(REPOS)}`, method: "GET", maxredirects: 10 });
  else if (x < 0.8) plan.push({ name, group: g, type: "keyword", interval, url: "http://gitea:3000/api/healthz", method: "GET", maxredirects: 10, keyword: "pass" });
  else if (x < 0.93) plan.push({ name, group: g, type: "port", interval, hostname: "gitea", port: 3000 });
  else plan.push({ name, group: g, type: "ping", interval, hostname: "127.0.0.1", ping_count: 3, ping_numeric: true, packetSize: 56, ping_per_request_timeout: 2 });
}

const existing = Object.values(await listen("monitorList", () => call("getMonitorList")) ?? {});
const byName = new Map(existing.map((m) => [m.name, m.id]));
let added = 0;
for (const m of plan) {
  if (byName.has(m.name)) continue;
  const { group, ...rest } = m;
  const payload = base(rest);
  if (group) payload.parent = byName.get(`scale-${group}`);
  const res = ok(await call("add", payload), `add ${m.name}`);
  byName.set(m.name, res.monitorID);
  added++;
  if (added % 50 === 0) log(`added ${added}`);
}
log(`done: ${plan.length - GROUPS.length} scale monitors in ${GROUPS.length} groups; this run added ${added}`);
socket.disconnect();
process.stdout.write("OK\n");
