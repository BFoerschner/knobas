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
//   deleteMonitor(id, children, cb)-> cb({ok, msg})           // three args!
//   getAPIKeyList(cb)              -> cb({ok}) + pushes `apiKeyList`
//   addAPIKey(key, cb)             -> cb({ok, key: "uk<id>_<secret>", keyID})
//   deleteAPIKey(keyID, cb)        -> cb({ok, msg})
//
// Three of those are traps worth naming: `needSetup` answers with a plain
// boolean rather than the `{ok, msg}` every other handler uses; the list
// handlers answer `{ok:true}` while the actual list arrives as a separate
// pushed event -- so a script that reads the callback for its data sees
// nothing and concludes the instance is empty; and `deleteMonitor` takes a
// `deleteChildren` flag BETWEEN the id and the callback (2.5.3 still accepts
// the two-argument form by sniffing for a function, but the three-argument
// call is the one the handler is written for).
//
// THIS SCRIPT OWNS THE MONITOR LIST. `monitors.json` is the whole truth: an
// entry it no longer names is deleted, an entry whose definition drifted is
// deleted and re-added, and a matching one is left alone so that a second run
// changes nothing. See README.md, "Monitors".
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
// Set by seed-kuma.sh when testenv/kuma-api-key holds a key this instance
// still answers 200 to -- the host verifies before it claims (issue #552),
// because a file is not a credential. Kuma returns an API key's clear text
// exactly once, at creation, so a key that exists here with no working copy
// there is unrecoverable and worthless. The name says the host measured an
// answer, not that it found a file (issue #554).
//
// The variable name is half a contract with seed-kuma.sh across the container
// boundary, and the failure is silent in one direction: an unset variable is
// not "1", so a rename there without this one reads as "did not authenticate"
// and re-mints on EVERY run -- killing every sibling worktree's copy, while a
// single `just kuma-live` still passes because one run only ever exercises the
// re-mint side.
const HOST_KEY_AUTHENTICATED = process.env.KNOBAS_KEY_AUTHENTICATED === "1";

const log = (...a) => console.error("kuma-seed:", ...a);
const die = (msg) => { log("FAILED:", msg); process.exit(1); };

// The estate's three servers are addressed by IP, and the IPs live in
// hetzner/hosts.env, which provision.sh writes and .gitignore keeps out of the
// repo. So monitors.json carries `${VAR}` placeholders and seed-kuma.sh passes
// the values in. An unresolved placeholder is fatal: a ping monitor on the
// literal string "${KNOBAS_HETZNER_JIRA_IP}" is a permanent red in the UI that
// looks like an outage.
const expand = (s, where) =>
  s.replace(/\$\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_, name) => {
    const v = process.env[name];
    if (!v) die(`${where}: \${${name}} is not set -- run hetzner/provision.sh, or see README.md "Monitors"`);
    return v;
  });

const monitors = JSON.parse(readFileSync(new URL("monitors.json", import.meta.url), "utf8"))
  .map((m) => Object.fromEntries(
    Object.entries(m).map(([k, v]) => [k, typeof v === "string" ? expand(v, `monitors.json: ${m.name}`) : v]),
  ));

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
/**
 * The `add` payload for one monitors.json entry: the defaults the Kuma
 * frontend sends, plus the per-type fields. The ping defaults (count 3,
 * numeric, 56-byte packets, 2 s per request) are EditMonitor.vue's own.
 *
 * `accepted_statuscodes` is in the BASE, not in the http branch, and it MUST
 * be strings. server.js:752 runs `monitor.accepted_statuscodes.every(...)` on
 * the way in with no type check first, so a ping monitor without the field --
 * which needs no status codes at all -- is refused with `Cannot read
 * properties of undefined (reading 'every')`, an error that names neither the
 * field nor the monitor. (Hit on 2.5.3, 2026-09-06, adding the first ping.)
 */
