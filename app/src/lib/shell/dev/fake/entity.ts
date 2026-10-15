/**
 * `?fake-ipc`: One entity: the detail and the job page's reads, the reachable
 * transitions, and the URL resolver.
 */
import type { LinkEntry, LinkRow } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { FAKE_ACTIVITY } from "./activity";
import { CORPUS, mirrorRow } from "./corpus";

/**
 * `resolve_url` (#496): what a pasted link names, over this fixture's corpus.
 *
 * A **second** implementation of the normalisation rule, and deliberately so:
 * the rule itself is SQL (`knobas_core::web_url_normalized!`) and there is no
 * database here. What this owes is not fidelity to the SQL but the two
 * outcomes a QA walk has to be able to see — a link that opens its entity, and
 * one that offers the browser — so it does the three things the rule does to
 * the URLs this corpus actually holds and no more.
 */
function resolveUrl(args: Record<string, unknown>) {
  const url = String(args["url"] ?? "");
  if (!/^https?:\/\//i.test(url)) {
    throw { code: "invalid", message: `${url} is not an http or https URL`, source_id: null };
  }
  const key = (raw: string) => {
    const parsed = new URL(raw);
    parsed.hash = "";
    parsed.pathname = parsed.pathname.replace(/\/+$/, "");
    return parsed.href.replace(/\/$/, "");
  };
  const wanted = key(url);
  const found = CORPUS.find((entry) => entry.web_url !== null && key(entry.web_url) === wanted);
  return found ? { entity_id: found.entity_id, kind: found.kind } : null;
}

/**
 * The link graph, in memory (#835, #867): prototype-6's `s1` and `s3` already
 * confirmed, over the entities this corpus holds (PR #142 implements PAY-231;
 * the retry design page documents it), so the job card's connections have rows. A link
 * made by `create_link` is `manual` and one `unlink` removes goes from both
 * ends at once, as the real graph is undirected. Reload and it is the seed
 * again. `delete_note` is not answered: notes are hidden from the new shell
 * (D10).
 */
let nextLink = 1;
function linkRow(from_id: string, to_id: string, relation: string, origin: LinkRow["origin"], note: string | null): LinkRow {
  return {
    id: `link:fake-made-${nextLink++}`,
    from_id,
    to_id,
    relation,
    origin,
    note,
    created_by: origin === "manual" ? "mara" : "knobas",
    created_at: "2026-08-22T10:30:00Z",
    confirmed_at: "2026-08-22T10:30:00Z",
    rule: null,
    rule_class: null,
    reason: null,
  };
}
const LINKS: LinkRow[] = [
  linkRow("mock:payout-service#142", "mock:PAY-231", "implements", "source", null),
  linkRow("mock:ENG-SEPA", "mock:PAY-231", "documents", "manual", null),
];

/** One link's far end from `id`, resolved against the corpus. */
function entryFor(id: string, link: LinkRow): LinkEntry | null {
  const otherId = link.from_id === id ? link.to_id : link.from_id;
  const other = CORPUS.find((candidate) => candidate.entity_id === otherId);
  if (!other) return null;
  return { link, other: { entity_id: otherId, kind: other.kind, title: other.title, deleted_at: other.deleted_at } };
}

/** The confirmed links on an entity, newest first, from either end. */
function linksOf(id: string): LinkEntry[] {
  return LINKS.filter((link) => link.from_id === id || link.to_id === id)
    .map((link) => entryFor(id, link))
    .filter((entry): entry is LinkEntry => entry !== null)
    .reverse();
}

