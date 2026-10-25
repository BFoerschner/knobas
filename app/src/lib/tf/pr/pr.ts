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
import type { SourceView, Tone, ViewAction, ViewBlock, ViewRow, ViewSection } from "../../ipc/entity";

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

/**
 * A muted line (#2065): what a source writes where something could not be
 * read, or to say what a list leaves out, as Gitea's `view::muted_line` does,
 * an untitled list whose every row is muted and names no entity and no chip.
 * The page draws it as a line where the source put it, never as a check, a
 * commit or a message. A list's own last note (a cut list's *The first 250
 * of 312*) sits in a list of what it lists, so it stays in that list.
 */
function lineOf(block: ViewBlock): string[] | null {
  if (block.type !== "entities" || block.title !== null || block.rows.length === 0) return null;
  const quiet = block.rows.every((row) => row.tone === "muted" && row.entity_id === null && row.chip === null);
  return quiet ? block.rows.map((row) => row.title) : null;
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
 * *Ready to merge?* (R-8.16, #994): prototype-6's three lines, each with its
 * dot: *Approvals: 1 of 2* and the conflicts line from the facts the source
 * declares for them (Gitea's `land.rs`), and the checks between them. A
 * conflict fact reads `none with <base>` or `conflicts with <base>`, drawn
 * *No conflicts with main* or *Conflicts with main*.
 *
 * A source that declares neither is drawn its *Mergeable* sentence as
 * written, then the checks. Nothing here decides mergeability: the source
 * does, once, for this and for *Merge…*.
 */
export function readyLines(view: SourceView | null): ReadyLine[] {
  const all = facts(view);
  const approvals = all.find((fact) => fact.label === "Approvals");
  const conflicts = all.find((fact) => fact.label === "Conflicts");
  const checks = checksLine(headerOf(view).checks);
  const lines: ReadyLine[] = [];
  if (approvals === undefined && conflicts === undefined) {
    const mergeable = all.find((fact) => fact.label === "Mergeable");
    if (mergeable) lines.push({ text: `Mergeable: ${mergeable.value}`, tone: mergeable.tone ?? "ok" });
    if (checks && lines.length > 0) lines.push(checks);
    return lines;
  }
  if (approvals) lines.push({ text: `Approvals: ${approvals.value}`, tone: approvals.tone ?? "muted" });
  if (checks) lines.push(checks);
  if (conflicts) {
    const none = conflicts.value.startsWith("none ");
    const text = none ? `No conflicts ${conflicts.value.slice("none ".length)}` : opening(conflicts.value);
    lines.push({ text, tone: conflicts.tone ?? (none ? "ok" : "warn") });
  }
  return lines;
}

/**
 * Why *Merge…* cannot be taken now, one reason a line, as its source wrote
 * them: Gitea's `land.rs` joins its reasons with `; ` into the one sentence
 * the toolbar's tooltip shows (#843, R-8.19).
 */
export function mergeReasons(merge: ViewAction | null): string[] {
  const sentence = merge?.disabled ?? "";
  return sentence
    .split("; ")
    .map((reason) => reason.trim())
    .filter((reason) => reason !== "");
}

/** The verdicts a review can carry, in prototype-6's order. */
export type Verdict = "comment" | "approve" | "request_changes";

/** One choice of *Submit review* (R-8.18): its verdict, its words, and the source's action that sends it. */
export interface ReviewChoice {
  verdict: Verdict;
  label: string;
  /** Prototype-6's one-line meaning. */
  meaning: string;
  /** The source's action for this verdict; `null` where it cannot be sent now (a comment with no line comments, a write the credential cannot make). */
  action: ViewAction | null;
  /** Why it cannot be sent now, where `action` is `null`: the source's refusal (#2030), else the line comment it waits for. */
  why: string | null;
}

const CHOICES: { verdict: Verdict; label: string; meaning: string }[] = [
  { verdict: "comment", label: "Comment", meaning: "Feedback without a verdict" },
  { verdict: "approve", label: "Approve", meaning: "Counts toward the approvals {base} needs" },
  { verdict: "request_changes", label: "Request changes", meaning: "Blocks the merge until you approve" },
];

/**
 * The verdicts *Submit review* offers, from the source's `submit_review`
 * actions (Gitea's `review.rs`): a verdict it does not offer is not drawn
 * (one's own pull request takes no approval), and its comment-only review
 * (`line_comments: requires`) is drawn without an action while no line
 * comment waits, since the source would refuse it. A verdict the source
 * draws disabled (a token that may not write, #2030) is drawn without an
 * action too, saying why.
 */
export function reviewChoices(actions: readonly ViewAction[], pending: number, base: string | null = null): ReviewChoice[] {
  const reviews = actions.filter((action) => action.write_op === "submit_review");
  const choices: ReviewChoice[] = [];
  for (const choice of CHOICES) {
    const action = reviews.find((candidate) => candidate.values["verdict"] === choice.verdict);
    if (action === undefined) continue;
    const why = action.disabled ?? (action.line_comments === "requires" && pending === 0 ? "Add a line comment first" : null);
    const meaning = choice.meaning.replace("{base}", base ?? "the base");
    choices.push({ ...choice, meaning, action: why === null ? action : null, why });
  }
  return choices;
}

/** One thing said in *Conversation*: a comment, or a review with its verdict. */
export interface PrMessage {
  who: string;
  /** When, in the source's words. */
  at: string;
  verdict: { text: string; tone: Tone | null } | null;
  body: string;
}

/** What a tab draws of its section, in the source's order: a muted line, or one of the tab's own things. */
export type Part<T> = { line: string } | { item: T };

/**
 * The conversation, oldest first: the source writes each entry as a facts
 * block (who, when, and a review's *Review* verdict) and then its words, if
 * it has any; a muted line (comments the token may not read, #2042) stands
 * where the source put it.
 */
export function conversationParts(view: SourceView | null): Part<PrMessage>[] {
  const parts: Part<PrMessage>[] = [];
  for (const block of section(view, "conversation")?.blocks ?? []) {
    const last = parts.at(-1);
    if (block.type === "facts") {
      const [said, ...rest] = block.facts;
      if (!said) continue;
      const review = rest.find((fact) => fact.label === "Review");
      parts.push({
        item: {
          who: said.label,
          at: said.value,
          verdict: review ? { text: review.value, tone: review.tone } : null,
          body: "",
        },
      });
    } else if (block.type === "text" && last !== undefined && "item" in last) {
      last.item.body = block.text;
    } else {
      for (const line of lineOf(block) ?? []) parts.push({ line });
    }
  }
  return parts;
}

/** The conversation's messages alone, oldest first. */
export function conversationOf(view: SourceView | null): PrMessage[] {
  return conversationParts(view).flatMap((part) => ("item" in part ? [part.item] : []));
}

/** The pull request's commits, as its *Commits* lists them (the note a cut list ends on included). */
export function commitRows(view: SourceView | null): ViewRow[] {
  for (const block of section(view, "commits")?.blocks ?? []) if (block.type === "entities") return block.rows;
  return [];
}