const payload = (m) => ({
  type: m.type,
  name: m.name,
  interval: m.interval,
  retryInterval: m.interval,
  resendInterval: 0,
  maxretries: 0,
  timeout: 16,
  accepted_statuscodes: ["200-299"],
  active: true,
  expiryNotification: false,
  ignoreTls: false,
  upsideDown: false,
  notificationIDList: {},
  conditions: [],
  ...(m.type === "ping"
    ? { hostname: m.hostname, ping_count: 3, ping_numeric: true, packetSize: 56, ping_per_request_timeout: 2 }
    : { url: m.url, method: "GET", maxredirects: 10 }),
});

/**
 * The fields monitors.json is allowed to decide. Anything else about a monitor
 * -- notifications, tags, a paused state a live run left behind -- is the
 * instance's business and is not drift.
 */
const OWNED = ["type", "url", "hostname", "interval"];
const drift = (have, want) =>
  OWNED.filter((k) => want[k] !== undefined && String(have[k] ?? "") !== String(want[k]))
       .map((k) => `${k}: ${JSON.stringify(have[k] ?? null)} -> ${JSON.stringify(want[k])}`);

// `monitorList` is an object keyed by monitor id, not an array.
const existing = await listen("monitorList", () => call("getMonitorList").then((r) => ok(r, "getMonitorList")));
const rows = Object.values(existing ?? {});
log(`${rows.length} monitor(s) already configured`);

// Delete first, so a rename frees its old row before the new one is added and
// the count in the UI never overshoots. `false` is `deleteChildren`: nothing
// here is a group monitor, and unlinking is the safe answer if one ever is.
//
// The sweep walks the ROWS, and `byName` is filled from it rather than built
// by `new Map(rows.map(...))`. A Map keyed by name collapses duplicates, and
// Kuma's UI will happily hold two monitors called the same thing -- so a
// name-keyed sweep would delete one of a duplicated pair and leave the other,
// while this script printed that Kuma now holds exactly what monitors.json
// names. Keeping the first row of a wanted name and deleting every other row
// makes that sentence true whatever the instance was holding.
const wanted = new Set(monitors.map((m) => m.name));
const byName = new Map();
for (const m of rows) {
  if (wanted.has(m.name) && !byName.has(m.name)) { byName.set(m.name, m); continue; }
  ok(await call("deleteMonitor", m.id, false), `deleteMonitor ${m.name}`);
  log("deleted", JSON.stringify(m.name),
      wanted.has(m.name) ? "-- a second monitor of that name" : "-- monitors.json no longer names it");
}

for (const m of monitors) {
  const have = byName.get(m.name);
  const changed = have ? drift(have, m) : null;
  if (have && changed.length === 0) { log("keeping", JSON.stringify(m.name)); continue; }
  if (have) {
    // Kuma has `editMonitor`, but it wants the whole stored row back and a
    // partial one silently blanks fields. A testenv monitor's history is worth
    // nothing, so drift is repaired by delete-and-add, which cannot half-apply.
    log("redefining", JSON.stringify(m.name), `(${changed.join("; ")})`);
    ok(await call("deleteMonitor", have.id, false), `deleteMonitor ${m.name}`);
  }
  const res = ok(await call("add", payload(m)), `add ${m.name}`);
  log("added", JSON.stringify(m.name), "as monitor", res.monitorID);
}

// -- API key ----------------------------------------------------------------
const keys = await listen("apiKeyList", () => call("getAPIKeyList").then((r) => ok(r, "getAPIKeyList")));
const mine = (keys ?? []).find((k) => k.name === KEY_NAME);

if (mine && HOST_KEY_AUTHENTICATED) {
  log(`API key ${KEY_NAME} exists and the host's copy still authenticates; keeping`);
  console.log("KEEP");
} else {
  if (mine) {
    // Unrecoverable: Kuma hands the clear text out once. Replacing it is the
    // only way back to a working /metrics, and leaving it would be a key
    // nobody can use sitting in the list forever. "No working copy" covers
    // both halves of what the host checked: no file at all, and a file whose
    // key the instance has since rotated away (issue #552).
    log(`API key ${KEY_NAME} exists but the host has no working copy; replacing it`);
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
