// The Pleasant estate, deterministic (testenv/pleasant/README.md). `plan(20)` is what `seed.mjs` writes to the real
// server (the free edition holds 20 passwords); `plan(2000)` is what `standin.mjs` serves for volume. The first 20 of
// `plan(2000)` are `plan(20)`, so the stand-in at 2,000 is the real server's estate plus generated entries.
//
// Every entry is made up: Tidewater is the mockups' fictional company, the passwords are invented and the TOTP seeds
// are random Base32 from a fixed seed. Nothing here is a secret.
import { readFileSync } from "node:fs";

const tidewater = JSON.parse(readFileSync(new URL("./tidewater.json", import.meta.url), "utf8"));

/** A small seeded PRNG (mulberry32), so every run plans the same estate. */
export function rng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    next,
    int: (lo, hi) => lo + Math.floor(next() * (hi - lo + 1)),
    pick: (xs) => xs[Math.floor(next() * xs.length)],
    sample: (xs, n) => {
      const c = [...xs];
      const out = [];
      while (out.length < n && c.length) out.push(c.splice(Math.floor(next() * c.length), 1)[0]);
      return out;
    },
  };
}

const B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const seed32 = (r, n = 32) => Array.from({ length: n }, () => B32[Math.floor(r.next() * 32)]).join("");

/** The four places a KeePass TOTP plugin keeps its key (the design brief's table). */
export const TOTP_FORMATS = ["builtin", "traytotp", "keeotp", "xc"];

export function totpFields(format, r, label) {
  const s = seed32(r);
  switch (format) {
    case "builtin": return { "TimeOtp-Secret-Base32": s };
    case "traytotp": return { "TOTP Seed": s, "TOTP Settings": "30;6" };
    case "keeotp": return { otp: `key=${s}&step=30&size=6` };
    case "xc": return { otp: `otpauth://totp/${encodeURIComponent(`Tidewater:${label}`)}?secret=${s}&period=30&digits=6&issuer=Tidewater` };
    default: throw new Error(format);
  }
}

/** Folders whose passwords need a usage comment to view (set on the real server by `seed.mjs`). */
export const COMMENT_FOLDERS = ["Tidewater/Payments/Prod", "Tidewater/Customers/Tenants/EU"];

/** Folders that block access inheritance, where the reader holds only "Request only" (names, contents, Request
 *  Access; no password, no TOTP). Set on the real server through the web client (README, "Setting up a fresh VM"). */
export const RESTRICTED_FOLDERS = ["Tidewater/Payments/Prod/PSP"];

/** The protected custom field (KeePass's protected string); the real server's answer for it is in the recordings. */
export const PROTECTED = "Recovery PIN";

const entry = (o) => ({ user: "", url: "", notes: "", fields: {}, files: [], expires: null, approver: null, totp: null, ...o });

/** Pleasant 9's own TOTP (TOTPSecret and friends on the entry), as the web client sets it. */
export const nativeTotp = (r, issuer) => ({ issuer, secret: seed32(r), digits: 6, period: 30 });

function fromTidewater(r) {
  let t = 0;
  const out = [];
  for (const e of tidewater.entries) {
    const fields = {};
    for (const [k, v] of e.fields ?? []) fields[k] = v;
    if (e.totp) fields && Object.assign(fields, totpFields(TOTP_FORMATS[t++ % 4], r, e.title));
    out.push(entry({
      key: e.id, path: e.path, name: e.title, user: e.user ?? "", password: e.pw, url: e.url ?? "", notes: e.notes ?? "",
      fields, expires: e.expires ?? null, approver: e.approver ?? null,
      files: (e.files ?? []).map((f) => ({ name: f.name, content: tidewater.files[f.name] ?? `Made-up content of ${f.name}\n` })),
    }));
  }
  // The PSP dashboard also carries Pleasant's own TOTP (set on the real server with PATCH, recorded 2026-11-22).
  const dash = out.find((e) => e.key === "pl-psp-dash");
  if (dash) dash.totp = { issuer: "Tidewater PSP", secret: seed32(rng(99)), digits: 6, period: 30 };
  return out;
}

