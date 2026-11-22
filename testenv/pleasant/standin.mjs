#!/usr/bin/env node
// A stand-in for Pleasant Password Server's REST API v5, for volume (testenv/pleasant/README.md, ADR-0013's licence
// exception). It serves plan(n) with the shapes, status codes, headers and error bodies the real server (9.5.4)
// answered in `recorded/` (shapes.mjs), and `contract.mjs` fails on any difference. What it serves beyond the
// recordings (2,000 entries, 500+ in a folder) is the same shapes, more of them; how fast the real server would be at
// that size is witnessed by nothing.
//
//   node testenv/pleasant/standin.mjs --listen 8297 [--entries 2000] [--delay <ms>] [--log] [--control]
//                                     [--user name:password:admin|writer|reader ...]
//
// Users default to admin, tomasz.wojcik (may write) and mara.lindqvist (reads only), password `knobas-dev` each, as
// on the real server: admin holds Root, the writer has Full and the reader Read-only on Tidewater.
// `--control` adds POST /__standin/clock {"advanceSeconds": n}, which moves the stand-in's clock (token expiry).
// Nothing is written anywhere; the estate lives in memory and resets on restart.
import http from "node:http";
import { createHash, randomBytes } from "node:crypto";
import { plan } from "./plan.mjs";
import { SHAPES } from "./shapes.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const all = (name) => args.flatMap((a, i) => (a === name ? [args[i + 1]] : []));
const listen = Number(opt("--listen", "8297"));
const n = Number(opt("--entries", "2000"));
const delay = Number(opt("--delay", "0"));
const logging = args.includes("--log");
const control = args.includes("--control");
const users = new Map((all("--user").length ? all("--user") : ["admin:knobas-dev:admin", "tomasz.wojcik:knobas-dev:writer", "mara.lindqvist:knobas-dev:reader"])
  .map((s) => { const [user, password, role] = s.split(":"); return [user.toLowerCase(), { user, password, role }]; }));

let skew = 0; // seconds the clock has been moved by --control
const now = () => Date.now() + skew * 1000;

// ---------------------------------------------------------------- the estate

