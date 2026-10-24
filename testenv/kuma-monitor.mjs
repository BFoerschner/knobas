// One monitor, added or deleted through the same socket.io channel the seed
// uses. This is the *scratch* half of the Kuma seed: `kuma-seed.mjs` owns the
// baseline list that monitors.json names, and this owns exactly one monitor
// that the baseline deliberately does not name.
//
// WHY IT EXISTS. `just kuma-live` has to witness that a monitor deleted in
// Kuma leaves the mirror on the next run (M4 spec #427, story 54). The only
// honest way to witness that is to delete a real monitor -- and every monitor
// `monitors.json` names is part of the estate, watched by whatever else is
// reading Kuma. So the live run gets one monitor of its own, whose whole blast
// radius is a name nothing else uses, exactly as the canary gets a port
// nothing else binds.
//
// Kuma v2 has NO REST API for configuration (roadmap §4 gotcha 6), so this is
// node in a one-shot container and not curl -- see kuma-seed.mjs, whose header
// records the event names, argument order and callback shapes read off the
// pinned image (2.5.3) and the three traps in them.
//
// THE SEED IS THE LITTER GUARD. `kuma-seed.mjs` deletes every monitor
// `monitors.json` does not name, so a scratch monitor a killed run left behind
// is removed by the next `./seed-kuma.sh` -- which `just kuma-live` runs before
// the suite. Nothing here has to survive a crash.
//
// PROTOCOL WITH THE HOST: stderr carries all progress, stdout carries nothing.
// The exit status is the answer. The repo is mounted read-only, so nothing
// here writes into it.
//
//   node kuma-monitor.mjs add <name> <url>
//   node kuma-monitor.mjs delete <name>

import { io } from "socket.io-client";

const KUMA_URL = process.env.KUMA_URL ?? "http://uptime-kuma:3001";
const USER = process.env.KUMA_USER ?? "knobas";
const PASS = process.env.KUMA_PASS ?? "knobas-dev";

const log = (...a) => console.error("kuma-monitor:", ...a);
const die = (msg) => { log("FAILED:", msg); process.exit(1); };

const [op, name, url] = process.argv.slice(2);
if (op !== "add" && op !== "delete") die("usage: kuma-monitor.mjs add <name> <url> | delete <name>");
if (!name) die("a monitor name is required");
if (op === "add" && !url) die("add needs a url");

// Transports left to negotiation, for kuma-seed.mjs's reason: 2.5.3 refuses a
// websocket-only client outright.
const socket = io(KUMA_URL, { reconnection: false, timeout: 20000 });

// The guard on every callback and push below (#1961): KUMA_WAIT_S, whole
// seconds, 120 by default. A loaded machine answers slowly, and the old 30 s
// per event (5 s for the login's dockerHostList) failed seeds with nothing
// wrong in Kuma. A socket Kuma drops fails the wait at once, so the guard
// costs a real fault nothing.
const WAIT_S = process.env.KUMA_WAIT_S || "120";
if (!/^[1-9][0-9]*$/.test(WAIT_S)) {
  console.error(`kuma-monitor: FAILED: KUMA_WAIT_S must be a whole number of seconds, got '${WAIT_S}'`);
  process.exit(2);
}
const WAIT_MS = Number(WAIT_S) * 1000;

const call = (event, ...args) =>
  new Promise((resolve) => {
    const t = setTimeout(() => die(`${event}: no callback within ${WAIT_S} s (KUMA_WAIT_S)`), WAIT_MS);
    socket.emit(event, ...args, (res) => { clearTimeout(t); resolve(res); });
  });

const listen = (event, trigger) =>
  new Promise((resolve, reject) => {
    const t = setTimeout(() => die(`${event}: not pushed within ${WAIT_S} s (KUMA_WAIT_S)`), WAIT_MS);
    socket.once(event, (payload) => { clearTimeout(t); resolve(payload); });
    trigger().catch(reject);
  });

const ok = (res, what) => {
  if (!res || res.ok !== true) die(`${what}: ${res?.msg ?? JSON.stringify(res)}`);
  return res;
};

await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("connect_error", (e) => reject(new Error(`cannot reach ${KUMA_URL}: ${e.message}`)));
}).catch((e) => die(e.message));
log("connected to", KUMA_URL);
// Every wait below ends here if Kuma drops the socket; only this script's own
// `socket.disconnect()` at the end is not a failure.
socket.on("disconnect", (reason) => {
  if (reason !== "io client disconnect") die(`Kuma closed the connection (${reason})`);
});

if (await call("needSetup")) die("this Kuma has no admin account yet -- run ./seed-kuma.sh first");
// Kuma 2.5.3 allows twenty logins a minute across every client
// (`server/rate-limiter.js`, `loginRateLimiter`) and refuses the next with
// this message inside an ok-shaped callback. A live run spends logins in the
// adapter and in this helper alike, so a refusal for the limit waits out the
// window once rather than failing a test on a fixture's budget (#1627).
const RATE_LIMITED = "Too frequently, try again later.";
let login = await call("login", { username: USER, password: PASS });
if (login?.ok !== true && login?.msg === RATE_LIMITED) {
  log("login refused for the rate limit; waiting 61 s for the next window");
  await new Promise((resolve) => setTimeout(resolve, 61000));
  login = await call("login", { username: USER, password: PASS });
}
ok(login, "login");

// `monitorList` is an object keyed by monitor id, not an array, and it arrives
// as a pushed event rather than in the callback (kuma-seed.mjs, trap 2).
const existing = await listen("monitorList", () => call("getMonitorList").then((r) => ok(r, "getMonitorList")));
const rows = Object.values(existing ?? {}).filter((m) => m.name === name);

// Both operations start by deleting every row of that name, so both are
// idempotent and `add` twice leaves one monitor rather than two -- Kuma's UI
// will happily hold two monitors called the same thing, and a duplicate would
// make "the mirror stopped holding it" untestable.
for (const m of rows) {
  // `false` is `deleteChildren`, and it sits BETWEEN the id and the callback
  // (kuma-seed.mjs, trap 3).
  ok(await call("deleteMonitor", m.id, false), `deleteMonitor ${name}`);
  log("deleted", JSON.stringify(name), `(monitor ${m.id})`);
}
if (op === "delete" && rows.length === 0) log("nothing to delete:", JSON.stringify(name));

if (op === "add") {
  // The same payload shape kuma-seed.mjs sends for an http monitor, including
  // `accepted_statuscodes` -- which server.js runs `.every(...)` on for every
  // monitor of every type with no type check first.
  const res = ok(await call("add", {
    type: "http",
    name,
    url,
    interval: 60,
    retryInterval: 60,
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
    method: "GET",
    maxredirects: 10,
  }), `add ${name}`);
  log("added", JSON.stringify(name), "as monitor", res.monitorID);
}

socket.disconnect();
log("done");
process.exit(0);
