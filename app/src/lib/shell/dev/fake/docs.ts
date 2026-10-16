/**
 * `?fake-ipc`: Docs, the reader over declared docs (#859).
 *
 * Answers `docs_tree` and `docs_page`.
 *
 * **Shape from prototype-6's Docs data**: its five spaces and their trees
 * (`tree`), the editors and times of its pages (`pageMeta`), and the bodies
 * it writes out (`BODIES`): *SEPA payout retry design*, the page read last,
 * with its info panel, code, table, warning, list and the *Jira roadmap*
 * macro; *PSP error codes*, *Payments architecture overview*, *On-call
 * handbook* and *Ledger reconciliation runbook*. Every other page gets
 * prototype-6's generic body. Its inline marks become blocks here the way the
 * Confluence projection makes them: `[[KEY]]` is a ticket chip, `{{page:…}}`
 * a page link, `@Name` a mention (Mara's is the reader's), `` `x` `` code and
 * `**x**` bold.
 *
 * **Where this diverges from prototype-6 and the mock adapter:**
 *
 * - The mock adapter emits no pages, so the source is a fixture-only
 *   Confluence (`confluence:` ids); `visit_entity` refuses these ids (it knows
 *   only `CORPUS`), which the reader ignores.
 * - A ticket chip's summary and status come from `CORPUS` where it holds the
 *   ticket (PAY-231, PAY-228, OPS-77), so a chip cannot disagree with the
 *   board; a key `CORPUS` lacks draws as a bare key, as the real read does.
 * - Pages with prototype-6's generic body carry no comments.
 */
import type { Handler } from "../fake-tauri";
import type { DocBlock, DocInline, DocPage, DocsTree, DocSpace } from "../../../ipc/entity";
import { CORPUS } from "./corpus";

const HOST = "http://127.0.0.1:5399/confluence";
const ME = "mara";

const SPACES: Record<string, string> = {
  ENG: "Engineering",
  OPS: "Operations",
  PAY: "Payments team",
  FIN: "Finance",
  ARCH: "Architecture decisions",
};

/** prototype-6's `tree`: each space's top pages and their children. */
const TREE: Record<string, [string, string[]][]> = {
  ENG: [
    ["Payments", ["SEPA payout retry design", "Payments architecture overview", "PSP error codes", "Payout batch lifecycle", "Idempotency for payout API", "Refund flows", "FX rate caching", "Ledger double-entry model"]],
    ["Customs", ["Customs gateway overview", "EDI message formats", "HS code lookup service", "Customs document checklist"]],
    ["Tracking", ["Carrier event model", "DHL integration notes", "Maersk integration notes", "ETA prediction model"]],
    ["Booking", ["Booking API v3 guide", "Rate quote engine", "Invoice generation"]],
    ["Platform", ["Kubernetes onboarding", "Traefik routing guide", "Secrets handling", "CI templates guide", "Observability conventions"]],
    ["Onboarding", ["Engineering onboarding", "Local dev setup", "Code review guidelines"]],
    ["Standup protocols", ["2026-08-21", "2026-08-20", "2026-08-19", "2026-08-18", "2026-08-15", "2026-08-14"]],
  ],
  OPS: [
    ["On-call handbook", []],
    ["Ledger reconciliation runbook", []],
    ["Runbooks", ["Payout worker restart", "Staging DB restore", "Traefik cert renewal", "Kuma alert routing", "Proxmox node patching"]],
    ["Incident reviews", ["2026-08-11 payout-worker OOM", "2026-07-29 tracking cert expired", "2026-07-03 PSP outage"]],
    ["Change calendar", []],
  ],
  PAY: [
    ["Team home", ["Sprint 34 goals", "Sprint 33 retro", "Sprint 32 retro"]],
    ["Manual review queue", []],
    ["Payout SLAs", []],
    ["PSP contacts", []],
    ["Finance requests", ["Payout CSV export spec", "Dashboard latency complaints"]],
  ],
  FIN: [
    ["Payout reconciliation process", []],
    ["Month-end close checklist", []],
    ["Chargeback handling", []],
    ["Fee schedule 2026", []],
  ],
  ARCH: [
    ["ADR index", ["ADR-031 Retry policy for external calls", "ADR-030 Events over polling for carriers", "ADR-029 One Postgres per service"]],
  ],
};

const PEOPLE: Record<string, [string, string]> = {
  ML: ["mara", "Mara Lindqvist"],
  JB: ["jonas", "Jonas Becker"],
  PN: ["priya", "Priya Nair"],
  TW: ["tomasz", "Tomasz Wójcik"],
  LH: ["lena", "Lena Hoffmann"],
};

