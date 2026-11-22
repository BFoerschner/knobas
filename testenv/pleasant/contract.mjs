#!/usr/bin/env node
// The Pleasant contract (testenv/pleasant/README.md): one request list (requests.json) sent to two servers holding the
// same 20 entries, every answer normalised the same way, and any difference a failure.
//
//   node testenv/pleasant/contract.mjs record      the real server -> recorded/ (rewrites the recordings)
//   node testenv/pleasant/contract.mjs --live      the real server against a fresh stand-in (plan(20))
//   node testenv/pleasant/contract.mjs --recorded  a fresh stand-in against recorded/ (no VM; part of witness-unit)
//
// The real server comes from KNOBAS_PLEASANT_URL and three users: KNOBAS_PLEASANT_USER/_PASSWORD (an admin, who binds
// the plan's names to the server's ids), KNOBAS_PLEASANT_WRITER_USER/_PASSWORD and KNOBAS_PLEASANT_READER_USER/_PASSWORD.
// The expired-token step runs KNOBAS_PLEASANT_EXPIRE_CMD (moves the server's clock past a token's life) and afterwards
// KNOBAS_PLEASANT_RESTORE_CMD; `just pleasant-contract` sets both for the WSL VM.
//
// What "the same" means is `normalise` below: ids become the plan's names, times keep only their format, secrets are
// masked with their kind and size kept, and everything else (names, casing, types, nulls, arrays and their order,
// status codes, error bodies, and the headers in HEADERS) must match exactly.
import { spawn } from "node:child_process";
import { execSync } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request, token } from "./client.mjs";
import { PROTECTED } from "./plan.mjs";

const HERE = new URL(".", import.meta.url).pathname;
const RECORDED = `${HERE}recorded`;
export const HEADERS = ["content-type", "www-authenticate", "x-pleasant-otp", "x-pleasant-otp-provider", "location"];

const list = JSON.parse(readFileSync(`${HERE}requests.json`, "utf8")).requests;

/** The stand-in's users when it stands in for the real server in a contract run: the real server's names and passwords. */
export function realUsers(env = process.env) {
  const need = (n) => env[n] || (console.error(`error: ${n} is unset (testenv/pleasant/README.md)`), process.exit(1));
  return {
    url: need("KNOBAS_PLEASANT_URL"),
    admin: { user: need("KNOBAS_PLEASANT_USER"), password: need("KNOBAS_PLEASANT_PASSWORD") },
    writer: { user: need("KNOBAS_PLEASANT_WRITER_USER"), password: need("KNOBAS_PLEASANT_WRITER_PASSWORD") },
    reader: { user: need("KNOBAS_PLEASANT_READER_USER"), password: need("KNOBAS_PLEASANT_READER_PASSWORD") },
  };
}

/** The plan's names for the server's ids: folders by path, entries by plan key, attachments by entry and position. */
export async function bind(base, admin) {
  const { plan } = await import("./plan.mjs");
  const p = plan(20);
  const tok = await token(base, admin.user, admin.password);
  const tree = (await request(base, "GET", "/api/v5/rest/folders", { token: tok })).json;
  const names = { root: tree.Id, folders: {}, entries: {}, attachments: {} };
  const walk = (f, path) => {
    if (path.length) names.folders[path.join("/")] = f.Id;
    for (const c of f.Children ?? []) walk(c, [...path, c.Name]);
    for (const e of f.Credentials ?? []) {
      const planned = p.entries.find((x) => x.path.join("/") === path.join("/") && x.name === e.Name);
      if (planned) names.entries[planned.key] = e.Id;
    }
  };
  walk(tree, []);
  for (const e of p.entries.filter((x) => x.files.length)) {
    const id = names.entries[e.key];
    if (!id) continue;
    const atts = (await request(base, "GET", `/api/v5/rest/entries/${id}/attachments`, { token: tok })).json ?? [];
    atts.forEach((a, i) => { names.attachments[`${e.key}:${i}`] = a.AttachmentId; });
  }
  const missing = p.entries.filter((e) => !names.entries[e.key]).map((e) => e.key);
  if (missing.length) throw new Error(`the server lacks plan entries ${missing.join(", ")}: run seed.mjs first`);
  return names;
}

const fill = (s, names, users) => s.replace(/\{(entry|folder|attachment|user|password):([^}]+)\}|\{root\}/g, (m, kind, key) => {
  if (m === "{root}") return names.root;
  if (kind === "user") return users[key].user;
  if (kind === "password") return users[key].password;
  const v = (kind === "entry" ? names.entries : kind === "folder" ? names.folders : names.attachments)[key];
  if (!v) throw new Error(`no ${kind} ${key}`);
  return v;
});
const fillDeep = (o, names, users) => typeof o === "string" ? fill(o, names, users)
  : Array.isArray(o) ? o.map((x) => fillDeep(x, names, users))
  : o && typeof o === "object" ? Object.fromEntries(Object.entries(o).map(([k, v]) => [k, fillDeep(v, names, users)])) : o;

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const TIME = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)?$/;
const B32 = /(?<![A-Z2-7])[A-Z2-7]{32}(?![A-Z2-7])/g;

