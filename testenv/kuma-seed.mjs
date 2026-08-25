// Configure a fresh Uptime Kuma v2 to the testenv baseline: an admin account,
// the monitors of monitors.json, and an API key for /metrics.
//
// Kuma v2 has NO REST API for configuration (roadmap §4 gotcha 6, design
// §12.3) -- setup, login, monitor creation and API-key creation are all
// socket.io events. That is why this is node in a one-shot container and not
// curl in the seed script.
//
// The event names, argument order and callback shapes below were read off the
// PINNED image (louislam/uptime-kuma:2 => 2.5.3), not recalled:
//
//   needSetup(cb)                  -> cb(bool)                 // a bare bool!
//   setup(username, password, cb)  -> cb({ok, msg, msgi18n})
//   login({username, password}, cb)-> cb({ok, token})
//   getMonitorList(cb)             -> cb({ok}) + pushes `monitorList`
//   add(monitor, cb)               -> cb({ok, msg, monitorID})
//   getAPIKeyList(cb)              -> cb({ok}) + pushes `apiKeyList`
//   addAPIKey(key, cb)             -> cb({ok, key: "uk<id>_<secret>", keyID})
//   deleteAPIKey(keyID, cb)        -> cb({ok, msg})
//
// Two of those are traps worth naming: `needSetup` answers with a plain
// boolean rather than the `{ok, msg}` every other handler uses, and the list
// handlers answer `{ok:true}` while the actual list arrives as a separate
// pushed event -- so a script that reads the callback for its data sees
// nothing and concludes the instance is empty.
//
// PROTOCOL WITH THE HOST: stderr carries all progress; stdout carries exactly
// one line, either `KEY=<api key>` or `KEEP`. seed-kuma.sh parses that. The
// repo is mounted read-only, so nothing here writes into it -- a container
// writing to a bind mount leaves root-owned files behind on Linux.

import { readFileSync } from "node:fs";
import { io } from "socket.io-client";

// Named KUMA_URL, not URL: a module-scope `const URL` would shadow the global
// URL constructor that the monitors.json read below needs.
const KUMA_URL = process.env.KUMA_URL ?? "http://uptime-kuma:3001";
const USER = process.env.KUMA_USER ?? "knobas";
const PASS = process.env.KUMA_PASS ?? "knobas-dev";
const KEY_NAME = "knobas-seed";
// Set by seed-kuma.sh when testenv/kuma-api-key already exists on the host.
// Kuma returns an API key's clear text exactly once, at creation, so a key
// that exists here with no file there is unrecoverable and worthless.
const HOST_HAS_KEY = process.env.KNOBAS_HAVE_KEY === "1";

const log = (...a) => console.error("kuma-seed:", ...a);
const die = (msg) => { log("FAILED:", msg); process.exit(1); };

const monitors = JSON.parse(readFileSync(new URL("monitors.json", import.meta.url), "utf8"));

// Transports are left to negotiation (polling, then an upgrade) rather than
// pinned to "websocket". Kuma 2.5.3 refuses a websocket-only client outright
// -- `connect_error: websocket error` -- while the default order connects and
// upgrades cleanly. This is the seed's only connection, so the upgrade costs
// nothing worth pinning for.
const socket = io(KUMA_URL, { reconnection: false, timeout: 20000 });

/** Emit an event and resolve with its callback argument. */
const call = (event, ...args) =>
  new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`${event}: no callback within 30s`)), 30000);
    socket.emit(event, ...args, (res) => { clearTimeout(t); resolve(res); });
  });

/** Await the next push of `event` while running `trigger`. */
const listen = (event, trigger) =>
  new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`${event}: not pushed within 30s`)), 30000);
    socket.once(event, (payload) => { clearTimeout(t); resolve(payload); });
    trigger().catch(reject);
  });

/** Every handler but `needSetup` answers {ok, msg}; a falsy ok is fatal. */
const ok = (res, what) => {
  if (!res || res.ok !== true) die(`${what}: ${res?.msg ?? JSON.stringify(res)}`);
  return res;
};

await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("connect_error", (e) => reject(new Error(`cannot reach ${KUMA_URL}: ${e.message}`)));
}).catch((e) => die(e.message));
log("connected to", KUMA_URL);

// -- account ----------------------------------------------------------------
if (await call("needSetup")) {
  log("instance needs setup; creating admin", USER);
  ok(await call("setup", USER, PASS), "setup");
} else {
  log("instance already set up");
}
ok(await call("login", { username: USER, password: PASS }), "login");
log("logged in as", USER);

// -- monitors ---------------------------------------------------------------
// `monitorList` is an object keyed by monitor id, not an array.
const existing = await listen("monitorList", () => call("getMonitorList").then((r) => ok(r, "getMonitorList")));
const present = new Set(Object.values(existing ?? {}).map((m) => m.name));
log(`${present.size} monitor(s) already configured`);

for (const m of monitors) {
  if (present.has(m.name)) { log("keeping", JSON.stringify(m.name)); continue; }
  // The defaults the Kuma frontend sends with every http monitor.
  // accepted_statuscodes MUST be strings -- `add` rejects numbers explicitly.
  const res = await call("add", {
    type: m.type,
    name: m.name,
    url: m.url,
    interval: m.interval,
    retryInterval: m.interval,
    resendInterval: 0,
    maxretries: 0,
    timeout: 16,
    method: "GET",
    accepted_statuscodes: ["200-299"],
    active: true,
    maxredirects: 10,
    expiryNotification: false,
    ignoreTls: false,
    upsideDown: false,
    notificationIDList: {},
    conditions: [],
  });
  ok(res, `add ${m.name}`);
  log("added", JSON.stringify(m.name), "as monitor", res.monitorID);
}

// -- API key ----------------------------------------------------------------
const keys = await listen("apiKeyList", () => call("getAPIKeyList").then((r) => ok(r, "getAPIKeyList")));
const mine = (keys ?? []).find((k) => k.name === KEY_NAME);

if (mine && HOST_HAS_KEY) {
  log(`API key ${KEY_NAME} exists and the host still has it; keeping`);
  console.log("KEEP");
} else {
  if (mine) {
    // Unrecoverable: Kuma hands the clear text out once. Replacing it is the
    // only way back to a working /metrics, and leaving it would be a key
    // nobody can use sitting in the list forever.
    log(`API key ${KEY_NAME} exists but the host has no copy; replacing it`);
    ok(await call("deleteAPIKey", mine.id), "deleteAPIKey");
  }
  const res = ok(await call("addAPIKey", { name: KEY_NAME, expires: null, active: true }), "addAPIKey");
  if (typeof res.key !== "string" || !res.key.startsWith("uk")) {
    die(`addAPIKey returned no usable key: ${JSON.stringify(res)}`);
  }
  log(`created API key ${KEY_NAME} (id ${res.keyID})`);
  console.log(`KEY=${res.key}`);
}

socket.disconnect();
log("done");
process.exit(0);
