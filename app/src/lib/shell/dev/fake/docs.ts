/**
 * `?fake-ipc`: Docs, the reader over declared docs (#859).
 *
 * Answers `docs_tree`, `docs_page`, `docs_search` (#860), and the writes
 * `docs_export_markdown` (#861), `docs_edit_section` and `docs_create_page`
 * (#862), and `move_notes_to_confluence` (#941), which answer as queued and
 * change nothing.
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
 * - `get_entity` answers only for *SEPA payout retry design* (`confluence:sepa`,
 *   which `CORPUS` holds), so only that page draws *Refresh* (#2136); the
 *   real read answers for every mirrored page.
 * - The search is a small stand-in for the backend's (`docs_search`): every
 *   word must appear (prototype-6's stemming), `"…"`, `-word`, `by:`,
 *   `updated:` and `space:` as the grammar reads them; ranking is title
 *   first, then the number of matches.
 * - `?fake-docs-pages=<n>` adds a sixth space, *BULK*, of `n` generated
 *   pages four or five folds deep, to measure Docs at a real space's size
 *   (#2200: `&fake-docs-pages=4000`). Its pages carry the generic body and
 *   the search matches them by title only.
 * - *For you* is prototype-6's mention on *SEPA payout retry design* only;
 *   its *Manual review queue* "SLA section is empty" is a reason no source
 *   declares, so it is not here.
 * - A section is refused for a table, code or a macro; links, chips and
 *   marks refuse as formatting. A panel of plain text and lists is edited as
 *   a quote naming its kind, `> **Info**`, as the Confluence projection
 *   spells it (#975).
 */
import type { QueuedWrite, WriteOpPayload } from "../../../ipc/sources";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import type {
  DocBlock,
  DocHit,
  DocInline,
  DocPage,
  DocSection,
  DocsHits,
  DocsTree,
  DocSpace,
  SectionRefusal,
} from "../../../ipc/entity";
import { blockWords, inlineWords, sectionsOf, stem } from "../../../tf/docs/docs";
import { CORPUS, fakeRoleScope } from "./corpus";
import { keyOf } from "../../../time/format";

/** The role `docs_tree` is read for, as `knobas_source::role::WIKI` spells it. */
const WIKI_ROLE = "wiki";
/** The source the fixture's spaces and pages are of. */
const SOURCE = "confluence";

/** Whether the fixture's source fills the wiki, which both Docs reads narrow to (#1296). */
function wikiFilled(): boolean {
  return fakeRoleScope(WIKI_ROLE).sources.includes(SOURCE);
}

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

/**
 * The pages other modules name by id: the one read last, and PAY-231's two
 * pages, which `job-reach.ts` lists in what the job reaches (#863).
 */
const NAMED: Record<string, string> = {
  "SEPA payout retry design": "confluence:sepa",
  "Manual review queue": "confluence:manual-review",
  "PSP error codes": "confluence:psp-codes",
};

const PAGES: FakePage[] = [];
let next = 65600;
for (const [space, tops] of Object.entries(TREE)) {
  for (const [title, kids] of tops) {
    const id = NAMED[title] ?? `confluence:${next++}`;
    PAGES.push({ id, title, space, parent: null, parentTitle: null });
    for (const kid of kids) {
      const kidId = NAMED[kid] ?? `confluence:${next++}`;
      PAGES.push({ id: kidId, title: kid, space, parent: id, parentTitle: title });
    }
  }
}

/** The key of the space `?fake-docs-pages=<n>` adds (#2200). */
const BULK_KEY = "BULK";
const BULK_NAME = "Bulk pages";
/**
 * `?fake-docs-pages=<n>` (#2200): one more space, *BULK*, of `n` generated
 * pages, the size a real Confluence space reaches (Björn's held 4,000). Twenty
 * pages at its top and every later page under an earlier one, eight to a
 * fold, so the tree is four or five folds deep. A title is two words and its
 * number, from sixteen words each, so a word finds a sixteenth of the space.
 */
