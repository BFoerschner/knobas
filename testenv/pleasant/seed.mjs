#!/usr/bin/env node
// Writes plan(20) to a real Pleasant Password Server through REST API v5 (testenv/pleasant/README.md).
// Idempotent against the server itself: a folder or entry already there by name under its parent is kept, not doubled.
//
//   KNOBAS_PLEASANT_URL=https://127.0.0.1:24801 KNOBAS_PLEASANT_USER=admin KNOBAS_PLEASANT_PASSWORD=... node testenv/pleasant/seed.mjs
//
// The free (unregistered) edition holds 20 passwords and 3 users, so the seed writes exactly plan(20).
import { client, token } from "./client.mjs";
import { plan } from "./plan.mjs";

const need = (n) => process.env[n] || (console.error(`error: ${n} is unset`), process.exit(1));
const base = need("KNOBAS_PLEASANT_URL");
const api = client(base, await token(base, need("KNOBAS_PLEASANT_USER"), need("KNOBAS_PLEASANT_PASSWORD")));

const { entries, folders } = plan(20);
const rootId = await api.get("folders/root");
const ids = new Map(); // folder key -> id

/** A folder's direct children and entries, as the server holds them now. */
const children = async (id) => api.get(`folders/${id}?recurseLevel=1`);

const t0 = performance.now();
let madeFolders = 0, madeEntries = 0, attachments = 0;
for (const f of folders) {
  const parentId = f.path.length === 1 ? rootId : ids.get(f.path.slice(0, -1).join("/"));
  const parent = await children(parentId);
  const found = (parent.Children ?? []).find((c) => c.Name === f.path.at(-1));
  if (found) { ids.set(f.key, found.Id); continue; }
  const id = await api.post("folders", { CustomUserFields: {}, CustomApplicationFields: {}, Children: [], Credentials: [], Tags: [],
    Name: f.path.at(-1), ParentId: parentId, Notes: null, Expires: null });
  ids.set(f.key, id);
  madeFolders++;
}
for (const e of entries) {
  const groupId = ids.get(e.path.join("/"));
  const folder = await children(groupId);
  if ((folder.Credentials ?? []).some((c) => c.Name === e.name)) continue;
  const id = await api.post("entries", { CustomUserFields: e.fields, CustomApplicationFields: {}, Tags: [], Name: e.name,
    Username: e.user, Password: e.password, Url: e.url, Notes: e.notes, GroupId: groupId,
    Expires: e.expires ? `${e.expires}T00:00:00` : null });
  madeEntries++;
  if (e.totp) await api.call("PATCH", `entries/${id}`, { TOTPSecret: e.totp.secret, TOTPIssuer: e.totp.issuer, TOTPDigits: e.totp.digits, TOTPPeriod: e.totp.period });
  for (const file of e.files) {
    await api.post(`entries/${id}/attachments`, { CredentialObjectId: id, FileName: file.name, FileData: Buffer.from(file.content).toString("base64") });
    attachments++;
  }
}
console.log(`seeded: ${madeFolders} folders and ${madeEntries} entries made (${attachments} attachments), ` +
  `${folders.length} folders and ${entries.length} entries in the plan, ${((performance.now() - t0) / 1000).toFixed(1)} s`);
console.log(JSON.stringify(Object.fromEntries(ids)));