const uuid = (key) => {
  const h = createHash("sha256").update(`pleasant-standin:${key}`).digest("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-4${h.slice(13, 16)}-${"89ab"[parseInt(h[16], 16) % 4]}${h.slice(17, 20)}-${h.slice(20, 32)}`;
};
/** Pleasant writes local time with its offset, whole seconds. */
const stamp = (ms) => { const d = new Date(ms + 3600_000); return `${d.toISOString().slice(0, 19)}+01:00`; };
const created = stamp(Date.parse("2026-11-22T04:10:22Z"));
const p = plan(n);
const rootId = uuid("root");
const folders = new Map(); // id -> {id, name, parentId, children: [ids], entries: [ids], commentRequired, path}
const entries = new Map(); // id -> {id, groupId, e (plan entry), files, modified}
folders.set(rootId, { id: rootId, name: "Root", parentId: "00000000-0000-0000-0000-000000000000", children: [], entries: [], commentRequired: false, path: [], root: true });
const byPath = new Map([["", rootId]]);
for (const f of p.folders) {
  const id = uuid(`folder:${f.key}`);
  const parentId = byPath.get(f.path.slice(0, -1).join("/"));
  folders.set(id, { id, name: f.path.at(-1), parentId, children: [], entries: [], commentRequired: f.commentRequired, path: f.path });
  folders.get(parentId).children.push(id);
  byPath.set(f.key, id);
}
const addEntry = (e, id = uuid(`entry:${e.key}`)) => {
  const groupId = byPath.get(e.path.join("/"));
  entries.set(id, { id, groupId, e: { ...e, fields: { ...e.fields } }, modified: created,
    files: e.files.map((f, i) => ({ id: uuid(`att:${e.key}:${i}`), name: f.name, data: Buffer.from(f.content) })) });
  folders.get(groupId).entries.push(id);
  return id;
};
for (const e of p.entries) addEntry(e);
const entryCount = () => entries.size;

/** Whether a password read in this folder (or below it) needs a usage comment: the rule is inherited. */
const commentRule = (folderId) => {
  for (let f = folders.get(folderId); f; f = folders.get(f.parentId)) if (f.commentRequired) return true;
  return false;
};
const fullPath = (folderId) => `Root/${folders.get(folderId).path.map((s) => `${s}/`).join("")}`;

// ---------------------------------------------------------------- the shapes

const role = (u) => users.get(u.toLowerCase())?.role;
const canWrite = (u) => ["admin", "writer"].includes(role(u));

function entryJson(id, user) {
  const { e, groupId, files, modified } = entries.get(id);
  return SHAPES.entry({
    id, groupId, name: e.name, username: e.user, url: e.url, notes: e.notes, fields: e.fields, totp: e.totp,
    attachments: files.map((a) => SHAPES.attachmentMeta({ entryId: id, id: a.id, name: a.name, size: a.data.length })),
    created, modified, expires: e.expires ? stamp(Date.parse(`${e.expires}T00:00:00Z`) - 3600_000) : null,
    canModify: canWrite(user), comment: commentRule(groupId),
  });
}

/** `wholeTree`: GET folders. Recorded quirk: folders carry their comment rule only there; under GET folders/{id} every
 *  folder says false, while the entries inside still carry theirs. */
function folderJson(id, user, depth, wholeTree) {
  const f = folders.get(id);
  return SHAPES.folder({
    id, name: f.name, parentId: f.parentId, created, modified: created, root: f.root,
    canModify: canWrite(user), comment: wholeTree && commentRule(id),
    children: depth > 0 ? f.children.map((c) => folderJson(c, user, depth - 1, wholeTree)) : [],
    credentials: f.entries.map((eid) => entryJson(eid, user)),
  });
}

// ---------------------------------------------------------------- HTTP

const tokens = new Map(); // token -> {user, expires (ms)}

const send = (res, status, body, headers = {}) => {
  const text = body === undefined ? "" : JSON.stringify(body);
  const h = { ...(text ? { "content-type": SHAPES.JSON_API } : {}), ...headers, "content-length": Buffer.byteLength(text) };
  res.writeHead(status, h);
  res.end(text);
};

const readBody = (req) => new Promise((resolve) => {
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
});

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

async function handle(req, res) {
  const url = new URL(req.url, "http://standin");
  const path = url.pathname.replace(/\/+$/, "");
  const raw = await readBody(req);
  const json = () => { try { return raw ? JSON.parse(raw) : {}; } catch { return {}; } };

  if (control && req.method === "POST" && path === "/__standin/clock") { skew += Number(json().advanceSeconds ?? 0); return send(res, 204); }

  if (req.method === "POST" && path.toLowerCase() === "/oauth2/token") {
    const form = new URLSearchParams(raw);
    if (form.get("grant_type") !== "password") return SHAPES.tokenRefused(send, res, "unsupported_grant_type");
    const u = users.get((form.get("username") ?? "").toLowerCase());
    if (!u || u.password !== form.get("password")) return SHAPES.tokenRefused(send, res, "invalid_grant");
    const tok = randomBytes(96).toString("base64url");
    tokens.set(tok, { user: u.user, expires: now() + SHAPES.tokenLifeSeconds * 1000 });
    return send(res, 200, SHAPES.token(tok), { "content-type": SHAPES.tokenType });
  }

  const m = path.match(/^\/api\/v[56]\/rest\/(.*)$/i);
  if (!m) return SHAPES.notFound(send, res);
  const auth = (req.headers.authorization ?? "").match(/^Bearer (.+)$/);
  const t = auth && tokens.get(auth[1]);
  if (!t || t.expires <= now()) return SHAPES.unauthorized(send, res);
  const user = t.user;
  const seg = m[1].split("/");
  const head = seg[0].toLowerCase();
  const kind = { entries: "entries", credential: "entries", folders: "folders", credentialgroup: "folders" }[head];

  if (head === "getserverinfo" && req.method === "GET") return send(res, 200, SHAPES.serverInfo());
  if (head === "search" && req.method === "POST") {
    const q = String(json().Search ?? "").toLowerCase();
    const hits = [...entries.values()].filter((x) => x.e.name.toLowerCase().includes(q));
    const groups = [...folders.values()].filter((f) => !f.root && f.name.toLowerCase().includes(q));
    return send(res, 200, {
      Credentials: hits.map((x) => SHAPES.searchEntry({ id: x.id, name: x.e.name, username: x.e.user, url: x.e.url, notes: x.e.notes, groupId: x.groupId, path: fullPath(x.groupId) })),
      Groups: groups.map((f) => SHAPES.searchGroup({ id: f.id, name: f.name, path: fullPath(f.id) })),
    });
  }
  if ((head === "canuseraddentry" || head === "canuseraddfolder") && req.method === "GET") {
    if (!GUID.test(seg[1] ?? "")) return SHAPES.notFound(send, res);
    if (!folders.has(seg[1])) return SHAPES.noFolder(send, res);
    return send(res, 200, canWrite(user));
  }
  if (head === "iscommentrequired" && req.method === "POST") {
    if (!GUID.test(seg[1] ?? "")) return SHAPES.notFound(send, res);
    // the real server answers false for "ViewPassword" even where a comment is required (recorded)
    return send(res, 200, false);
  }

  if (kind === "folders" && req.method === "GET") {
    if (seg.length === 1) return send(res, 200, folderJson(rootId, user, Infinity, true));
    if (seg[1].toLowerCase() === "root") return send(res, 200, rootId);
    if (seg.length === 2) {
      if (!GUID.test(seg[1])) return SHAPES.notFound(send, res);
      if (!folders.has(seg[1])) return SHAPES.noFolder(send, res);
      const level = url.searchParams.has("recurseLevel") ? Number(url.searchParams.get("recurseLevel")) : 1;
      return send(res, 200, folderJson(seg[1], user, level, false));
    }
  }

  if (kind === "entries") {
    if (req.method === "POST" && seg.length === 1) {
      if (entryCount() >= SHAPES.freeLimit(n)) return SHAPES.overLimit(send, res);
      const b = json();
      if (!folders.has(b.GroupId)) return SHAPES.noFolder(send, res);
      const id = uuid(`made:${randomBytes(8).toString("hex")}`);
      addEntry({ key: id, path: folders.get(b.GroupId).path, name: b.Name ?? "", user: b.Username ?? "", password: b.Password ?? "", url: b.Url ?? "",
        notes: b.Notes ?? "", fields: b.CustomUserFields ?? {}, files: [], expires: b.Expires ? String(b.Expires).slice(0, 10) : null }, id);
      return send(res, 200, id);
    }
    const id = seg[1];
    if (!id || !GUID.test(id)) return SHAPES.notFound(send, res);
    const x = entries.get(id);
    if (!x) return SHAPES.noEntry(send, res);
    const sub = (seg[2] ?? "").toLowerCase();
    const touch = () => { x.modified = stamp(now()); };
    if (sub === "" && req.method === "GET") return send(res, 200, entryJson(id, user));
    if (sub === "" && ["PATCH", "PUT", "DELETE"].includes(req.method) && !canWrite(user)) return SHAPES.mayNotUpdate(send, res, user, x.e.name);
    if (sub === "" && req.method === "PATCH") {
      const b = json();
      for (const [k, prop] of [["Name", "name"], ["Username", "user"], ["Password", "password"], ["Url", "url"], ["Notes", "notes"]]) if (k in b && b[k] !== null) x.e[prop] = b[k];
      if (b.CustomUserFields) x.e.fields = b.CustomUserFields;
      if ("TOTPSecret" in b) x.e.totp = b.TOTPSecret ? { issuer: b.TOTPIssuer ?? null, secret: b.TOTPSecret, digits: b.TOTPDigits ?? 6, period: b.TOTPPeriod ?? 30 } : null;
      touch();
      return send(res, 204);
    }
    if (sub === "" && req.method === "PUT") {
      const it = json().Item ?? {};
      for (const [k, prop] of [["Name", "name"], ["Username", "user"], ["Url", "url"], ["Notes", "notes"]]) if (k in it) x.e[prop] = it[k] ?? "";
      if (it.Password !== null && it.Password !== undefined) x.e.password = it.Password;
      if (it.CustomUserFields) x.e.fields = it.CustomUserFields;
      touch();
      return send(res, 204);
    }
    if (sub === "" && req.method === "DELETE") {
      entries.delete(id);
      const f = folders.get(x.groupId);
      f.entries = f.entries.filter((e) => e !== id);
      return send(res, 204);
    }
    if (sub === "password" && (req.method === "GET" || req.method === "POST")) {
      const comment = req.method === "POST" ? String(json().Comment ?? "").trim() : "";
      if (commentRule(x.groupId) && !comment) return SHAPES.commentRequired(send, res);
      return send(res, 200, x.e.password);
    }
    if (sub === "attachments" && req.method === "GET") {
      const att = (a) => SHAPES.attachment({ entryId: id, id: a.id, name: a.name, data: a.data.toString("base64"), size: a.data.length });
      if (seg[3]) {
        const a = x.files.find((f) => f.id === seg[3]);
        return a ? send(res, 200, att(a)) : SHAPES.notFound(send, res);
      }
      return send(res, 200, x.files.map(att));
    }
  }
  return SHAPES.notFound(send, res);
}

const server = http.createServer((req, res) => {
  const t0 = performance.now();
  res.on("finish", () => { if (logging) console.log(`${req.method} ${req.url} ${res.statusCode} ${Math.round(performance.now() - t0)} ms`); });
  const go = () => handle(req, res).catch((e) => { console.error(e); if (!res.headersSent) send(res, 500, { Message: "stand-in error" }); });
  if (delay > 0) setTimeout(go, delay); else go();
});
server.listen(listen, "127.0.0.1", () => {
  console.log(`pleasant stand-in listening on http://127.0.0.1:${server.address().port} (${entries.size} entries, ${folders.size} folders)`);
});