function bulkPages(n: number): FakePage[] {
  const words = ["Payout", "Ledger", "Carrier", "Customs", "Booking", "Retry", "Invoice", "Tracking", "Refund", "Settlement", "Runbook", "Design", "Review", "Onboarding", "Platform", "Release"];
  const out: FakePage[] = [];
  for (let i = 0; i < n; i++) {
    const title = `${words[i % words.length]} ${words[(i * 7 + 3) % words.length]!.toLowerCase()} ${i + 1}`;
    const up = i < 20 ? null : out[Math.floor((i - 20) / 8)]!;
    out.push({ id: `confluence:bulk-${i + 1}`, title, space: BULK_KEY, parent: up?.id ?? null, parentTitle: up?.title ?? null });
  }
  return out;
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

/**
 * `docs_tree`, the wiki role's read by definition: narrowed to the role's
 * fillers as the server's is (#1296), so with Confluence filling no wiki the
 * tree holds no space, no page read last and no *For you*.
 */
function tree(bulk: FakePage[]): DocsTree {
  if (!wikiFilled()) return { spaces: [], last_read: null, for_you: [] };
  const spaces: DocSpace[] = Object.entries(SPACES).map(([key, name]) => ({
    source_id: SOURCE,
    key,
    name,
    pages: PAGES.filter((p) => p.space === key)
      .map((p) => ({ entity_id: p.id, title: p.title, parent: p.parent }))
      .sort((a, b) => a.title.toLowerCase().localeCompare(b.title.toLowerCase())),
  }));
  if (bulk.length > 0)
    spaces.push({
      source_id: SOURCE,
      key: BULK_KEY,
      name: BULK_NAME,
      pages: bulk.map((p) => ({ entity_id: p.id, title: p.title, parent: p.parent })),
    });
  return {
    spaces,
    last_read: "confluence:sepa",
    for_you: [{ entity_id: "confluence:sepa", title: "SEPA payout retry design", space_key: "ENG", by_name: "Priya Nair" }],
  };
}

/** Why knobas would not edit a section, as the Confluence projection says. */
function refusal(blocks: DocBlock[]): SectionRefusal | null {
  let worst: SectionRefusal | null = null;
  const rank: SectionRefusal[] = ["table", "code", "macro", "formatting"];
  const note = (r: SectionRefusal) => {
    if (worst === null || rank.indexOf(r) < rank.indexOf(worst)) worst = r;
  };
  const inlines = (text: DocInline[]) => {
    for (const run of text) {
      if (run.type === "text" && !run.strong && !run.emphasis && !run.code) continue;
      if (run.type === "break") continue;
      note(run.type === "issue" || run.type === "mention" || run.type === "page_link" ? "macro" : "formatting");
    }
  };
  const visit = (list: DocBlock[], inPanel: boolean) => {
    for (const b of list) {
      if (b.type === "table") note("table");
      else if (b.type === "code") note("code");
      else if (b.type === "placeholder" || (b.type === "panel" && inPanel)) note("macro");
      else if (b.type === "panel") visit(b.blocks, true);
      else if (b.type === "paragraph") inlines(b.text);
      else if (b.type === "list") {
        for (const item of b.items) {
          for (const inner of item) {
            if (inner.type === "paragraph") inlines(inner.text);
            else note("formatting");
          }
        }
      }
    }
  };
  visit(blocks, false);
  return worst;
}

/** An editable section's text: a panel is a quote naming its kind, `> **Info**`. */
function sectionText(blocks: DocBlock[]): string {
  return blocks
    .map((b) => {
      if (b.type === "list") return b.items.map((item) => `- ${item.map(blockWords).join(" ")}`).join("\n");
      if (b.type !== "panel") return blockWords(b);
      const head = `**${b.kind.charAt(0).toUpperCase()}${b.kind.slice(1)}**${b.title ? ` ${b.title}` : ""}`;
      const body = sectionText(b.blocks);
      return (body === "" ? head : `${head}\n\n${body}`)
        .split("\n")
        .map((line) => (line === "" ? ">" : `> ${line}`))
        .join("\n");
    })
    .join("\n\n");
}

function sections(blocks: DocBlock[]): DocSection[] {
  return sectionsOf(blocks)
    .filter((s) => s.heading !== null)
    .map((s) => {
      const why = refusal(s.blocks);
      return { number: s.number, refusal: why, text: why === null ? sectionText(s.blocks) : null };
    });
}

/** The tickets a body's chips name, with their status. */
function work(blocks: DocBlock[]): DocPage["work"] {
  const out: DocPage["work"] = [];
  const walk = (text: DocInline[]) => {
    for (const run of text) {
      if (run.type === "issue" && run.entity_id && !out.some((w) => w.entity_id === run.entity_id)) {
        out.push({ entity_id: run.entity_id, key: run.key, title: run.title ?? run.key, status: run.status, linked: false });
      }
    }
  };
  const visit = (b: DocBlock) => {
    if (b.type === "heading" || b.type === "paragraph") walk(b.text);
    else if (b.type === "list") b.items.flat().forEach(visit);
    else if (b.type === "panel") b.blocks.forEach(visit);
    else if (b.type === "table") b.rows.flat().forEach((c) => walk(c.text));
  };
  blocks.forEach(visit);
  return out;
}

/** The pages whose prototype body links to `title`. */
function backlinks(title: string): DocPage["backlinks"] {
  return PAGES.filter((p) => {
    const body = BODIES[p.title];
    return body !== undefined && p.title !== title && JSON.stringify(body.blocks).includes(`{{page:${title}}}`);
  }).map((p) => ({ entity_id: p.id, title: p.title, space_key: p.space }));
}

function page(args: Record<string, unknown>, bulk: FakePage[] = []): DocPage {
  const id = String(args["entityId"] ?? "");
  const found = PAGES.find((p) => p.id === id) ?? bulk.find((p) => p.id === id);
  if (!found) throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  const [by, when] = META[found.title] ?? ["JB", "2026-08-10 09:00"];
  const body = BODIES[found.title] ?? generic(found, by);
  return {
    entity_id: found.id,
    source_id: "confluence",
    source_name: "Confluence",
    title: found.title,
    space: { key: found.space, name: SPACES[found.space] ?? (found.space === BULK_KEY ? BULK_NAME : null) },
    ancestors: found.parent === null ? [] : [{ title: found.parentTitle ?? "", entity_id: found.parent }],
    editor: PEOPLE[by]?.[0] ?? null,
    editor_name: PEOPLE[by]?.[1] ?? null,
    updated_at: at(when),
    web_url: `${HOST}/pages/viewpage.action?pageId=${keyOf(found.id)}`,
    readable: true,
    version: body.version,
    labels: body.labels,
    blocks: body.blocks.map(block),
    // The fake sends no comment, so it names none by id: Docs matches by the words.
    comments: body.comments.map(([who, said, text]) => ({
      id: null,
      author: PEOPLE[who]?.[0] ?? null,
      author_name: PEOPLE[who]?.[1] ?? null,
      at: at(said),
      blocks: [para(text)],
    })),
    sent_comments: [],
    me: [ME],
    sections: sections(body.blocks.map(block)),
    history: [
      {
        number: body.version,
        by: PEOPLE[by]?.[0] ?? null,
        by_name: PEOPLE[by]?.[1] ?? null,
        at: at(when),
        message: null,
      },
      // Where the page has moved on, its first version too, for *Created by* (#2080).
      ...(body.version > 1
        ? [{ number: 1, by: PEOPLE[by]?.[0] ?? null, by_name: PEOPLE[by]?.[1] ?? null, at: at("2026-06-01 09:00"), message: null }]
        : []),
    ],
    backlinks: backlinks(found.title),
    work: work(body.blocks.map(block)),
    writes: ["comment", "update_page", "create_page"],
  };
}

/** The query as the grammar reads it, for the stand-in search. */
function parse(q: string): { need: string[]; not: string[]; by?: string; updated?: string; space?: string } {
  const out: { need: string[]; not: string[]; by?: string; updated?: string; space?: string } = { need: [], not: [] };
  for (const m of q.matchAll(/(-?)"([^"]+)"|(\S+)/g)) {
    if (m[2] !== undefined) {
      (m[1] ? out.not : out.need).push(m[2].toLowerCase());
      continue;
    }
    const w = m[3]!;
    const key = w.match(/^(space|project|by|author|updated):(.+)$/i);
    if (key) {
      const k = key[1]!.toLowerCase();
      const v = key[2]!.toLowerCase();
      if (k === "space" || k === "project") out.space = v;
      else if (k === "updated") out.updated = v;
      else out.by = v;
    } else if (w.length > 1 && w.startsWith("-")) out.not.push(w.slice(1).toLowerCase());
    else if (w.toLowerCase() !== "or") out.need.push(w.toLowerCase());
  }
  return out;
}