const WORDS = "amber birch cobalt delta ember fjord granite harbour iris juniper kelp lumen marrow nimbus onyx pylon quartz reef slate tundra umber vector willow xenon yarrow zephyr".split(" ");
const SERVICES = "ledger payout billing invoices search auth gateway notify reports export import scheduler mailer audit catalog pricing quotes orders refunds risk kyc fx treasury".split(" ");

function generated(r, deepFirst) {
  const out = [];
  const T = ["Tidewater"];
  const pw = () => `${r.pick(WORDS)[0].toUpperCase()}${r.pick(WORDS).slice(1)}-${r.int(10, 99)}!${r.pick(WORDS)}#${seed32(r, 4)}`;
  const add = (path, name, user = "", url = "", extra = {}) =>
    out.push(entry({ key: `g${out.length}`, path, name, user, password: pw(), url, ...extra }));

  // A deep tree first (its first two entries are in plan(20)): Tidewater/Engineering/Teams/<team>/Services/<svc>/<env>/<kind>,
  // 8 levels, 9 under Pleasant's Root.
  for (const team of ["Backend", "Data", "Mobile", "Web"]) {
    for (const svc of r.sample(SERVICES, 4)) {
      for (const env of ["Prod", "Stage", "Dev"]) {
        const base = [...T, "Engineering", "Teams", team, "Services", svc, env];
        add([...base, "Databases"], `${svc}-${env.toLowerCase()} database user`, `${svc}_app`, `postgres://${svc}-${env.toLowerCase()}-db.tidewater.internal:5432/${svc}`);
        add([...base, "Keys"], `${svc}-${env.toLowerCase()} API signing key`, "", "",
          { fields: { "Key id": `${svc}-${env.toLowerCase()}-${r.int(2024, 2026)}`, Algorithm: r.pick(["Ed25519", "RS256", "ES256"]) } });
        if (env === "Prod") add(base, `${svc} on-call dashboard`, `${svc}-oncall`, `https://${svc}.tidewater.internal/admin`);
      }
    }
  }
  if (deepFirst) return out;
  // Three big flat folders (500+ each): the slow listing.
  for (let i = 1; i <= 610; i++) add([...T, "Customers", "Tenants", "EU"], `tenant-eu-${String(i).padStart(4, "0")} admin`, `admin@tenant-eu-${String(i).padStart(4, "0")}.example`, `https://tenant-eu-${String(i).padStart(4, "0")}.app.tidewater.example`);
  for (let i = 1; i <= 530; i++) add([...T, "Platform", "Fleet", "Hosts", "Linux"], `vm-app-${String(i).padStart(3, "0")} root`, "root", `ssh://10.40.${Math.floor(i / 250)}.${(i % 250) + 1}`);
  for (let i = 1; i <= 515; i++) add([...T, "Platform", "Fleet", "Hosts", "Windows"], `win-${String(i).padStart(3, "0")} local admin`, "Administrator", `rdp://10.41.${Math.floor(i / 250)}.${(i % 250) + 1}`);
  for (const v of ["Atlassian", "Hetzner", "AWS", "Cloudflare", "Datadog", "PagerDuty", "Sentry", "GitHub", "Slack", "Zoom", "Okta"]) {
    const n = r.int(3, 8);
    for (let k = 1; k <= n; k++) add([...T, "Vendors", v, r.pick(["Admin", "Billing", "API"])], `${v} ${r.pick(["admin", "billing", "api", "readonly", "sso"])} ${k}`, `${v.toLowerCase()}-${k}@tidewater.example`, `https://${v.toLowerCase()}.example`);
  }
  for (let i = 1; i <= 70; i++) add([...T, "Finance", "Banking", r.pick(["Accounts", "Cards", "Portals"])], `Bank portal ${i}`, `treasury${i}`, `https://bank${i % 7}.example`, { fields: { "Account no": `DE${r.int(10 ** 8, 10 ** 9 - 1)}${r.int(10 ** 9, 10 ** 10 - 1)}` } });
  for (const site of ["Hamburg", "Lisbon", "Gdansk"]) {
    for (let i = 1; i <= 25; i++) add([...T, "Office", "Sites", site, r.pick(["Building", "Printers", "Meeting rooms"])], `${site} ${r.pick(["door panel", "printer", "room display", "Wi-Fi"])} ${i}`, r.pick(["", "admin", "facility"]));
  }
  // The extras, spread deterministically over the generated entries.
  out.forEach((e, n) => {
    if (n % 13 === 5) Object.assign(e.fields, totpFields(TOTP_FORMATS[Math.floor(n / 13) % 4], r, e.name));
    if (n % 17 === 9) e.totp = nativeTotp(r, "Tidewater");
    if (n % 9 === 2 && !e.fields["Secret path"]) e.fields["Secret path"] = `kv/${e.path.slice(1).join("/").toLowerCase()}`;
    if (n % 37 === 11) e.fields[PROTECTED] = `${r.int(1000, 9999)}-${r.int(1000, 9999)}`;
    if (n % 11 === 3) e.notes = `Rotated by ${r.pick(["Priya Nair", "Tomasz Wójcik", "Jonas Becker", "Lena Hoffmann"])}. Ask in #ops before changing.`;
    if (n % 53 === 7) e.files = [{ name: `${e.name.slice(0, 20).replaceAll(" ", "-")}.txt`, content: `Made-up attachment for ${e.name}\n${"x".repeat(r.int(100, 4000))}` }];
    if (n % 71 === 13) e.expires = `2027-${String(r.int(1, 12)).padStart(2, "0")}-${String(r.int(1, 28)).padStart(2, "0")}`;
  });
  return out;
}