/** prototype-6's `pageMeta`: who edited a page and when. */
const META: Record<string, [string, string]> = {
  "SEPA payout retry design": ["ML", "today 10:40"],
  "PSP error codes": ["PN", "2026-08-19 09:00"],
  "Payout batch lifecycle": ["PN", "2026-08-14 09:00"],
  "Manual review queue": ["LH", "yesterday 15:10"],
  "On-call handbook": ["TW", "2026-07-02 09:00"],
  "Payments architecture overview": ["PN", "2026-08-11 09:00"],
  "Ledger reconciliation runbook": ["TW", "2026-08-19 09:00"],
};

/** A prototype body block. */
type Proto =
  | { h: string }
  | { p: string }
  | { ul: string[] }
  | { ol: string[] }
  | { code: string; lang?: string }
  | { table: string[][] }
  | { info: string }
  | { warn: string }
  | { note: string }
  | { macro: string };

interface Body {
  labels: string[];
  version: number;
  blocks: Proto[];
  comments: [string, string, string][];
}

/** prototype-6's `BODIES`, the pages it writes out. */
const BODIES: Record<string, Body> = {
  "SEPA payout retry design": {
    labels: ["payments", "sepa", "design"],
    version: 14,
    blocks: [
      { info: "Status: in review. Tracks [[PAY-231]]. Owner @Mara, reviewers @Priya and @Jonas." },
      { h: "Context" },
      { p: "The SEPA batch goes to the PSP every day at 13:00. About 30 payouts a week fail with a transient error, and finance retries them by hand the next morning. This page describes how payout-service retries them itself." },
      { h: "Backoff policy" },
      { p: "A transient failure is retried with exponential backoff: base 30 s, factor 2, at most 5 attempts, with jitter of ±10 %. The first try counts as attempt 1, so a payout is sent at most five times." },
      { code: "retry:\n  base: 30s\n  factor: 2\n  max_attempts: 5\n  jitter: 0.10", lang: "yaml" },
      { p: "Five attempts from a 30 s base take about 15 minutes in the worst case, well before the 15:00 cutoff finance gave us." },
      { h: "Transient codes" },
      { table: [["Code", "Comes from", "Retried?"], ["HTTP 503", "PSP gateway", "yes"], ["TEMP_UNAVAILABLE", "PSP", "yes"], ["RATE_LIMITED", "PSP", "no, the PSP asks us not to"], ["TIMEOUT after 60 s", "our client", "yes"]] },
      { p: "The full list of codes is on {{page:PSP error codes}}." },
      { h: "Manual review queue" },
      { p: "After the last attempt the payout moves to the manual review queue, see {{page:Manual review queue}}. Finance works the queue during business hours." },
      { warn: "SLA: to be defined. Finance has asked twice; @Priya decides." },
      { h: "Rollout" },
      { ul: ["Stage first, behind the flag `sepa_retry`, for one week of batches.", "Watch the retries metric on the payouts dashboard.", "Prod after the stage week, if no payout was sent twice."] },
      { macro: "Jira roadmap" },
      { h: "Open questions" },
      { ul: ["Should RATE_LIMITED wait for the PSP's Retry-After header instead of going straight to review?", "Do we page on-call when more than 10 payouts land in review in one batch?"] },
    ],
    comments: [
      ["PN", "today 12:05", "@Mara can you add the manual-review queue SLA here?"],
      ["JB", "yesterday 18:02", "The jitter bound is ±10 % now, which matches PR #142."],
    ],
  },
  "PSP error codes": {
    labels: ["payments", "psp", "reference"],
    version: 9,
    blocks: [
      { p: "Every code the PSP returns on a batch or a single payout, and what payout-service does with it. Keep this table in step with `psp::errors` in payout-service." },
      { h: "Codes" },
      { table: [["Code", "Meaning", "What we do"], ["TEMP_UNAVAILABLE", "The PSP is overloaded or in maintenance", "retry with backoff"], ["RATE_LIMITED", "Too many requests from our key", "stop, send to manual review"], ["INVALID_IBAN", "Checksum or length is wrong", "fail, tell the customer"], ["DUPLICATE", "The idempotency key was seen before", "treat as sent"], ["TIMEOUT", "No answer within 60 s", "retry with backoff"]] },
      { h: "Transient or permanent" },
      { p: "Only TEMP_UNAVAILABLE and TIMEOUT are transient. Everything else is permanent and is never retried automatically. See {{page:SEPA payout retry design}} for the backoff." },
      { note: "The PSP sandbox returns TEMP_UNAVAILABLE on purpose for IBANs ending in 0503. Use those in integration tests." },
    ],
    comments: [["ML", "2026-08-20 09:00", "Added the sandbox IBAN trick, it makes the retry test deterministic."]],
  },
  "Payments architecture overview": {
    labels: ["payments", "architecture"],
    version: 22,
    blocks: [
      { p: "How money moves through Tidewater's payment services, for new engineers and for anyone changing more than one of them." },
      { h: "Services" },
      { ul: ["**payout-service** builds the daily SEPA batch and sends it to the PSP.", "**ledger-api** owns the double-entry tables and exposes /balances.", "**refund-worker** books refunds and partial refunds into the ledger."] },
      { h: "Data flow" },
      { p: "A payout starts as a booking in ledger-api, is picked up by payout-service at 13:00, and comes back as a settlement event the next morning." },
      { macro: "draw.io diagram" },
    ],
    comments: [],
  },
  "On-call handbook": {
    labels: ["ops", "on-call"],
    version: 31,
    blocks: [
      { warn: "Parts of this page describe the old ledger topology. Tomasz is updating it in [[OPS-77]]." },
      { h: "Rota" },
      { p: "On-call rotates weekly on Tuesdays at 10:00. The current person is in the #oncall channel topic." },
      { h: "When an alert fires" },
      { ol: ["Acknowledge it in Uptime Kuma within 15 minutes.", "Check the runbook linked from the monitor.", "If prod payouts are affected, tell finance in #payments-ops."] },
      { h: "Ledger" },
      { p: "The ledger runs as one Postgres on vm-db-01 with a streaming replica on vm-db-02. Restore drills are monthly; see {{page:Ledger reconciliation runbook}}." },
      { h: "Escalation" },
      { table: [["After", "Who"], ["15 min unacknowledged", "second on-call"], ["30 min prod outage", "Tomasz Wójcik"], ["Money at risk", "Priya Nair"]] },
    ],
    comments: [["TW", "2026-08-19 09:00", "Rewriting the Ledger section this week."]],
  },
  "Ledger reconciliation runbook": {
    labels: ["ops", "ledger", "runbook"],
    version: 12,
    blocks: [
      { h: "When to use this" },
      { p: "The nightly reconciliation job flags rows where the ledger and the PSP settlement disagree. Use this runbook when it flags more than 20 rows." },
      { h: "Steps" },
      { ol: ["Export the flagged rows.", "Group them by PSP code and amount.", "For partial refunds, check whether the fee was booked twice; that was [[PAY-228]]."] },
      { code: "select payout_id, ledger_amount, psp_amount\nfrom recon.flagged\nwhere created_at > now() - interval '1 day'\norder by abs(ledger_amount - psp_amount) desc;", lang: "sql" },
    ],
    comments: [],
  },
};

