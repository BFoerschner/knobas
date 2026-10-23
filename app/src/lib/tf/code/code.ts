/**
 * Code's pure half (#837, R-8.1, R-8.2): how pull requests group, what a
 * repository row says, and which rows the two filter bars keep.
 *
 * Over `list_entities`, `entity_checkout` and `code_facts` only. Nothing
 * here reads a payload: a pull request's group is the attention read's
 * (#712), its state the declared chip, its repository the declared parent
 * path, its author and a repository's team and language `code_facts` (#966).
 */
import type { AttentionGroup, CheckoutView, CodeFacts, EntityPage, EntityRow, WorkingTreeState } from "../../ipc/entity";
import type { SourceSummary } from "../../ipc/sources";
import { matchesAll } from "../filter/filter";

/** `code_facts`' answer by entity id. */
export type FactsById = ReadonlyMap<string, CodeFacts>;

/** `code_facts`' answer keyed by entity id, for the reads below. */
export function factsById(facts: readonly CodeFacts[]): FactsById {
  return new Map(facts.map((entry) => [entry.entity_id, entry]));
}

/** A pull request's group on the *Pull requests* tab, in draw order. */
export type PrGroup = "review" | "yours" | "others" | "settled";

/** Each group's header, in prototype-6's words. */
export const GROUP_LABEL: Record<PrGroup, string> = {
  review: "Asking for your review",
  yours: "Yours",
  others: "Open, others",
  settled: "Merged or closed",
};

export const GROUP_ORDER: readonly PrGroup[] = ["review", "yours", "others", "settled"];

const FROM_ATTENTION: Partial<Record<AttentionGroup, PrGroup>> = {
  review_requested: "review",
  my_pull_requests: "yours",
  open_pull_requests: "others",
};

/** How long an open pull request may sit before it is *Idle > 5 days*. */
const IDLE_MS = 5 * 86_400_000;

/** One pull request as the tab draws it. */
export interface PrLine {
  row: EntityRow;
  /**
   * Its group, or `null` for an open pull request when no source has a
   * configured identity, so the attention read could not say whose it is.
   */
  group: PrGroup | null;
  /** `#142`, from the id's `#<number>` key, or `null` for an id without one. */
  number: string | null;
  /** Open, and nothing happened to it for more than five days. */
  idle: boolean;
  /** Who opened it, as the source spells the account, or `null` before `code_facts` answers or on a miss. */
  author: string | null;
}

/** Whether a group is *Mine*: asking your review, or yours. */
function isMineGroup(group: PrGroup | null): boolean {
  return group === "review" || group === "yours";
}

/** `#142` from `gitea:tidewater/payout-service#142`. */
export function prNumber(entityId: string): string | null {
  const found = /#(\d+)$/.exec(entityId);
  return found ? `#${found[1]}` : null;
}

/**
 * The tab's rows: first the attention read's (#712), which holds the open
 * pull requests group by group in its own order, then every other pull
 * request the recency read holds (merged or closed, which attention drops as
 * settled), newest first. A settled one whose author is mine (`code_facts`,
 * #966) is under *Yours*, as prototype-6 groups `p.by===ME`.
 */
export function prLines(attention: EntityPage, all: EntityPage, now: Date, facts: FactsById = new Map()): PrLine[] {
  const groupOf: (PrGroup | null)[] = [];
  for (const tally of attention.groups ?? []) {
    for (let n = 0; n < tally.count; n += 1) groupOf.push(FROM_ATTENTION[tally.group] ?? null);
  }
  // The rows arrive group by group, so the n-th row is in the n-th slot.
  const open = attention.rows
    .map((row, at) => line(row, attention.groups ? (groupOf[at] ?? null) : null, now, true, facts))
    .filter((entry) => entry.row.kind === "pr");
  const seen = new Set(open.map((entry) => entry.row.entity_id));
  const settled = all.rows
    .filter((row) => row.kind === "pr" && !seen.has(row.entity_id))
    .map((row) => line(row, facts.get(row.entity_id)?.mine ? "yours" : "settled", now, false, facts));
  return [...open, ...settled];
}