/** The 18 clickdummy entries the real server holds: every case the adapter meets (each TOTP format, custom fields with
 *  a protected one, attachments, an access-request entry, expiry dates, a comment-required folder). */
export const REAL_KEYS = ["pl-payout-db", "pl-ledger-db", "pl-pg-prod", "pl-ledger-key", "pl-webhook", "pl-psp-live", "pl-psp-dash",
  "pl-pg-stage", "pl-teamcity", "pl-agent-1", "pl-grafana", "pl-ops-mail", "pl-wg", "pl-fr-prod", "pl-ebics",
  "pl-sepa-portal", "pl-alarm", "pl-wifi"];

/** The estate with `n` entries: 20 (the real server's: 18 clickdummy entries and two 8 levels deep) or more. */
export function plan(n = 20) {
  const tw = Object.fromEntries(fromTidewater(rng(2026)).map((e) => [e.key, e]));
  const real = REAL_KEYS.map((k) => tw[k]);
  const deep = generated(rng(7), true).slice(0, 2).map((e, i) => ({ ...e, key: `deep${i}` }));
  const first = [...real, ...deep];
  // The real 18 carry one TOTP format each among them; make sure it stays so (the seed of the 4 TOTP entries).
  if (n <= 20) return estate(first.slice(0, n));
  const rest = Object.values(tw).filter((e) => !REAL_KEYS.includes(e.key));
  const more = generated(rng(2026), false).slice(2);
  return estate([...first, ...rest, ...more].slice(0, n));
}

/** Entries plus every folder on their paths, parents first. */
function estate(entries) {
  const seen = new Set();
  const folders = [];
  for (const e of entries) {
    for (let i = 1; i <= e.path.length; i++) {
      const key = e.path.slice(0, i).join("/");
      if (!seen.has(key)) { seen.add(key); folders.push({ key, path: e.path.slice(0, i), commentRequired: COMMENT_FOLDERS.includes(key), restricted: RESTRICTED_FOLDERS.includes(key) }); }
    }
  }
  return { entries, folders };
}

// `node plan.mjs 2000` prints a summary.
if (import.meta.url === `file://${process.argv[1]}`) {
  const p = plan(Number(process.argv[2] ?? 20));
  const counts = {};
  for (const e of p.entries) counts[e.path.join("/")] = (counts[e.path.join("/")] ?? 0) + 1;
  const totp = p.entries.filter((e) => e.totp || Object.keys(e.fields).some((k) => ["otp", "TOTP Seed", "TimeOtp-Secret-Base32"].includes(k))).length;
  console.log(`${p.entries.length} entries, ${p.folders.length} folders, deepest ${Math.max(...p.folders.map((f) => f.path.length))} levels, ` +
    `${totp} with TOTP fields, ${p.entries.filter((e) => e.files.length).length} with attachments, ` +
    `${p.entries.filter((e) => e.fields[PROTECTED]).length} with a protected field; 500+: ` +
    Object.entries(counts).filter(([, c]) => c >= 500).map(([k, c]) => `${k} (${c})`).join(", "));
}