/** `get_entity`, including the two refusals the detail view branches on. */
function getEntity(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (!id.includes(":") || id.startsWith(":") || id.endsWith(":")) {
    throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key`, source_id: null };
  }
  const entry = CORPUS.find((candidate) => candidate.entity_id === id);
  if (!entry) {
    throw { code: "not_found", message: `${id} is not in the mirror`, source_id: null };
  }
  return {
    row: mirrorRow(entry),
    source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true },
    // Null, where the real command now resolves the adapter's KindInfo from
    // the registry. Harmless divergence: the fallback path draws the same
    // words — knobas' own vocabulary (`kinds.ts` layer 2) matches the mock's
    // declared KindInfo for every kind in this corpus.
    kind_info: null,
    body_text: entry.body_text,
    author: entry.author,
    payload: entry.payload,
    web_url: entry.web_url,
    deleted_at: entry.deleted_at,
    links: linksOf(id),
    activity: FAKE_ACTIVITY.filter((line) => line.entity_id === id),
    // The mock corpus gives no build a commit, so nothing ran on any (#710).
    builds: [],
    // The job card's fields (#816), read from the fixture's flat mock shape:
    // the mock adapter declares `status` and `priority` there. Type and
    // estimate are this fixture's own (`type`, `estimate_h`), which the mock
    // adapter does not declare: a demo divergence, so the card has its whole
    // facts line under `?fake-ipc`.
    status: entry.kind === "ticket" && entry.deleted_at === null ? fakeText(entry.payload["status"]) : null,
    issue_type: fakeText(entry.payload["type"]),
    priority: fakeText(entry.payload["priority"]),
    estimate_seconds:
      typeof entry.payload["estimate_h"] === "number" ? Math.round(entry.payload["estimate_h"] * 3600) : null,
  };
}

/** A payload value that is non-blank text, trimmed, or `null` (ADR-0007's miss). */
function fakeText(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/**
 * `reachable_transitions` (#816's status select). The mock adapter declares no
 * workflow, so the real command refuses it by name; the fixture answers with
 * prototype-6's workflow instead, so the select has somewhere to go.
 */
function fakeTransitions(args: Record<string, unknown>): string[] {
  const entry = CORPUS.find((candidate) => candidate.entity_id === String(args["entityId"] ?? ""));
  const status = fakeText(entry?.payload["status"]);
  const WORKFLOW: Record<string, string[]> = {
    "To Do": ["In Progress"],
    "In Progress": ["In Review", "To Do"],
    "In Review": ["Done", "In Progress"],
    Done: ["In Progress"],
  };
  return status === null ? [] : (WORKFLOW[status] ?? []);
}

/**
 * `entity_view` (#661): the demo commit draws an overview and a diff, so the
 * view's tabs and the diff viewer can be looked at in a browser; every other
 * entity draws nothing, which is the real mock adapter's answer for all of
 * them. A deliberate divergence, for the same reason `kind_info` above is one:
 * the shell under test is the same either way.
 */
function entityView(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (id !== "mock:9f2c1ab") return null;
  const section = (args["section"] as string | null | undefined) ?? null;
  const overview = {
    id: "overview",
    title: "Overview",
    count: null,
    deferred: false,
    blocks: [
      {
        type: "facts",
        facts: [
          { label: "Commit", value: "9f2c1ab0d4", tone: null },
          { label: "SHA", value: "9f2c1ab0d47e3c5a8b1f6e2d9c0a7b4e3f5d6c1a", tone: null },
          { label: "Author", value: "Mara Lindqvist <mara@tidewater.example>", tone: null },
          { label: "Authored", value: "2026-08-22 09:40 +02:00", tone: null },
        ],
      },
      {
        type: "entities",
        title: "Parent",
        rows: [{ entity_id: null, title: "41d0e7c2aa", meta: null, tone: null, chip: null, url: null }],
      },
      {
        type: "text",
        text: "payout: key the retry on the mandate id\n\nPAY-231. Two SEPA banks answer 409 on a retried payout\nbecause the idempotency key was the batch id.",
        markdown: false,
      },
    ],
  };
  const line = (kind: string, old: number | null, nu: number | null, text: string) => ({ kind, old, new: nu, text });
  const diff = {
    id: "diff",
    title: "Diff",
    count: 2,
    deferred: false,
    blocks: [
      {
        type: "diff",
        truncated: false,
        files: [
          {
            path: "src/payout/retry.rs",
            old_path: null,
            change: "modified",
            additions: 3,
            deletions: 1,
            truncated: false,
            hunks: [
              {
                header: "@@ -41,7 +41,9 @@ impl Retry {",
                lines: [
                  line("context", 41, 41, "    fn key(&self, payout: &Payout) -> IdempotencyKey {"),
                  line("del", 42, null, "        IdempotencyKey::from(payout.batch_id)"),
                  line("add", null, 42, "        // One key per mandate: a retried batch is the same payout."),
                  line("add", null, 43, "        IdempotencyKey::from(payout.mandate_id)"),
                  line("context", 43, 44, "    }"),
                  line("context", 44, 45, ""),
                  line("add", null, 46, "    const MAX_ATTEMPTS: u32 = 5;"),
                ],
              },
            ],
          },
          {
            path: "docs/retry.md",
            old_path: "docs/backoff.md",
            change: "renamed",
            additions: 0,
            deletions: 0,
            truncated: false,
            hunks: [],
          },
        ],
      },
    ],
  };
  const sections = section === null ? [overview, diff] : [overview, diff].filter((s) => s.id === section);
  return { sections };
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    get_entity: (args) => getEntity(args),
    reachable_transitions: (args) => fakeTransitions(args),
    entity_view: (args) => entityView(args),
    resolve_url: (args) => resolveUrl(args),
    create_link: (args) => {
      const from = String(args["fromId"]);
      const to = String(args["toId"]);
      for (const end of [from, to]) {
        if (!CORPUS.some((candidate) => candidate.entity_id === end)) {
          throw { code: "not_found", message: `${end} is not in the mirror`, source_id: null };
        }
      }
      const made = linkRow(from, to, String(args["relation"] ?? "related"), "manual", (args["note"] as string | null) ?? null);
      LINKS.push(made);
      return made;
    },
    unlink: (args) => {
      const at = LINKS.findIndex((link) => link.id === args["linkId"]);
      if (at === -1) throw { code: "not_found", message: `${String(args["linkId"])} is not a link`, source_id: null };
      LINKS.splice(at, 1);
      return null;
    },
  };
}