function search(args: Record<string, unknown>, bulk: FakePage[]): DocsHits | null {
  const started = performance.now();
  const q = parse(String(args["query"] ?? ""));
  const scope = args["space"] as { key: string } | null | undefined;
  if (q.need.length === 0 && q.not.length === 0 && !q.by && !q.updated && !q.space) return null;
  const space = q.space ?? scope?.key.toLowerCase();
  const days = q.updated ? ({ today: 1, "7d": 7, "30d": 30, "1y": 365 } as Record<string, number>)[q.updated] : undefined;
  const stems = q.need.map(stem);
  const count = (text: string, s: string) => text.toLowerCase().split(s).length - 1;
  const hits: (DocHit & { score: number })[] = [];
  // The wiki's search, narrowed to the role's fillers as the tree is (#1296).
  for (const p of wikiFilled() ? PAGES : []) {
    if (space && p.space.toLowerCase() !== space) continue;
    const doc = page({ entityId: p.id });
    if (q.by && doc.editor !== q.by && !(doc.editor_name ?? "").toLowerCase().startsWith(q.by)) continue;
    if (days !== undefined && Date.now() - new Date(doc.updated_at ?? 0).getTime() > days * 864e5) continue;
    const parts: { section: string | null; comment_by: string | null; text: string }[] = sectionsOf(doc.blocks).map(
      (s) => ({
        section: s.heading ? inlineWords(s.heading.text) : null,
        comment_by: null,
        text: [s.heading ? inlineWords(s.heading.text) : "", ...s.blocks.map(blockWords)].join(" "),
      }),
    );
    for (const c of doc.comments) parts.push({ section: null, comment_by: c.author_name, text: c.blocks.map(blockWords).join(" ") });
    const all = [doc.title, ...parts.map((x) => x.text)].join(" ");
    if (q.not.some((n) => count(all, stem(n)) > 0)) continue;
    if (stems.some((s) => count(all, s) === 0)) continue;
    let best: { k: number; sentence: string; section: string | null; comment_by: string | null } | null = null;
    for (const part of parts) {
      for (const sentence of part.text.split(/(?<=[.!?])\s+/)) {
        const k = stems.filter((s) => count(sentence, s) > 0).length;
        if (k > 0 && (best === null || k > best.k)) best = { k, sentence, section: part.section, comment_by: part.comment_by };
      }
    }
    const sentence = best?.sentence ?? parts.find((x) => x.text.trim() !== "")?.text ?? "";
    const segments: { text: string; hit: boolean }[] = [];
    for (const piece of sentence.slice(0, 150).split(new RegExp(`(${stems.map((s) => `${s}[\\p{L}\\p{N}]*`).join("|") || "$^"})`, "iu"))) {
      if (piece === "") continue;
      segments.push({ text: piece, hit: stems.some((s) => piece.toLowerCase().startsWith(s)) });
    }
    const matches = stems.reduce((n, s) => n + count(all, s), 0);
    hits.push({
      entity_id: doc.entity_id,
      title: doc.title,
      space: doc.space,
      path: p.parentTitle,
      snippet: { section: best?.section ?? null, comment_by: best?.comment_by ?? null, segments },
      matches,
      editor: doc.editor,
      editor_name: doc.editor_name,
      updated_at: doc.updated_at,
      score: stems.reduce((n, s) => n + count(doc.title, s) * 8, 0) + matches,
    });
  }
  // The bulk space's pages match on their titles alone, so the stand-in costs
  // the reader's main thread no more than the backend's answer does (#2200).
  for (const p of wikiFilled() && !q.by && days === undefined ? bulk : []) {
    if (space && p.space.toLowerCase() !== space) continue;
    if (q.not.some((n) => count(p.title, stem(n)) > 0) || stems.some((s) => count(p.title, s) === 0)) continue;
    const matches = stems.reduce((n, s) => n + count(p.title, s), 0);
    hits.push({
      entity_id: p.id,
      title: p.title,
      space: { key: BULK_KEY, name: BULK_NAME },
      path: p.parentTitle,
      snippet: { section: null, comment_by: null, segments: [{ text: p.title, hit: false }] },
      matches,
      editor: "jonas",
      editor_name: "Jonas Becker",
      updated_at: null,
      score: matches * 8,
    });
  }
  hits.sort((a, b) => b.score - a.score);
  return {
    total: hits.length,
    took_ms: Math.max(1, Math.round(performance.now() - started)),
    needles: q.need,
    unknown_tokens: [],
    hits: hits.slice(0, 50).map(({ score: _score, ...hit }) => hit),
  };
}