/** One answer in the form both servers are compared in. `secret` marks a body that is a password. */
export function normalise(r, names, { secret = false } = {}) {
  const rev = new Map();
  if (names) {
    rev.set(names.root, "{root}");
    for (const [k, v] of Object.entries(names.folders)) rev.set(v.toLowerCase(), `{folder:${k}}`);
    for (const [k, v] of Object.entries(names.entries)) rev.set(v.toLowerCase(), `{entry:${k}}`);
    for (const [k, v] of Object.entries(names.attachments)) rev.set(v.toLowerCase(), `{attachment:${k}}`);
    rev.set(String(names.root).toLowerCase(), "{root}");
  }
  const str = (s, key) => {
    if (key === "Password" || (secret && key === null)) return "<password>";
    if (key === "access_token" || key === "refresh_token") return "<token>";
    if (key === "FileData") return `<base64 ${Buffer.from(s, "base64").length} bytes>`;
    if (key === PROTECTED) return "<protected>";
    if (["ServerVersion", "NetVersion", "OsVersion", "DnsInformation", "Ping"].includes(key)) return `<${key}>`;
    if (GUID.test(s)) return rev.get(s.toLowerCase()) ?? "<guid>";
    // the format, not the moment: digits, fraction and the offset's sign (+01:00 in Germany, -08:00 on the VM) go
    if (TIME.test(s)) return `<time ${s.replace(/\.\d+/, ".f").replace(/[+-](\d\d:\d\d)$/, "±$1").replace(/\d/g, "d")}>`;
    return s.replace(B32, "<base32 seed>");
  };
  const walk = (o, key = null) => {
    if (typeof o === "string") return str(o, key);
    if (Array.isArray(o)) return o.map((x) => walk(x, key === "Password" ? key : null));
    if (o && typeof o === "object") return Object.fromEntries(Object.entries(o).map(([k, v]) => [walk(k, "__key"), walk(v, k)]));
    return o;
  };
  const headers = {};
  for (const h of HEADERS) if (r.headers[h] !== undefined) headers[h] = String(r.headers[h]).replace(/charset=utf-8/i, "charset=utf-8");
  return { status: r.status, headers, body: r.json !== null ? walk(r.json) : r.text === "" ? "" : `<text> ${r.text.slice(0, 2000)}` };
}

/** Every difference between two normalised answers, as JSON paths. */
export function diff(a, b, path = "$", out = []) {
  if (out.length > 40) return out;
  const ta = a === null ? "null" : Array.isArray(a) ? "array" : typeof a;
  const tb = b === null ? "null" : Array.isArray(b) ? "array" : typeof b;
  if (ta !== tb) { out.push(`${path}: ${ta} ${JSON.stringify(a)?.slice(0, 80)} vs ${tb} ${JSON.stringify(b)?.slice(0, 80)}`); return out; }
  if (ta === "array") {
    if (a.length !== b.length) out.push(`${path}: ${a.length} items vs ${b.length}`);
    for (let i = 0; i < Math.min(a.length, b.length); i++) diff(a[i], b[i], `${path}[${i}]`, out);
  } else if (ta === "object") {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.join("\u0000") !== kb.join("\u0000")) out.push(`${path}: keys [${ka.join(", ")}] vs [${kb.join(", ")}]`);
    for (const k of ka) if (k in b) diff(a[k], b[k], `${path}.${k}`, out);
  } else if (a !== b) out.push(`${path}: ${JSON.stringify(a)?.slice(0, 120)} vs ${JSON.stringify(b)?.slice(0, 120)}`);
  return out;
}

/** Runs the request list against one server; returns {name: normalised answer} and the raw timings. */
export async function run(base, users, { expire, restore, log = () => {} } = {}) {
  let names = await bind(base, users.admin);
  const tokens = {};
  for (const role of ["admin", "writer", "reader"]) tokens[role] = await token(base, users[role].user, users[role].password);
  const out = {}, ms = {};
  try {
    for (const q of list) {
      if (q.step === "expire-tokens") { tokens.expired = tokens.reader; await expire(); continue; }
      const path = fill(q.path, names, users);
      const auth = q.as === "none" ? undefined : q.as === "garbage" ? "bm90LWEtdG9rZW4tbWFkZS11cC1mb3ItdGhlLWNvbnRyYWN0" : tokens[q.as];
      const r = await request(base, q.method, path, { token: auth, body: q.body ? fillDeep(q.body, names, users) : undefined, form: q.form ? fillDeep(q.form, names, users) : undefined });
      if (q.rebind && r.status === 200 && typeof r.json === "string") names = { ...names, entries: { ...names.entries, [q.rebind]: r.json } };
      out[q.name] = normalise(r, names, { secret: /\/password$/i.test(q.path) && r.status === 200 });
      out[q.name].request = { as: q.as, method: q.method, path: q.path };
      ms[q.name] = Math.round(r.ms);
      log(`${q.name}: ${r.status} ${Math.round(r.ms)} ms`);
    }
  } finally {
    if (restore) await restore();
  }
  return { out, ms };
}

