/**
 * `?fake-ipc`: the Passwords page (ADR-0056), over the clickdummy's Tidewater entries (`passwords-tidewater.json`,
 * made up: every password, key and code here is invented).
 *
 * Switches, for the press trips, `just moves` and screenshots:
 * - `?pw-none`: no password source; `?pw-unsynced`: one source that has never synced.
 * - `?pw-offline`: the source did not answer; codes still come.
 * - `?pw-comment`: every folder asks for a usage comment.
 * - `?pw-slow=<ms>`: a secret read takes that long.
 * - `?pw-scale=<n>`: about `n` entries (the mock Pleasant's 2,000: eight levels, folders of 610, 530 and 515).
 */
import TIDEWATER from "./passwords-tidewater.json";
import type { ClipState, CodeAnswer, Held, PwEntry, PwFolder, PwRow, ReadAnswer } from "../../../ipc/passwords";
import type { Handlers } from "../fake-tauri";

interface Raw {
  id: string;
  title: string;
  user: string;
  url: string;
  notes: string;
  fields: (string | boolean)[][];
  totp: boolean;
  expires: string | null;
  approver: string | null;
  readonly: boolean;
  by: string;
  path: string[];
  pw: string;
  changed: string;
  /** Attachments, as the clickdummy names them: `size` in its words ("3.2 KB"). */
  files?: { name: string; size: string; kind: string }[];
}

/** "3.2 KB" as bytes. */
function bytesOf(size: string): number {
  const [n, unit] = size.split(" ");
  const scale = unit === "MB" ? 1024 * 1024 : unit === "KB" ? 1024 : 1;
  return Math.round(Number(n) * scale);
}

/** What a text attachment holds in the fake: made up, and saying so. */
function fakeText(name: string): string {
  return `# ${name}\n# Made up for the dev fake: no real key, no real host.\n[Interface]\nAddress = 10.99.0.1/24\nListenPort = 51820\n`;
}

const SOURCE = "team";
const SYNCED_AT = "2026-11-22T10:22:00Z";
const COMMENT_FOLDERS = ["Tidewater/Payments/Prod"];

/** A deterministic six-digit code for an entry and a 30-second slot. */
function code(id: string, slot: number): string {
  let h = 2166136261;
  for (const c of `${id}:${slot}`) h = Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0;
  return String(h % 1_000_000).padStart(6, "0");
}

/** The entries: Tidewater's, and generated ones up to `scale`. */
function corpus(scale: number): Raw[] {
  const base = (TIDEWATER as { entries: Raw[] }).entries;
  if (scale <= base.length) return base;
  const out = [...base];
  const words = "amber birch cobalt delta ember fjord granite harbour iris juniper kelp lumen marrow nimbus onyx pylon".split(" ");
  const services = "ledger payout billing invoices search auth gateway notify reports export import scheduler mailer audit".split(" ");
  // Three big folders first, then a deep tree of small ones.
  const big: [string[], number][] = [
    [["Tidewater", "Platform", "CI", "Agents", "Linux"], 610],
    [["Tidewater", "Platform", "Hosts", "Prod"], 530],
    [["Tidewater", "Integrations", "Partners", "Keys"], 515],
  ];
  let n = 0;
  const add = (path: string[]) => {
    const w = words[n % words.length]!;
    const s = services[Math.floor(n / words.length) % services.length]!;
    out.push({
      id: `gen-${n}`, title: `${s}-${w}-${n}`, user: `${s}_${w}`, url: n % 3 === 0 ? `https://tidewater.example/${s}` : `${s}-${n % 40}.tidewater.test`,
      notes: "", fields: [], totp: n % 13 === 0, expires: null, approver: null, readonly: n % 9 === 0, by: "Priya Nair",
      path, pw: `${w}-${n}!x`, changed: `2026-${String((n % 11) + 1).padStart(2, "0")}-${String((n % 27) + 1).padStart(2, "0")}`,
    });
    n++;
  };
  for (const [path, count] of big) for (let i = 0; i < count && out.length < scale; i++) add(path);
  for (let f = 0; out.length < scale; f++) {
    const depth = 2 + (f % 7);
    const path = ["Tidewater", ...Array.from({ length: depth }, (_, d) => `${words[(f + d) % words.length]!}-${(f >> d) % 4}`)];
    for (let i = 0; i < 6 && out.length < scale; i++) add(path);
  }
  return out;
}

