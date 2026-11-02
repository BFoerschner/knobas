/**
 * PROTOTYPE (`?fake-ipc&launcher-next`, 2026-11-02): a full-text search over
 * the fixture, so the launcher clickdummy in `launcher/prototype/` has
 * something to find in a plain browser.
 *
 * The real `search` is PostgreSQL FTS in `knobas-search`; `fake/launcher.ts`
 * refuses free text on purpose, because a substring match certifies nothing
 * about the query. This one exists only so the clickdummy can be clicked. It
 * answers the same shape: groups in the engine's order, the matched excerpt
 * as `snippet` segments, `kind:` / `type:` / `#` as the grammar reads them.
 *
 * It also reads `doc:` as "pages only", which the real parser does not yet:
 * the prototype's one grammar change, here so the wire shape of the answer
 * can be looked at. The launcher itself rewrites `doc:` to `kind:page` before
 * asking, so against the real backend the same words work today.
 *
 * Throwaway: on the prototype branch, never on main.
 */
import type { ResultGroup, SearchHit, SearchResponse, Segment } from "../../ipc/search";
import { fakePipelines } from "./fake/builds";
import { CORPUS, SYNCED_AT, mirrorRow } from "./fake/corpus";
import { fakeDocPages } from "./fake/docs";

/** The engine's group order (`knobas_search::group`), unknown kinds last. */
const ORDER = ["ticket", "pr", "pipeline", "build", "page", "note", "commit", "branch", "repo", "asset", "monitor", "incident"];

const WORDS: Record<string, [label: string, plural: string, monogram: string]> = {
  ticket: ["Ticket", "Tickets", "TK"],
  pr: ["Pull request", "Pull requests", "PR"],
  build: ["Build", "Builds", "BU"],
  pipeline: ["Pipeline", "Pipelines", "PL"],
  page: ["Page", "Pages", "PG"],
  note: ["Note", "Notes", "NT"],
  commit: ["Commit", "Commits", "CM"],
  branch: ["Branch", "Branches", "BR"],
  repo: ["Repository", "Repositories", "RP"],
  asset: ["Asset", "Assets", "AS"],
  monitor: ["Monitor", "Monitors", "MO"],
  incident: ["Incident", "Incidents", "IN"],
};

interface Parsed {
  words: string[];
  kinds: string[];
  prefix: "ticket" | null;
}

/** `#` for tickets, `kind:`/`type:` for one kind, `doc:` for pages; the rest are words. */
function parse(raw: string): Parsed {
  let text = raw.trim();
  const kinds: string[] = [];
  let prefix: Parsed["prefix"] = null;
  if (text.startsWith("#")) {
    prefix = "ticket";
    kinds.push("ticket");
    text = text.slice(1);
  }
  const words: string[] = [];
  for (const token of text.split(/\s+/).filter(Boolean)) {
    const keyed = /^(doc|kind|type):(.*)$/i.exec(token);
    if (keyed) {
      const key = keyed[1]!.toLowerCase();
      const value = keyed[2]!.toLowerCase();
      if (key === "doc") {
        kinds.push("page");
        if (value) words.push(value);
      } else if (value) {
        kinds.push(value);
      }
      continue;
    }
    words.push(token.toLowerCase());
  }
  return { words, kinds: [...new Set(kinds)], prefix };
}

/** Every string one level down in a payload, joined: a title, a description, labels. */
function payloadText(payload: Record<string, unknown>): string {
  const out: string[] = [];
  for (const value of Object.values(payload)) {
    if (typeof value === "string") out.push(value);
    else if (Array.isArray(value)) for (const item of value) if (typeof item === "string") out.push(item);
  }
  return out.join(" ");
}

/** The excerpt around the first word found in `text`, or none when only the title matched. */
function excerpt(text: string, words: string[]): Segment[] {
  const flat = text.replace(/\s+/g, " ");
  const lower = flat.toLowerCase();
  for (const word of words) {
    const at = lower.indexOf(word);
    if (at < 0) continue;
    const from = Math.max(0, at - 40);
    const to = Math.min(flat.length, at + word.length + 60);
    return [
      { text: `${from > 0 ? "…" : ""}${flat.slice(from, at)}`, hit: false },
      { text: flat.slice(at, at + word.length), hit: true },
      { text: `${flat.slice(at + word.length, to)}${to < flat.length ? "…" : ""}`, hit: false },
    ];
  }
  return [];
}

type Candidate = Omit<SearchHit, "rank" | "snippet"> & { haystack: string; body: string };