function line(row: EntityRow, group: PrGroup | null, now: Date, open: boolean, facts: FactsById): PrLine {
  const at = row.updated_at ? new Date(row.updated_at).getTime() : Number.NaN;
  return {
    row,
    group,
    number: prNumber(row.entity_id),
    idle: open && !Number.isNaN(at) && now.getTime() - at > IDLE_MS,
    author: facts.get(row.entity_id)?.author ?? null,
  };
}

/** The *Pull requests* tab's chips, in prototype-6's words. */
export const PR_STATES = ["Open", "Merged", "Closed"] as const;
export const PR_FLAGS = ["Waiting on me", "Idle > 5 days"] as const;

/** What the *Pull requests* filter bar has set. */
export interface PrFilter {
  scope: "job" | "mine" | "all";
  /** Whether the current job holds an entity: what *This job* keeps. */
  held: (entityId: string) => boolean;
  /** The state chips on; none keeps every state. */
  states: readonly string[];
  flags: readonly string[];
  /** The *Repo* dropdown's value, a repository's full name, or `""` for *Any*. */
  repo: string;
  /** The *Team* dropdown's value, or `""` for *Any*: the pull request's repository is in that team. */
  team: string;
  text: string;
}

/** Each repository's declared teams, by {@link repoKey}: what a pull request's *Team* is (#966). */
export function repoTeams(repos: readonly EntityRow[], facts: FactsById): Map<string, readonly string[]> {
  return new Map(repos.map((row) => [repoKey(row.source_id, row.title), facts.get(row.entity_id)?.teams ?? []]));
}

/**
 * The pull requests the filter keeps, in the order given. *Mine* is what
 * asks for your review and what is yours (prototype-6: `p.by===ME||p.wait`).
 * The words reach the number, title, repository and author.
 */
export function visiblePrs(
  lines: readonly PrLine[],
  filter: PrFilter,
  teams: ReadonlyMap<string, readonly string[]>,
): PrLine[] {
  const states = filter.states.map((state) => state.toLowerCase());
  return lines.filter(({ row, group, number, idle, author }) => {
    if (filter.scope === "job" && !filter.held(row.entity_id)) return false;
    if (filter.scope === "mine" && !isMineGroup(group)) return false;
    if (states.length > 0 && !states.includes(row.state?.text.toLowerCase() ?? "")) return false;
    if (filter.flags.includes("Waiting on me") && group !== "review") return false;
    if (filter.flags.includes("Idle > 5 days") && !idle) return false;
    if (filter.repo !== "" && row.path !== filter.repo) return false;
    if (filter.team !== "") {
      const of = row.path === null ? [] : (teams.get(repoKey(row.source_id, row.path)) ?? []);
      if (!of.includes(filter.team)) return false;
    }
    return matchesAll(`${number ?? ""} ${row.title} ${row.path ?? ""} ${author ?? ""}`, filter.text);
  });
}

/**
 * The *Local checkout* cell: the branch checked out and where it stands
 * (`main, clean`; `feature/x, 3 ahead, 1 changed`), the path when git could
 * not be read there, *not on this machine* with no clone, and `null` while
 * `entity_checkout` has not answered.
 */
export function checkoutLine(view: CheckoutView | undefined): { text: string; dim: boolean } | null {
  if (view === undefined) return null;
  if (view.path === null) return { text: "not on this machine", dim: true };
  const tree = view.tree;
  if (tree === null || tree.status !== "read") return { text: view.path, dim: false };
  return { text: `${tree.branch ?? "detached"}, ${treeStanding(tree)}`, dim: false };
}