/** The read the page holds, with the shared root folder left out as `passwords::reads` leaves it. */
function held(entries: Raw[], params: URLSearchParams): Held {
  const folders = new Map<string, PwFolder>();
  const folderId = (path: string[]) => `${SOURCE}:f/${path.join("/")}`;
  for (const e of entries) {
    for (let d = 2; d <= e.path.length; d++) {
      const path = e.path.slice(0, d);
      const id = folderId(path);
      if (folders.has(id)) continue;
      folders.set(id, {
        id, source: 0, parent: d > 2 ? folderId(path.slice(0, -1)) : null, name: path[d - 1]!,
        comment_read: params.has("pw-comment") || COMMENT_FOLDERS.includes(path.join("/")), can_add: true,
      });
    }
  }
  const under = (path: string[]) => params.has("pw-comment") || COMMENT_FOLDERS.some((c) => path.join("/").startsWith(c));
  const rows: PwRow[] = entries.map((e) => [
    `${SOURCE}:${e.id}`, 0, e.path.length > 1 ? folderId(e.path) : null, e.title, e.user || null, e.url || null, `${e.changed}T09:00:00Z`,
    (e.totp ? 1 : 0) | (e.approver ? 2 : 0) | (under(e.path) ? 4 : 0) | (e.readonly && e.approver ? 8 : 0), e.expires, [],
  ]);
  if (params.has("pw-none")) return { sources: [], folders: [], entries: [], recent: [] };
  const unsynced = params.has("pw-unsynced");
  return {
    sources: [{
      id: SOURCE, name: "Team passwords", account: "mara.lindqvist", by_hand: true,
      synced_at: unsynced ? null : SYNCED_AT, down_at: params.has("pw-offline") ? "2026-11-22T10:40:00Z" : null, never_synced: unsynced,
      offers: { one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: true, access_requests: false, marks_protected_fields: false, writes: ["create_entry", "edit_entry", "set_password"] },
      skipped_root: `${SOURCE}:f/Tidewater`, adapter_name: "Pleasant Password Server",
    }],
    folders: unsynced ? [] : [...folders.values()],
    entries: unsynced ? [] : rows,
    recent: unsynced ? [] : [`${SOURCE}:pl-payout-db`, `${SOURCE}:pl-ci-deploy`].filter((id) => rows.some((r) => r[0] === id)),
  };
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const scale = Number(params.get("pw-scale") ?? 0);
  const entries = corpus(scale);
  const byId = new Map(entries.map((e) => [`${SOURCE}:${e.id}`, e]));
  const view = held(entries, params);
  const slow = Number(params.get("pw-slow") ?? 400);
  const offline = params.has("pw-offline");
  let clip: ClipState = { what: null, entity_id: null, until: null };
  const wait = () => new Promise((resolve) => setTimeout(resolve, slow));
  const until = () => new Date(Date.now() + 30_000).toISOString();
  const commentOf = (id: string) => (view.entries.find((r) => r[0] === id)?.[7] ?? 0) & 4;

  async function read(args: Record<string, unknown>, show: boolean): Promise<ReadAnswer> {
    const id = String(args["entityId"]);
    const e = byId.get(id);
    if (!e) return { kind: "not_offered" };
    if (offline) return { kind: "unreachable", message: "unreachable: the server did not answer" };
    await wait();
    if (commentOf(id) && !args["comment"]) return { kind: "comment_required", scope: [] };
    const which = args["which"] as { kind: string; name?: string };
    const value = which.kind === "field" ? String(e.fields.find((f) => f[0] === which.name)?.[1] ?? "") : e.pw;
    const at = new Date().toISOString();
    if (show) return { kind: "shown", value, read_at: at, until: until() };
    clip = { what: which.kind === "field" ? which.name ?? "field" : "password", entity_id: id, until: until() };
    return { kind: "copied", read_at: at, until: clip.until! };
  }

  function codeOf(id: string): CodeAnswer {
    const e = byId.get(id);
    if (!e?.totp) return { kind: "not_offered" };
    if (e.approver) return { kind: "hidden" };
    const now = Date.now();
    return { kind: "code", code: code(id, Math.floor(now / 30_000)), digits: 6, period: 30, left_ms: 30_000 - (now % 30_000) };
  }

  return {
    passwords_held: () => view,
    passwords_entry: (args) => {
      const id = String(args["entityId"]);
      const e = byId.get(id);
      if (!e) return null;
      const row = view.entries.find((r) => r[0] === id)!;
      const entry: PwEntry = {
        id, source: SOURCE, title: e.title, username: e.user || null, address: e.url || null, folder: row[2], notes: e.notes || null,
        fields: e.fields.map((f) => ({ name: String(f[0]), value: f[2] ? null : String(f[1]) })),
        otp: e.totp ? { format: "keepass", digits: 6, period: 30, algorithm: "SHA1" } : null,
        expires: e.expires, changed: `${e.changed}T09:00:00Z`, changed_by: e.by || null,
        comment: { read: (row[7] & 4) !== 0, modify: false },
        approval: e.approver ? { state: "required", approver: e.approver } : { state: "none" },
        attachments: e.files?.length ? { state: "listed", files: e.files.map((f, i) => ({ id: `${e.id}-file-${i}`, name: f.name, size: bytesOf(f.size) })) } : { state: "none" },
        can_read: !(e.readonly && e.approver), can_modify: !e.readonly,
      };
      return entry;
    },
    password_attachment: (args) => {
      if (offline) return { kind: "unreachable", message: "unreachable: the server did not answer" };
      const e = byId.get(String(args["entityId"]));
      const at = Number(String(args["attachmentId"]).split("-file-")[1]);
      const f = e?.files?.[at];
      if (!f) return { kind: "not_offered" };
      const text = /\.(pem|conf|txt)$/.test(f.name) ? fakeText(f.name) : null;
      return { kind: "read", name: f.name, size: bytesOf(f.size), data: btoa(text ?? "PK"), text };
    },
    password_attachment_save: (args) => {
      if (offline) throw new Error("unreachable: the server did not answer");
      const e = byId.get(String(args["entityId"]));
      const f = e?.files?.[Number(String(args["attachmentId"]).split("-file-")[1])];
      return `/home/mara/Downloads/${f?.name ?? "attachment"}`;
    },
    password_show: (args) => read(args, true) as never,
    password_copy: (args) => read(args, false) as never,
    password_hide: () => undefined,
    otp_code: (args) => codeOf(String(args["entityId"])),
    otp_copy: (args) => {
      const answer = codeOf(String(args["entityId"]));
      if (answer.kind === "code") clip = { what: "one-time code", entity_id: String(args["entityId"]), until: until() };
      return answer;
    },
    clipboard_copy_secret: (args) => {
      clip = { what: String(args["what"]), entity_id: null, until: until() };
      return { until: clip.until! };
    },
    clipboard_clear: () => {
      const had = clip.what !== null;
      clip = { what: null, entity_id: null, until: null };
      return had;
    },
    clipboard_forget: () => {
      clip = { what: null, entity_id: null, until: null };
      return undefined;
    },
    clipboard_state: () => clip,
    passwords_write: (args) => {
      if (offline) return { kind: "unreachable", message: "unreachable: the server did not answer" };
      const write = args["write"] as { write: string; entity?: string; source?: string };
      return { kind: "written", entity_id: write.entity ?? `${SOURCE}:pl-payout-db` };
    },
    password_source_facts: () => {
      const codes = entries.filter((e) => e.totp).length;
      return { entries: view.entries.length, folders: view.folders.length, formats: codes > 0 ? [["keepass", codes] as [string, number]] : [] };
    },
  };
}
