/**
 * The pull request page's pure half (#841, R-8.15, R-8.16, R-8.20): what an
 * address names, and what the source's view of a pull request says the page
 * draws.
 *
 * The page reads a pull request through its source's own view (`entity_view`,
 * ADR-0020), whole and once: `knobas-source-gitea`'s `view/pr.rs` answers
 * *Overview* (its facts, the head commit's *Checks* and the description),
 * *Files* as a deferred stub with its count, *Commits*, *Conversation* and the
 * toolbar's actions. Facts are read by their label, as the branch view's are
 * (`repo.ts`'s `branchStanding`); the sentences the source wrote are drawn as
 * it wrote them.
 */
import type { SourceView, Tone, ViewRow, ViewSection } from "../../ipc/entity";

/** The page's tabs, in prototype-6's order; *Files changed* is #842's. */
export const PR_TABS = [
  { id: "overview", label: "Overview" },
  { id: "conversation", label: "Conversation" },
  { id: "diff", label: "Files changed" },
  { id: "commits", label: "Commits" },
] as const;

export type PrTab = (typeof PR_TABS)[number]["id"];

const TABS = new Set<string>(PR_TABS.map((tab) => tab.id));

/** What `#/pr/<entity id>/<tab>` names. */
export interface PrRoute {
  /** The pull request's entity id, or `null` for a bare `#/pr`. */
  prId: string | null;
  tab: PrTab;
}

/** Read what follows `pr` in the address; the entity id arrives decoded, as one segment. */
export function readPrRoute(rest: readonly string[]): PrRoute {
  const tab = rest[1] ?? "";
  return { prId: rest[0] || null, tab: (TABS.has(tab) ? tab : "overview") as PrTab };
}

/** The address of the page, or of one of its tabs; Overview is the bare address. */
export function prHash(prId: string, tab: PrTab = "overview"): string {
  const base = `#/pr/${encodeURIComponent(prId)}`;
  return tab === "overview" ? base : `${base}/${tab}`;
}

function section(view: SourceView | null, id: string): ViewSection | undefined {
  return view?.sections.find((candidate) => candidate.id === id);
}

/** The overview's facts, in order. */
function facts(view: SourceView | null) {
  return (section(view, "overview")?.blocks ?? []).flatMap((block) => (block.type === "facts" ? block.facts : []));
}

/** What the header and the Overview tab draw. */
export interface PrHeader {
  state: { text: string; tone: Tone | null } | null;
  author: string | null;
  /** The head branch (a fork's spelled `owner/repo:branch`, as the source names it). */
  head: string | null;
  base: string | null;
  /** The description, as the source holds it; `null` for none. */
  description: string | null;
  /** The head commit's checks, failing first, as the source sorts them. */
  checks: ViewRow[];
}

/** The header and the Overview, from the view; empty while there is none. */
export function headerOf(view: SourceView | null): PrHeader {
  const all = facts(view);
  const value = (label: string) => all.find((candidate) => candidate.label === label);
  const state = value("State");
  // `head → base`, the overview's one fact for both (`overview_facts`).
  const branches = value("Branches")?.value ?? null;
  const at = branches?.indexOf(" → ") ?? -1;
  const blocks = section(view, "overview")?.blocks ?? [];
  const checks = blocks.find((block) => block.type === "entities");
  const text = blocks.find((block) => block.type === "text");
  return {
    state: state ? { text: state.value, tone: state.tone } : null,
    author: value("Author")?.value ?? null,
    head: branches === null ? null : at < 0 ? branches : branches.slice(0, at),
    base: branches === null || at < 0 ? null : branches.slice(at + 3),
    description: text?.type === "text" ? text.text : null,
    checks: checks?.type === "entities" ? checks.rows : [],
  };
}

/** Each tab's count, from its section; `null` where the source gave none. */
export function tabCounts(view: SourceView | null): Record<PrTab, number | null> {
  return {
    overview: null,
    conversation: section(view, "conversation")?.count ?? null,
    diff: section(view, "files")?.count ?? null,
    commits: section(view, "commits")?.count ?? null,
  };
}

/** One line of *Ready to merge?*: its words and its dot's tone. */
export interface ReadyLine {
  text: string;
  tone: Tone;
}

/** The opening letter raised: the line starts a sentence knobas draws. */
function opening(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const LOUDNESS: Tone[] = ["err", "warn", "info", "muted", "ok"];

/**
 * The checks as one line: *all checks passing* when every one is ok, else
 * each that is not, by its name and state, toned by the loudest. A list the
 * source could not read is its one muted line, drawn as written.
 */
export function checksLine(rows: readonly ViewRow[]): ReadyLine | null {
  if (rows.length === 0) return null;
  if (rows.length === 1 && rows[0]!.chip === null) return { text: rows[0]!.title, tone: rows[0]!.tone ?? "muted" };
  const open = rows.filter((row) => row.tone !== "ok");
  if (open.length === 0) return { text: "Checks: all checks passing", tone: "ok" };
  const tone = LOUDNESS.find((candidate) => open.some((row) => (row.tone ?? "muted") === candidate)) ?? "muted";
  return { text: `Checks: ${open.map((row) => `${row.title} ${row.chip ?? ""}`.trim()).join(", ")}`, tone };
}

/**
 * *Ready to merge?* (R-8.16): whether the source says it can merge (its
 * *Mergeable*, which names the approvals still missing, the checks it
 * requires and a conflict), each reviewer's standing verdict, and the
 * checks. Nothing here decides mergeability: the source's `land.rs` does,
 * once, for this and for *Merge…*.
 */
export function readyLines(view: SourceView | null): ReadyLine[] {
  const all = facts(view);
  const lines: ReadyLine[] = [];
  const mergeable = all.find((fact) => fact.label === "Mergeable");
  if (mergeable) lines.push({ text: `Mergeable: ${mergeable.value}`, tone: mergeable.tone ?? "ok" });
  const reviews = all.filter((fact) => fact.label === "Review");
  for (const review of reviews) lines.push({ text: opening(review.value), tone: review.tone ?? "muted" });
  if (reviews.length === 0) lines.push({ text: "No reviews yet", tone: "muted" });
  const checks = checksLine(headerOf(view).checks);
  if (checks) lines.push(checks);
  return lines;
}

/** One thing said in *Conversation*: a comment, or a review with its verdict. */
export interface PrMessage {
  who: string;
  /** When, in the source's words. */
  at: string;
  verdict: { text: string; tone: Tone | null } | null;
  body: string;
}

/**
 * The conversation, oldest first: the source writes each entry as a facts
 * block (who, when, and a review's *Review* verdict) and then its words, if
 * it has any.
 */
export function conversationOf(view: SourceView | null): PrMessage[] {
  const messages: PrMessage[] = [];
  for (const block of section(view, "conversation")?.blocks ?? []) {
    if (block.type === "facts") {
      const [said, ...rest] = block.facts;
      if (!said) continue;
      const review = rest.find((fact) => fact.label === "Review");
      messages.push({
        who: said.label,
        at: said.value,
        verdict: review ? { text: review.value, tone: review.tone } : null,
        body: "",
      });
    } else if (block.type === "text" && messages.length > 0) {
      messages[messages.length - 1]!.body = block.text;
    }
  }
  return messages;
}

/** The pull request's commits, as its *Commits* lists them (the note a cut list ends on included). */
export function commitRows(view: SourceView | null): ViewRow[] {
  for (const block of section(view, "commits")?.blocks ?? []) if (block.type === "entities") return block.rows;
  return [];
}