/** Where a working tree stands, without its branch: `3 ahead, 1 changed`, or `clean`. */
export function treeStanding(tree: Pick<WorkingTreeState, "ahead" | "behind" | "changed" | "untracked">): string {
  const changed = tree.changed + tree.untracked;
  const parts = [
    tree.ahead > 0 ? `${tree.ahead} ahead` : null,
    tree.behind > 0 ? `${tree.behind} behind` : null,
    changed > 0 ? `${changed} changed` : null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? parts.join(", ") : "clean";
}

/** A repository's key among pull requests: its source and its full name, which a pull request's parent path is. */
function repoKey(sourceId: string, name: string): string {
  return `${sourceId} ${name}`;
}

/** How many open pull requests each repository has, by {@link repoKey}. */
export function openPrsOf(lines: readonly PrLine[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const { row } of lines) {
    if (row.path === null || row.state?.text.toLowerCase() !== "open") continue;
    const key = repoKey(row.source_id, row.path);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

/** A repository's open pull requests. */
export function openCount(counts: Map<string, number>, row: EntityRow): number {
  return counts.get(repoKey(row.source_id, row.title)) ?? 0;
}

/** The *Repositories* tab's chips, in prototype-6's words. */
export const REPO_FLAGS = ["Cloned here", "Has open PRs"] as const;

/** What the *Repositories* filter bar has set. */
export interface RepoFilter {
  scope: "job" | "mine" | "all";
  held: (entityId: string) => boolean;
  flags: readonly string[];
  /** The *Team* dropdown's value, or `""` for *Any*. */
  team: string;
  /** The *Language* dropdown's value, or `""` for *Any*. */
  language: string;
  text: string;
}

/** The *Team* and *Language* dropdowns' choices: every value the repositories declare, sorted. */
export function repoChoices(repos: readonly EntityRow[], facts: FactsById): { teams: string[]; languages: string[] } {
  const known = repos.map((row) => facts.get(row.entity_id)).filter((entry) => entry !== undefined);
  const sorted = (values: string[]) => [...new Set(values)].sort((a, b) => a.localeCompare(b));
  return {
    teams: sorted(known.flatMap((entry) => entry.teams)),
    languages: sorted(known.flatMap((entry) => (entry.language === null ? [] : [entry.language]))),
  };
}

/**
 * The repositories the filter keeps: yours first (a pull request of yours, or
 * one asking your review, is in it), then by last push, newest first. The
 * words reach the name, the teams and the language.
 */
export function visibleRepos(
  rows: readonly EntityRow[],
  lines: readonly PrLine[],
  checkouts: Readonly<Record<string, CheckoutView>>,
  facts: FactsById,
  filter: RepoFilter,
): EntityRow[] {
  const open = openPrsOf(lines);
  const mine = new Set(
    lines
      .filter((line) => isMineGroup(line.group) && line.row.path !== null)
      .map((line) => repoKey(line.row.source_id, line.row.path!)),
  );
  const isMine = (row: EntityRow) => mine.has(repoKey(row.source_id, row.title));
  return rows
    .filter((row) => {
      if (filter.scope === "job" && !filter.held(row.entity_id)) return false;
      if (filter.scope === "mine" && !isMine(row)) return false;
      if (filter.flags.includes("Cloned here") && !checkouts[row.entity_id]?.path) return false;
      if (filter.flags.includes("Has open PRs") && openCount(open, row) === 0) return false;
      const known = facts.get(row.entity_id);
      if (filter.team !== "" && !known?.teams.includes(filter.team)) return false;
      if (filter.language !== "" && known?.language !== filter.language) return false;
      return matchesAll(`${row.title} ${known?.teams.join(" ") ?? ""} ${known?.language ?? ""}`, filter.text);
    })
    .sort(
      (a, b) => Number(isMine(b)) - Number(isMine(a)) || (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
    );
}

/**
 * Where *New pull request* goes (R-8.2): the compare page for the job's
 * branch, in the repository it belongs to (the first branch the job reaches
 * whose repository is mirrored), or `null` when the job has none.
 */
export function compareHash(branches: readonly EntityRow[], repos: readonly EntityRow[]): string | null {
  for (const branch of branches) {
    if (branch.kind !== "branch" || branch.path === null) continue;
    const repo = repos.find((row) => row.source_id === branch.source_id && row.title === branch.path);
    if (repo) return `#/repo/${encodeURIComponent(repo.entity_id)}/compare/${encodeURIComponent(branch.title)}`;
  }
  return null;
}

/**
 * The header's line: how many repositories, and the host they are on when
 * they all come from one source with a readable URL.
 */
export function headerLine(
  total: number,
  repos: readonly EntityRow[],
  sources: readonly Pick<SourceSummary, "id" | "base_url">[],
): string {
  const noun = total === 1 ? "repository" : "repositories";
  const ids = [...new Set(repos.map((row) => row.source_id))];
  const url = ids.length === 1 ? sources.find((source) => source.id === ids[0])?.base_url : null;
  let host: string | null = null;
  try {
    host = url ? new URL(url).host : null;
  } catch {
    host = null;
  }
  return host ? `${total} ${noun} on ${host}, what asks for you first.` : `${total} ${noun}, what asks for you first.`;
}