function queued(op: string, entity: string, payload: WriteOpPayload): QueuedWrite {
  return {
    id: Math.floor(Math.random() * 1e6),
    source_id: "confluence",
    entity_id: entity,
    op,
    payload,
    target_snapshot: null,
    state: "pending",
    wait_reason: null,
    detail: null,
    queued_at: new Date().toISOString(),
    attempted_at: null,
    attempts: 0,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  };
}

/** This feature's commands. */
export function handlers(params: URLSearchParams): Handlers {
  // The bulk space's pages, none unless the page was opened with the switch.
  const bulk = bulkPages(Number(params.get("fake-docs-pages") ?? 0) || 0);
  return {
    docs_tree: () => tree(bulk),
    docs_page: (args) => page(args, bulk),
    docs_search: (args) => search(args, bulk),
    docs_export_markdown: (args) => {
      const found = [...PAGES, ...bulk].find((p) => p.id === String(args["entityId"] ?? ""));
      const file = `${(found?.title ?? "untitled").toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "")}.md`;
      return { file, folder: "/Users/you/Downloads" };
    },
    docs_edit_section: (args) =>
      queued("update_page", String(args["entityId"] ?? ""), { UpdatePage: { entity: String(args["entityId"] ?? ""), base_version: Number(args["version"] ?? 0), body: "" } }),
    docs_create_page: (args) => {
      const next = args["page"] as { parent: string; title: string };
      return queued("create_page", next.parent, { CreatePage: { parent: next.parent, space: "", title: next.title, body: "" } });
    },
    // *Move to Confluence…* (#941): one page per note, the 37 `capture_shortcut` counts.
    move_notes_to_confluence: (args) => {
      const parent = String(args["parent"] ?? "");
      return Array.from({ length: 37 }, (_, n) =>
        queued("create_page", parent, { CreatePage: { parent, space: "", title: `Note ${n + 1}`, body: "" } }),
      );
    },
  };
}

keepSeed(PAGES);