interface FakePage {
  id: string;
  title: string;
  space: string;
  parent: string | null;
  parentTitle: string | null;
}

const PAGES: FakePage[] = [];
let next = 65600;
for (const [space, tops] of Object.entries(TREE)) {
  for (const [title, kids] of tops) {
    const id = title === "SEPA payout retry design" ? "confluence:sepa" : `confluence:${next++}`;
    PAGES.push({ id, title, space, parent: null, parentTitle: null });
    for (const kid of kids) {
      const kidId = kid === "SEPA payout retry design" ? "confluence:sepa" : `confluence:${next++}`;
      PAGES.push({ id: kidId, title: kid, space, parent: id, parentTitle: title });
    }
  }
}

/** A prototype time (`today 10:40`, `2026-08-19 09:00`) on the page's own clock. */
function at(said: string): string {
  const [day, clock] = said.split(" ") as [string, string];
  const [hours, minutes] = clock.split(":").map(Number) as [number, number];
  let when: Date;
  if (day === "today" || day === "yesterday") {
    when = new Date();
    if (day === "yesterday") when.setDate(when.getDate() - 1);
  } else {
    const [year, month, date] = day.split("-").map(Number) as [number, number, number];
    when = new Date(year, month - 1, date);
  }
  when.setHours(hours, minutes, 0, 0);
  return when.toISOString();
}