// ---------------------------------------------------------------- the stand-in, spawned

/** A stand-in on a free port with plan(20) and these users; `extra` options come first, so they win. */
export function spawnStandin(users, extra = []) {
  return new Promise((resolve, reject) => {
    const userArgs = ["admin", "writer", "reader"].flatMap((role) => ["--user", `${users[role].user}:${users[role].password}:${role}`]);
    const p = spawn(process.execPath, [`${HERE}standin.mjs`, ...extra, "--listen", "0", "--entries", "20", "--control", ...userArgs], { stdio: ["ignore", "pipe", "inherit"] });
    let buf = "";
    p.stdout.on("data", (c) => {
      buf += c;
      const m = buf.match(/listening on (http:\/\/127\.0\.0\.1:\d+)/);
      if (m) resolve({ base: m[1], stop: () => p.kill() });
    });
    p.on("exit", (code) => reject(new Error(`stand-in exited ${code}`)));
  });
}

const clockAdvance = (base) => () => request(base, "POST", "/__standin/clock", { body: { advanceSeconds: 400 * 24 * 3600 } });

async function main() {
  const mode = process.argv[2];
  const log = (s) => console.log(s);
  if (mode === "record") {
    const u = realUsers();
    const sh = (n) => async () => { if (process.env[n]) execSync(process.env[n], { stdio: "inherit" }); };
    const { out, ms } = await run(u.url, u, { expire: sh("KNOBAS_PLEASANT_EXPIRE_CMD"), restore: sh("KNOBAS_PLEASANT_RESTORE_CMD"), log });
    rmSync(RECORDED, { recursive: true, force: true });
    mkdirSync(RECORDED, { recursive: true });
    for (const [name, v] of Object.entries(out)) writeFileSync(`${RECORDED}/${name}.json`, `${JSON.stringify({ name, ...v }, null, 1)}\n`);
    writeFileSync(`${HERE}recorded-timings.json`, `${JSON.stringify({ note: "ms per request on the real server when recorded (WSL VM, 20 entries); not compared", ms }, null, 1)}\n`);
    log(`recorded ${Object.keys(out).length} answers in ${RECORDED}`);
    return;
  }
  let want;
  let users;
  if (mode === "--live") {
    users = realUsers();
    const sh = (n) => async () => { if (process.env[n]) execSync(process.env[n], { stdio: "inherit" }); };
    want = (await run(users.url, users, { expire: sh("KNOBAS_PLEASANT_EXPIRE_CMD"), restore: sh("KNOBAS_PLEASANT_RESTORE_CMD") })).out;
  } else if (mode === "--recorded") {
    users = { admin: { user: "admin", password: "knobas-dev" }, writer: { user: "tomasz.wojcik", password: "knobas-dev" }, reader: { user: "mara.lindqvist", password: "knobas-dev" } };
    want = Object.fromEntries(readdirSync(RECORDED).filter((f) => f.endsWith(".json")).map((f) => {
      const { name, ...v } = JSON.parse(readFileSync(`${RECORDED}/${f}`, "utf8"));
      return [name, v];
    }));
  } else {
    console.error("usage: contract.mjs record | --live | --recorded");
    process.exit(2);
  }
  const s = await spawnStandin(users);
  let failed = 0;
  try {
    const got = (await run(s.base, users, { expire: clockAdvance(s.base) })).out;
    for (const q of list.filter((x) => x.name)) {
      const d = want[q.name] ? diff(want[q.name], got[q.name]) : [`no recording for ${q.name}`];
      if (d.length) { failed++; console.log(`FAIL ${q.name}\n  ${d.join("\n  ")}`); } else console.log(`ok   ${q.name}`);
    }
  } finally {
    s.stop();
  }
  console.log(failed ? `contract: ${failed} of ${list.filter((x) => x.name).length} answers differ` : `contract: all ${list.filter((x) => x.name).length} answers match`);
  process.exit(failed ? 1 : 0);
}

if (import.meta.url === `file://${process.argv[1]}`) await main();
