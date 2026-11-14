/**
 * A repository's Builds tab (the repo-builds pass, `mockups/builds/repo-builds.md`): pure rules over what
 * `repo_pipelines` answers, the pipelines that built the repository's commits with each one's newest run per branch.
 *
 * The rail lists the branches, each with the worst of its pipelines' newest runs. A branch's page answers the question
 * a branch is opened with: does anything fail here that passes on the default branch?
 */
import type { RepoPipelineRow, RunRow } from "../../ipc/entity";
import { runAt, runTone, toneWord, type GlyphTone } from "../builds/builds";
import { branchName } from "../builds/pipeline";

/** The failures only on the branch shown that the tab draws as problems above its table; the rest lead the table. */
export const PROBLEMS_SHOWN = 2;

/** Each answered row's newest run by branch, as a person names it: built once a row, so the rail's branches × pipelines are lookups. */
const byBranch = new WeakMap<RepoPipelineRow, Map<string, RunRow>>();

/** The pipeline's newest run on `branch`, or `null` where it never ran there. `refs/heads/main` is *main*. */
export function latestOn(row: RepoPipelineRow, branch: string): RunRow | null {
  let runs = byBranch.get(row);
  if (runs === undefined) {
    runs = new Map();
    // `latest` is newest first, so the first run a name meets is its newest, whichever way the CI spelled the ref.
    for (const run of row.latest) {
      const name = run.branch === null ? null : branchName(run.branch);
      if (name !== null && !runs.has(name)) runs.set(name, run);
    }
    byBranch.set(row, runs);
  }
  return runs.get(branch) ?? null;
}

/**
 * What a pipeline's newest run on a branch means beside the default branch's: `only`, a failure the default branch
 * does not share (on the default branch itself, any failure); `also`, one it shares; `fixed`, a pass where the
 * default branch fails; `running` and `queued` while in flight; `pass` otherwise. `null` where it never ran there.
 */
export type Verdict = "only" | "running" | "queued" | "also" | "fixed" | "pass";

export function verdictOf(row: RepoPipelineRow, branch: string, defaultBranch: string): Verdict | null {
  const here = latestOn(row, branch);
  if (here === null) return null;
  const tone = runTone(here);
  const onDefault = branch === defaultBranch ? null : latestOn(row, defaultBranch);
  const defaultFails = onDefault !== null && runTone(onDefault) === "err";
  if (tone === "err") return defaultFails ? "also" : "only";
  if (tone === "run") return "running";
  if (tone === "q") return "queued";
  // A cancelled run fixed nothing: only a run that passed, with or without warnings, is a fix.
  return defaultFails && (tone === "ok" || tone === "warn") ? "fixed" : "pass";
}

/** The order a branch's page lists its pipelines in: what fails only here first, what passes last. */
const ORDER: readonly Verdict[] = ["only", "running", "queued", "also", "fixed", "pass"];

/** A branch's page: the pipelines that ran on it, in {@link ORDER} and else by name, and those that never did. */
export function compareOn(
  rows: readonly RepoPipelineRow[],
  branch: string,
  defaultBranch: string,
): { ran: { row: RepoPipelineRow; verdict: Verdict }[]; notRun: RepoPipelineRow[] } {
  const ran: { row: RepoPipelineRow; verdict: Verdict }[] = [];
  const notRun: RepoPipelineRow[] = [];
  for (const row of rows) {
    const verdict = verdictOf(row, branch, defaultBranch);
    if (verdict === null) notRun.push(row);
    else ran.push({ row, verdict });
  }
  ran.sort((a, b) => ORDER.indexOf(a.verdict) - ORDER.indexOf(b.verdict));
  return { ran, notRun };
}

/** One branch of the rail. */
export interface RailBranch {
  name: string;
  /** The worst of its pipelines' newest runs; `none` where nothing ran on it. */
  tone: GlyphTone;
  /** How many of its pipelines ran on it. */
  ran: number;
  /** How many of its pipelines' newest runs failed. */
  failing: number;
  /** How many of its pipelines' newest runs are still running. */
  running: number;
  /** When its newest run was made, in ms; `-Infinity` where nothing ran on it. */
  lastRun: number;
}

/** Worst first: a failure is what the rail is scanned for, then what still runs. */
const WORST: readonly GlyphTone[] = ["err", "run", "q", "warn", "ok", "muted"];

