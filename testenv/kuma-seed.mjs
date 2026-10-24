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
// This worktree's key name, `knobas-seed-<worktree>-<checksum>`, derived host
// side by seed-kuma.sh (issue #1484): the container sees only its read-only
// mount, not which worktree it came from. Everything below keeps, replaces or
// adds the key of THIS name and no other, so a sibling worktree's key, and the
// pre-#1484 shared `knobas-seed`, are never deleted. Refused when unset rather
// than defaulted: a shared default is the collision #1484 ended, and a rename
// on one side of the boundary would bring it back without failing a single
// `just kuma-live`.
const KEY_NAME = process.env.KNOBAS_KEY_NAME ?? "";
if (!/^knobas-seed-.+-[0-9]+$/.test(KEY_NAME)) {
  console.error(`kuma-seed: FAILED: KNOBAS_KEY_NAME is ${JSON.stringify(KEY_NAME)}, not a per-worktree key name; run ./seed-kuma.sh, which derives it`);
  process.exit(1);
}
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
// and re-mints on EVERY run -- killing this worktree's working copy each time
// (and, before #1484, every sibling worktree's), while a single `just
// kuma-live` still passes because one run only ever exercises the re-mint side.
const HOST_KEY_AUTHENTICATED = process.env.KNOBAS_KEY_AUTHENTICATED === "1";

const log = (...a) => console.error("kuma-seed:", ...a);
const die = (msg) => { log("FAILED:", msg); process.exit(1); };

// monitors.json is read as it is: every value in it is literal. (Until #634 it
// carried `${VAR}` placeholders for the Hetzner servers' addresses, resolved
// from a gitignored hosts.env; the servers are gone and so is the expansion.)
const monitors = JSON.parse(readFileSync(new URL("monitors.json", import.meta.url), "utf8"));

// The guard on the connection and on every callback and push below (#1961):
// KUMA_WAIT_S, whole seconds, 120 by default. A loaded machine answers slowly,
// and the old fixed 20 s to connect, 30 s per event and 5 s for the login's
// dockerHostList failed seeds with nothing
// wrong in Kuma. A socket Kuma drops fails the wait at once, so the guard
// costs a real fault nothing.
const WAIT_S = process.env.KUMA_WAIT_S || "120";
if (!/^[1-9][0-9]*$/.test(WAIT_S)) {
  console.error(`kuma-seed: FAILED: KUMA_WAIT_S must be a whole number of seconds, got '${WAIT_S}'`);
  process.exit(2);
}
const WAIT_MS = Number(WAIT_S) * 1000;

// Transports are left to negotiation (polling, then an upgrade) rather than
// pinned to "websocket". Kuma 2.5.3 refuses a websocket-only client outright
// -- `connect_error: websocket error` -- while the default order connects and
// upgrades cleanly. This is the seed's only connection, so the upgrade costs
// nothing worth pinning for.
const socket = io(KUMA_URL, { reconnection: false, timeout: WAIT_MS });

/** Emit an event and resolve with its callback argument. */
const call = (event, ...args) =>
  new Promise((resolve) => {
    const t = setTimeout(() => die(`${event}: no callback within ${WAIT_S} s (KUMA_WAIT_S)`), WAIT_MS);
    socket.emit(event, ...args, (res) => { clearTimeout(t); resolve(res); });
  });

/** Await the next push of `event` while running `trigger`. */
const listen = (event, trigger) =>
  new Promise((resolve, reject) => {
    const t = setTimeout(() => die(`${event}: not pushed within ${WAIT_S} s (KUMA_WAIT_S)`), WAIT_MS);
    socket.once(event, (payload) => { clearTimeout(t); resolve(payload); });
    trigger().catch(reject);
  });

/** Every handler but `needSetup` answers {ok, msg}; a falsy ok is fatal. */
const ok = (res, what) => {
  if (!res || res.ok !== true) die(`${what}: ${res?.msg ?? JSON.stringify(res)}`);
  return res;
};

// Kuma pushes `dockerHostList` during login, before the login's ack, so the
// listener has to exist first. A later push (after addDockerHost) replaces it.
let dockerHosts = null;
socket.on("dockerHostList", (list) => { dockerHosts = list; });

await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("connect_error", (e) => reject(new Error(`cannot reach ${KUMA_URL}: ${e.message} (KUMA_WAIT_S is ${WAIT_S} s)`)));
}).catch((e) => die(e.message));
log("connected to", KUMA_URL);
// Every wait below ends here if Kuma drops the socket; only this script's own
// `socket.disconnect()` at the end is not a failure.
socket.on("disconnect", (reason) => {
  if (reason !== "io client disconnect") die(`Kuma closed the connection (${reason})`);
});

// -- account ----------------------------------------------------------------
if (await call("needSetup")) {
  log("instance needs setup; creating admin", USER);
  ok(await call("setup", USER, PASS), "setup");
} else {
  log("instance already set up");
}
ok(await call("login", { username: USER, password: PASS }), "login");
log("logged in as", USER);

// -- docker host ------------------------------------------------------------
// One Docker host, so a Docker monitor created through knobas has a host to
// name (#978: `CreateMonitor { kind: docker }` takes the lowest-id host of the
// login push). It points at the `docker-proxy` service, a socket proxy that
// exposes only container reads -- NOT at a mounted /var/run/docker.sock, which
// would hand Kuma control of the Docker daemon (see docker-compose.yml).
const DOCKER_HOST_NAME = "testenv-docker-proxy";
const DOCKER_HOST = { name: DOCKER_HOST_NAME, dockerType: "tcp", dockerDaemon: "tcp://docker-proxy:2375" };
const loggedInAt = Date.now();
while (dockerHosts === null && Date.now() - loggedInAt < WAIT_MS) await new Promise((r) => setTimeout(r, 250));
if (dockerHosts === null) die(`Kuma pushed no dockerHostList within ${WAIT_S} s of the login's ack (KUMA_WAIT_S)`);
const haveHost = dockerHosts.find((h) => h.name === DOCKER_HOST_NAME);
if (haveHost && haveHost.dockerType === DOCKER_HOST.dockerType && haveHost.dockerDaemon === DOCKER_HOST.dockerDaemon) {
  log("keeping docker host", DOCKER_HOST_NAME);
} else {
  // A drifted host is edited in place (its id is what monitors reference).
  const res = ok(await call("addDockerHost", DOCKER_HOST, haveHost?.id ?? null), "addDockerHost");
  log(haveHost ? "redefined" : "added", "docker host", DOCKER_HOST_NAME, "as", res.id);
}

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
// Every key of this worktree's name, not the first: a stale one is replaced,
// never duplicated, so the instance holds one key per worktree name.
const mine = (keys ?? []).filter((k) => k.name === KEY_NAME);

if (mine.length > 0 && HOST_KEY_AUTHENTICATED) {
  log(`API key ${KEY_NAME} exists and the host's copy still authenticates; keeping`);
  console.log("KEEP");
} else {
  for (const stale of mine) {
    // Unrecoverable: Kuma hands the clear text out once. Replacing it is the
    // only way back to a working /metrics, and leaving it would be a key
    // nobody can use sitting in the list forever. "No working copy" covers
    // both halves of what the host checked: no file at all, and a file whose
    // key the instance has since deleted (issue #552). Only this worktree's
    // name is in `mine`, so no other worktree's key goes with it (#1484).
    log(`API key ${KEY_NAME} (id ${stale.id}) exists but the host has no working copy; replacing it`);
    ok(await call("deleteAPIKey", stale.id), "deleteAPIKey");
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