/** Prototype-6's inline marks, as the projection's inlines. */
function inl(s: string): DocInline[] {
  return s
    .split(/(\[\[[A-Z]+-\d+\]\]|\{\{page:[^}]+\}\}|@[A-Z][a-z]+|`[^`]+`|\*\*[^*]+\*\*)/)
    .filter((part) => part !== "")
    .map((part): DocInline => {
      let m: RegExpMatchArray | null;
      if ((m = part.match(/^\[\[([A-Z]+-\d+)\]\]$/))) {
        const key = m[1]!;
        const row = CORPUS.find((r) => r.kind === "ticket" && r.entity_id === `mock:${key}`);
        return {
          type: "issue",
          key,
          entity_id: row?.entity_id ?? null,
          title: row?.title ?? null,
          status: row ? (String(row.payload["status"] ?? "") || null) : null,
        };
      }
      if ((m = part.match(/^\{\{page:([^}]+)\}\}$/))) {
        const title = m[1]!;
        const target = PAGES.find((p) => p.title === title);
        return { type: "page_link", text: title, title, space: null, entity_id: target?.id ?? null };
      }
      if ((m = part.match(/^@([A-Z][a-z]+)$/))) {
        const first = m[1]!;
        const known = Object.values(PEOPLE).find(([, name]) => name.startsWith(`${first} `));
        return { type: "mention", user: known?.[0] ?? null, name: known?.[1] ?? first, me: known?.[0] === ME };
      }
      if ((m = part.match(/^`([^`]+)`$/))) return { type: "text", text: m[1]!, strong: false, emphasis: false, code: true };
      if ((m = part.match(/^\*\*([^*]+)\*\*$/))) return { type: "text", text: m[1]!, strong: true, emphasis: false, code: false };
      return { type: "text", text: part, strong: false, emphasis: false, code: false };
    });
}

function para(s: string): DocBlock {
  return { type: "paragraph", text: inl(s) };
}

function block(b: Proto): DocBlock {
  if ("h" in b) return { type: "heading", level: 2, text: inl(b.h) };
  if ("p" in b) return para(b.p);
  if ("ul" in b) return { type: "list", ordered: false, items: b.ul.map((l) => [para(l)]) };
  if ("ol" in b) return { type: "list", ordered: true, items: b.ol.map((l) => [para(l)]) };
  if ("code" in b) return { type: "code", code: b.code, language: b.lang ?? null };
  if ("table" in b) {
    return {
      type: "table",
      rows: b.table.map((row, r) => row.map((cell) => ({ header: r === 0, text: inl(cell) }))),
    };
  }
  if ("info" in b) return { type: "panel", kind: "info", title: null, blocks: [para(b.info)] };
  if ("warn" in b) return { type: "panel", kind: "warning", title: null, blocks: [para(b.warn)] };
  if ("note" in b) return { type: "panel", kind: "note", title: null, blocks: [para(b.note)] };
  return { type: "placeholder", name: b.macro };
}

/** prototype-6's generic body for a page it does not write out. */
function generic(page: FakePage, by: string): Body {
  const owner = PEOPLE[by]?.[1] ?? "the team";
  return {
    labels: [(page.parentTitle ?? page.space).toLowerCase().split(" ")[0]!],
    version: 1 + (page.title.length % 9),
    blocks: [
      { p: `${page.title}. Notes kept by the team.` },
      { h: "Details" },
      { p: "Who owns what, and where to ask." },
      { h: "See also" },
      { p: `Ask in the team channel. Owner: ${owner}.` },
    ],
    comments: [],
  };
}

function tree(): DocsTree {
  const spaces: DocSpace[] = Object.entries(SPACES).map(([key, name]) => ({
    source_id: "confluence",
    key,
    name,
    pages: PAGES.filter((p) => p.space === key)
      .map((p) => ({ entity_id: p.id, title: p.title, parent: p.parent }))
      .sort((a, b) => a.title.toLowerCase().localeCompare(b.title.toLowerCase())),
  }));
  return { spaces, last_read: "confluence:sepa" };
}

function page(args: Record<string, unknown>): DocPage {
  const id = String(args["entityId"] ?? "");
  const found = PAGES.find((p) => p.id === id);
  if (!found) throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  const [by, when] = META[found.title] ?? ["JB", "2026-08-10 09:00"];
  const body = BODIES[found.title] ?? generic(found, by);
  return {
    entity_id: found.id,
    source_id: "confluence",
    source_name: "Confluence",
    title: found.title,
    space: { key: found.space, name: SPACES[found.space] ?? null },
    ancestors: found.parent === null ? [] : [{ title: found.parentTitle ?? "", entity_id: found.parent }],
    editor: PEOPLE[by]?.[0] ?? null,
    editor_name: PEOPLE[by]?.[1] ?? null,
    updated_at: at(when),
    web_url: `${HOST}/pages/viewpage.action?pageId=${found.id.slice(found.id.indexOf(":") + 1)}`,
    readable: true,
    version: body.version,
    labels: body.labels,
    blocks: body.blocks.map(block),
    comments: body.comments.map(([who, said, text]) => ({
      author: PEOPLE[who]?.[0] ?? null,
      author_name: PEOPLE[who]?.[1] ?? null,
      at: at(said),
      blocks: [para(text)],
    })),
    me: [ME],
  };
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    docs_tree: () => tree(),
    docs_page: (args) => page(args),
  };
}