/**
 * Every branch the repository lists, in its order, then every other branch a run names (one deleted since, say), each
 * with the worst of its pipelines' newest runs. A ref that is not a branch (`refs/pull/12/head`, `refs/tags/v1`) is
 * not one of the rail's.
 */
export function railOf(rows: readonly RepoPipelineRow[], listed: readonly string[]): RailBranch[] {
  const names = [...listed];
  const known = new Set(listed);
  for (const row of rows) {
    for (const run of row.latest) {
      const name = run.branch === null ? null : branchName(run.branch);
      if (name !== null && !name.startsWith("refs/") && !known.has(name)) {
        known.add(name);
        names.push(name);
      }
    }
  }
  return names.map((name) => {
    const runs = rows.map((row) => latestOn(row, name)).filter((run): run is RunRow => run !== null);
    const tones: GlyphTone[] = runs.map(runTone);
    const made = runs.map((run) => Date.parse(runAt(run) ?? "")).filter((ms) => !Number.isNaN(ms));
    return {
      name,
      tone: WORST.find((tone) => tones.includes(tone)) ?? "none",
      ran: runs.length,
      failing: tones.filter((tone) => tone === "err").length,
      running: tones.filter((tone) => tone === "run").length,
      lastRun: made.length > 0 ? Math.max(...made) : Number.NEGATIVE_INFINITY,
    };
  });
}

/**
 * The rail's groups: the default branch, yours (`mine`: the checkout's, those naming the job), those an open pull
 * request is opened from (`pulled`), then every other; each newest run first, those nothing ran on last in the order
 * they came. An empty group is left out.
 */
export function groupBranches(
  rail: readonly RailBranch[],
  defaultBranch: string,
  mine: readonly string[],
  pulled: readonly string[] = [],
): { head: string; branches: RailBranch[] }[] {
  const newestFirst = (branches: readonly RailBranch[]) =>
    branches
      .map((b, at) => ({ b, at }))
      .sort((x, y) => (x.b.lastRun === y.b.lastRun ? x.at - y.at : y.b.lastRun - x.b.lastRun))
      .map(({ b }) => b);
  const def = rail.filter((b) => b.name === defaultBranch);
  const yours = newestFirst(rail.filter((b) => b.name !== defaultBranch && mine.includes(b.name)));
  const rest = rail.filter((b) => b.name !== defaultBranch && !mine.includes(b.name));
  const pulls = newestFirst(rest.filter((b) => pulled.includes(b.name)));
  const other = newestFirst(rest.filter((b) => !pulled.includes(b.name)));
  return [
    { head: "Default branch", branches: def },
    { head: "Yours", branches: yours },
    { head: "Pull requests", branches: pulls },
    { head: "Other branches", branches: other },
  ].filter((group) => group.branches.length > 0);
}

/**
 * Your branches: the one the checkout is on, then every branch naming the job's key (`feat/PAY-231-retry`), ignoring
 * case. The default branch is nobody's.
 */
export function yoursOf(
  names: readonly string[],
  job: string | null,
  checkout: string | null,
  defaultBranch: string | null = null,
): string[] {
  const key = job?.toLowerCase() ?? null;
  const mine = [...(checkout !== null ? [checkout] : []), ...names.filter((name) => key !== null && name.toLowerCase().includes(key))];
  return [...new Set(mine)].filter((name) => name !== defaultBranch);
}

/**
 * `items` in the order they first drew (`docs/decisions/2026-11-11-rows-stay-where-they-drew.md`): `held` remembers
 * each key's place the first time it is met, so a read again that brings a new failure or a newer run moves no row
 * under the pointer; a key never met before joins the end.
 */
export function holdOrder<T>(held: Map<string, number>, items: readonly T[], key: (item: T) => string): T[] {
  for (const item of items) if (!held.has(key(item))) held.set(key(item), held.size);
  return [...items].sort((a, b) => held.get(key(a))! - held.get(key(b))!);
}

/**
 * A branch's builds in words, one way wherever a branch is drawn (the rail's titles, the Branches tab's *Builds*
 * column): what failed, else what still runs, else that every one passed; *No builds* where nothing ran on it.
 */
export function branchWords(branch: RailBranch): string {
  if (branch.ran === 0) return "No builds";
  if (branch.failing > 0) return `${branch.failing} of ${branch.ran} failing`;
  if (branch.running > 0) return `${branch.running} running`;
  if (branch.tone === "ok") return `All ${branch.ran} passed`;
  return toneWord(branch.tone);
}