/** Done is green, in progress blue, closed grey, to do uncoloured (Björn, round 10). */
function ticketState(status: string): SearchHit["state"] {
  const word = status.toLowerCase();
  if (word === "done" || word === "resolved") return { text: status, tone: "ok" };
  if (word.includes("progress") || word.includes("review")) return { text: status, tone: "info" };
  if (word.includes("closed") || word.includes("won't") || word.includes("cancel")) return { text: status, tone: "muted" };
  return null;
}

function candidates(): Candidate[] {
  const pages = fakeDocPages();
  const pageIds = new Set(pages.map((page) => page.id));
  const fromCorpus: Candidate[] = CORPUS.filter((entry) => entry.deleted_at === null && !pageIds.has(entry.entity_id)).map(
    (entry) => {
      const body = payloadText(entry.payload);
      const row = mirrorRow(entry);
      // A ticket's status as a state (round 10): the real chip is the adapter's declared status with its tone.
      const status = typeof entry.payload["status"] === "string" ? entry.payload["status"] : null;
      const state = row.state ?? (entry.kind === "ticket" && status ? ticketState(status) : null);
      return { ...row, state, haystack: `${entry.title} ${body}`.toLowerCase(), body };
    },
  );
  const fromDocs: Candidate[] = pages.map((page) => {
    const path = page.parentTitle ? `${page.space} › ${page.parentTitle}` : page.space;
    return {
      entity_id: page.id,
      kind: "page",
      source_id: "confluence",
      title: page.title,
      updated_at: null,
      synced_at: SYNCED_AT,
      path,
      state: null,
      haystack: `${page.title} ${path}`.toLowerCase(),
      body: "",
    };
  });
  // The build configurations (round 9): what `#b` finds; their state is the last run's.
  const fromPipelines: Candidate[] = fakePipelines().map((pipeline) => ({
    entity_id: pipeline.entity_id,
    kind: "pipeline",
    source_id: pipeline.source_id,
    title: pipeline.name,
    updated_at: pipeline.recent[0]?.updated_at ?? null,
    synced_at: SYNCED_AT,
    path: pipeline.path.join(" › "),
    state: pipeline.recent[0]?.state ?? null,
    haystack: `${pipeline.name} ${pipeline.path.join(" ")}`.toLowerCase(),
    body: "",
  }));
  return [...fromCorpus, ...fromDocs, ...fromPipelines];
}

/**
 * The prototype's `search`. `fallback` is the fixture's own answer, kept for
 * the Tree's asset search (`kind:asset`), which it answers today.
 */
export function launcherNextSearch(
  args: Record<string, unknown>,
  fallback: (args: Record<string, unknown>) => SearchResponse,
): SearchResponse {
  const query = (args.query ?? {}) as { raw?: string; filters?: { kinds?: string[] } };
  const raw = (query.raw ?? "").trim();
  if ((query.filters?.kinds ?? []).includes("asset") || raw.startsWith(">") || /^list:/.test(raw)) return fallback(args);
  const parsed = parse(raw);
  const interpreted = {
    text: parsed.words.join(" "),
    prefix: parsed.prefix,
    filters: { sources: [], kinds: parsed.kinds, updated_within_days: null, mine: false, authors: [] },
    unknown_tokens: [],
  };
  if (parsed.words.length === 0 && parsed.kinds.length === 0) {
    return { interpreted, groups: [], total: 0, took_ms: 1, coverage: [] };
  }
  const byKind = new Map<string, SearchHit[]>();
  for (const candidate of candidates()) {
    if (parsed.kinds.length > 0 && !parsed.kinds.includes(candidate.kind)) continue;
    if (!parsed.words.every((word) => candidate.haystack.includes(word))) continue;
    const { haystack, body, ...hit } = candidate;
    void haystack;
    const hits = byKind.get(hit.kind) ?? [];
    hits.push({ ...hit, rank: 1 - hits.length / 10, snippet: excerpt(body, parsed.words) });
    byKind.set(hit.kind, hits);
  }
  const groups: ResultGroup[] = [...byKind.entries()]
    .sort(([left], [right]) => {
      const l = ORDER.indexOf(left);
      const r = ORDER.indexOf(right);
      return (l < 0 ? ORDER.length : l) - (r < 0 ? ORDER.length : r) || left.localeCompare(right);
    })
    .map(([kind, hits]) => {
      const [label, plural, monogram] = WORDS[kind] ?? [kind, `${kind}s`, kind.slice(0, 2).toUpperCase()];
      return { kind, label, plural, monogram, total: hits.length, hits };
    });
  return { interpreted, groups, total: groups.reduce((n, group) => n + group.total, 0), took_ms: 3, coverage: [] };
}
