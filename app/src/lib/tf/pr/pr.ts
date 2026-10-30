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
function linesOf(block: ViewBlock): Line[] | null {
  if (block.type !== "entities" || block.title !== null || block.rows.length === 0) return null;
  const quiet = block.rows.every((row) => row.tone === "muted" && row.entity_id === null && row.chip === null);
  return quiet ? block.rows.map((row) => ({ line: row.title })) : null;
}

/** A muted line as a tab draws it: its words. */
export interface Line {
  line: string;
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
  const checks = blocks.find((block) => block.type === "entities" && linesOf(block) === null);
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

/** What the Overview tab draws, in the source's order: the checks, the description, and a muted line where the source put one. */
export type OverviewPart = Line | { checks: ViewRow[] } | { description: string };

export function overviewParts(view: SourceView | null): OverviewPart[] {
  const parts: OverviewPart[] = [];
  let seenChecks = false;
  let seenDescription = false;
  for (const block of section(view, "overview")?.blocks ?? []) {
    const lines = linesOf(block);
    if (lines !== null) parts.push(...lines);
    else if (block.type === "entities" && !seenChecks) {
      seenChecks = true;
      if (block.rows.length > 0) parts.push({ checks: block.rows });
    } else if (block.type === "text" && !seenDescription) {
      seenDescription = true;
      parts.push({ description: block.text });
    }
  }
  return parts;
}

/**
 * The rows *Ready to merge?* reads its checks line from: the checks, else,
 * where the overview opens its lists on a muted line, that line, which
 * stands where the checks would (Gitea's `checks::unread`).
 */
function checksPlace(view: SourceView | null): ViewRow[] {
  const checks = headerOf(view).checks;
  if (checks.length > 0) return checks;
  const first = (section(view, "overview")?.blocks ?? []).find((block) => block.type === "entities");
  return first?.type === "entities" && linesOf(first) !== null ? first.rows : [];
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

/** One line of *Ready to merge?*: its words and its dot's tone, and what stands in its way where the page can name it. */
export interface ReadyLine {
  text: string;
  tone: Tone;
  why?: string;
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
  const checks = checksLine(checksPlace(view));
  const lines: ReadyLine[] = [];
  if (approvals === undefined && conflicts === undefined) {
    const mergeable = all.find((fact) => fact.label === "Mergeable");
    if (mergeable) lines.push({ text: `Mergeable: ${mergeable.value}`, tone: mergeable.tone ?? "ok" });
    if (checks && lines.length > 0) lines.push(checks);
    return lines;
  }
  if (approvals) lines.push({ text: `Approvals: ${approvals.value}`, tone: approvals.tone ?? "muted" });
  const required = all.find((fact) => fact.label === "Required checks");
  if (required) lines.push(requiredLine(view, required.value, required.tone ?? "muted"));
  else if (checks) lines.push(checks);
  const said = conflictsOf(view);
  if (conflicts && said) lines.push({ text: said.text, tone: conflicts.tone ?? (said.conflicts ? "warn" : "ok") });
  return lines;
}

/**
 * *Required checks: 1 of 3 pass*, and each required check that has not
 * passed by its name and state. Whether one passed is its *Required check*
 * fact's tone where the source gives one (Gitea passes a `skipped` status,
 * which its checks list leaves untoned), else its row's.
 */
function requiredLine(view: SourceView | null, value: string, tone: Tone): ReadyLine {
  const open = railChecks(view).filter((check) => check.required && check.standing !== "ok");
  const line: ReadyLine = { text: `Required checks: ${value}`, tone };
  if (open.length > 0) line.why = open.map((check) => `${check.row.title} ${check.row.chip ?? ""}`.trim()).join(", ");
  return line;
}

/**
 * One check in the side rail (#2278): the source's row, whether the base
 * requires it, and where it stands: a required check's *Required check*
 * fact's tone where the source gives one (Gitea fails a `warning` and passes
 * a `skipped` status, which its checks list tones warn and not at all), else
 * the row's.
 */
export interface RailCheck {
  row: ViewRow;
  required: boolean;
  standing: Tone | null;
}

const CHECK_RANK: Tone[] = ["err", "warn", "info", "ok"];

/**
 * The side rail's *Checks* (#2278): the head commit's checks, failing first
 * as the source sorts them and, within one state, the ones the base
 * requires first (Gitea's *Required check* facts, `land.rs`), so a failing
 * required check is the top line. A required check that never reported is
 * drawn last, *not reported*; the source names it by its pattern.
 */
export function railChecks(view: SourceView | null): RailCheck[] {
  const required = facts(view).filter((fact) => fact.label === "Required check");
  const rows = headerOf(view).checks;
  const rank = (check: RailCheck) => {
    const at = check.standing === null ? -1 : CHECK_RANK.indexOf(check.standing);
    return (at < 0 ? CHECK_RANK.length : at) * 2 + (check.required ? 0 : 1);
  };
  const drawn = rows
    .map((row): RailCheck => {
      const fact = required.find((candidate) => candidate.value === row.title);
      return { row, required: fact !== undefined, standing: fact?.tone ?? row.tone };
    })
    .sort((a, b) => rank(a) - rank(b));
  const unreported = required
    .filter((fact) => !rows.some((row) => row.title === fact.value))
    .map(
      (fact): RailCheck => ({
        row: { entity_id: null, title: fact.value, meta: null, tone: "info", chip: "not reported", url: null },
        required: true,
        standing: "info",
      }),
    );
  return [...drawn, ...unreported];
}

/** *Ready to merge?*'s verdict: its words, its tone, and the source's reason where *Merge…* is refused. */
export interface ReadyVerdict {
  text: string;
  tone: Tone;
  why: string | null;
}

/**
 * *Ready to merge?* as the side rail draws it (#2278): its lines and, where
 * the source offers *Merge…*, a verdict over them. Ready only where every
 * line is met and the source does not refuse the merge: *Merge…*'s reason is
 * the source's one decision (`land.rs`), so a line the page cannot draw (a
 * head behind a protected base) still holds the verdict back, saying why.
 */
export function readiness(view: SourceView | null): { verdict: ReadyVerdict | null; lines: ReadyLine[] } {
  const lines = readyLines(view);
  const merge = view?.actions.find((action) => action.write_op === "merge_pull_request") ?? null;
  if (merge === null || lines.length === 0) return { verdict: null, lines };
  const met = lines.filter((line) => line.tone === "ok").length;
  const counted = `${met} of ${lines.length} met`;
  if (met === lines.length && merge.disabled === null) return { verdict: { text: `Ready: ${counted}`, tone: "ok", why: null }, lines };
  const tone: Tone = lines.some((line) => line.tone === "err") ? "err" : "warn";
  return { verdict: { text: `Not yet: ${counted}`, tone, why: merge.disabled }, lines };
}

/** One reviewer in the side rail and where their review stands. */
export interface Reviewer {
  who: string;
  state: string;
  tone: Tone | null;
}

/**
 * The reviewers (#2278), from the overview's *Review* facts: each
 * reviewer's standing verdict as Gitea's `verdicts` writes it, `approved by
 * sven`, `changes requested by jonas`, `requested from priya`.
 */
export function reviewersOf(view: SourceView | null): Reviewer[] {
  return facts(view)
    .filter((fact) => fact.label === "Review")
    .flatMap((fact): Reviewer[] => {
      if (fact.value.startsWith("requested from ")) {
        return [{ who: fact.value.slice("requested from ".length), state: "Review requested", tone: fact.tone }];
      }
      const at = fact.value.lastIndexOf(" by ");
      if (at < 0) return [];
      return [{ who: fact.value.slice(at + 4), state: opening(fact.value.slice(0, at)), tone: fact.tone }];
    });
}

/** The side rail's *Conflicts*: whether the head conflicts with its base, in the source's words; `null` while it has not said. */
export function conflictsOf(view: SourceView | null): { conflicts: boolean; text: string } | null {
  const fact = facts(view).find((candidate) => candidate.label === "Conflicts");
  if (fact === undefined) return null;
  const none = fact.value.startsWith("none ");
  return { conflicts: !none, text: none ? `No conflicts ${fact.value.slice("none ".length)}` : opening(fact.value) };
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

/**
 * The conversation, oldest first: the source writes each entry as a facts
 * block (who, when, and a review's *Review* verdict) and then its words, if
 * it has any; a muted line (comments the token may not read, #2042) stands
 * where the source put it.
 */
export function conversationParts(view: SourceView | null): (Line | { message: PrMessage })[] {
  const parts: (Line | { message: PrMessage })[] = [];
  for (const block of section(view, "conversation")?.blocks ?? []) {
    const last = parts.at(-1);
    if (block.type === "facts") {
      const [said, ...rest] = block.facts;
      if (!said) continue;
      const review = rest.find((fact) => fact.label === "Review");
      parts.push({
        message: {
          who: said.label,
          at: said.value,
          verdict: review ? { text: review.value, tone: review.tone } : null,
          body: "",
        },
      });
    } else if (block.type === "text" && last !== undefined && "message" in last) {
      last.message.body = block.text;
    } else {
      parts.push(...(linesOf(block) ?? []));
    }
  }
  return parts;
}

/** The conversation's messages alone, oldest first. */
export function conversationOf(view: SourceView | null): PrMessage[] {
  return conversationParts(view).flatMap((part) => ("message" in part ? [part.message] : []));
}

/**
 * What *Commits* draws, in the source's order: each list of the pull
 * request's commits (the note a cut list ends on included), and a muted line
 * where the source put one, which is none of them.
 */
export function commitParts(view: SourceView | null): (Line | { commits: ViewRow[] })[] {
  return (section(view, "commits")?.blocks ?? []).flatMap((block): (Line | { commits: ViewRow[] })[] => {
    const lines = linesOf(block);
    if (lines !== null) return lines;
    return block.type === "entities" && block.rows.length > 0 ? [{ commits: block.rows }] : [];
  });
}
