/**
 * The run page's rules (#853, R-9.9 to R-9.11): pure functions over the run
 * rows `pipeline_runs` answers, the run's source view (`entity_view`, today's
 * build view) and the pipeline's settings, so every one is a test on plain
 * data.
 *
 * Nothing here knows a source. The run's own fields (who started it, its
 * agent, what it is doing now, its commit) are the declared ones on its row
 * (#1004); what the view says is read by its shape: the pipeline as the row
 * whose id is one of the declared pipelines, the tabs by their section ids.
 */
import type { PipelineSettings, RunRow, SourceView, ViewRow, ViewSection } from "../../ipc/entity";
import { duration, runHash, runTone } from "./builds";
import { branchName, defaultBranch, shortSha } from "./pipeline";
import { fieldsOf, overridesOf, runPayload, valuesFrom, type RunRequest, type StartValue } from "./run-dialog";

/** The run page's tabs, in prototype-6's order and words; `overview` is the default. */
export const RUN_TABS = [
  { id: "overview", name: "Overview" },
  { id: "log", name: "Log" },
  { id: "tests", name: "Tests" },
  { id: "artifacts", name: "Artifacts" },
  { id: "changes", name: "Changes" },
  { id: "params", name: "Parameters" },
] as const;
export type RunTab = (typeof RUN_TABS)[number]["id"];

/** The tab an address names: `#/build/<pipeline>/<run>/<tab>`; anything else is Overview. */
export function runTabOf(rest: readonly string[]): RunTab {
  const asked = rest[2];
  return RUN_TABS.find((tab) => tab.id === asked)?.id ?? "overview";
}

/** A tab's address. Overview is the run's own. */
export function runTabHash(pipelineId: string, runId: string, tab: RunTab): string {
  const base = runHash(pipelineId, runId);
  return tab === "overview" ? base : `${base}/${tab}`;
}

/** The runs older than `run`, from runs newest first. */
function olderThan(runs: readonly RunRow[], run: RunRow): readonly RunRow[] {
  return runs.slice(runs.findIndex((r) => r.entity_id === run.entity_id) + 1);
}

/** The next older finished run on the same branch, from runs newest first. */
export function previousOnBranch(runs: readonly RunRow[], run: RunRow): RunRow | null {
  return olderThan(runs, run).find((r) => r.branch === run.branch && r.finished !== null) ?? null;
}

/** *Took*: *4 m 12 s, −16 s against #1185*; a run in flight is *still running*. */
export function tookLine(run: RunRow, previous: RunRow | null): string {
  if (run.duration_seconds === null) return run.started !== null ? "still running" : "not started";
  const took = duration(run.duration_seconds);
  if (previous === null || previous.duration_seconds === null) return took;
  const diff = run.duration_seconds - previous.duration_seconds;
  return `${took}, ${diff > 0 ? "+" : "−"}${duration(Math.abs(diff))} against #${previous.number ?? ""}`;
}

/** How a finished run went, for the context line: `null` for one that neither passed nor failed. */
function went(run: RunRow): "passed" | "failed" | null {
  const tone = runTone(run);
  return tone === "ok" ? "passed" : tone === "err" ? "failed" : null;
}

/**
 * The line that puts a run in context (R-9.11): how the run before it on its
 * branch went, then the default branch's last run before it, when the run is
 * on another branch. *Also failed in #1185 on this branch. #1186 on main passed.*
 * It is about the runs: which test failed in them is the Tests tab's (#855).
 */
export function contextLine(runs: readonly RunRow[], run: RunRow, mainBranch: string | null): string {
  const parts: string[] = [];
  const before = previousOnBranch(runs, run);
  const was = before === null ? null : went(before);
  if (before !== null && was === "failed" && went(run) === "failed") parts.push(`Also failed in #${before.number} on this branch.`);
  else if (before !== null && was !== null) parts.push(`#${before.number} on this branch ${was}.`);
  if (mainBranch !== null && run.branch !== null && branchName(run.branch) !== mainBranch) {
    const main = olderThan(runs, run).find((r) => r.branch !== null && branchName(r.branch) === mainBranch && went(r) !== null);
    if (main !== undefined) parts.push(`#${main.number} on ${mainBranch} ${went(main)}.`);
  }
  return parts.join(" ");
}

/** The first failed test, as the Overview leads with it. */
export interface FirstFailure {
  name: string;
  /** What the source says about it: the assertion, a stack. */
  details: string | null;
  /** How many tests failed. */
  count: number;
  /** The file and line the details name, for *Open the test file*. */
  location: { path: string; line: number } | null;
}

/**
 * The first failed test of the view's *Tests* section: a row in err tone
 * (a muted test is not a failure), and the text block after it as its
 * details. `null` with none.
 */
export function firstFailure(section: ViewSection | null): FirstFailure | null {
  if (section === null) return null;
  const blocks = section.blocks;
  let failed = 0;
  let first: { name: string; at: number } | null = null;
  blocks.forEach((block, at) => {
    if (block.type !== "entities") return;
    for (const row of block.rows) {
      if (row.tone !== "err") continue;
      failed += 1;
      if (first === null) first = { name: row.title, at };
    }
  });
  if (first === null) return null;
  const { name, at } = first;
  const next = blocks[at + 1];
  const details = next?.type === "text" ? next.text : null;
  return { name, details, count: Math.max(section.count ?? 0, failed), location: locationIn(details) };
}

/** The problem's heading: *1 test failed: <name>*, or how many and the first. */
export function failureHeading(failure: FirstFailure): string {
  if (failure.count <= 1) return `1 test failed: ${failure.name}`;
  return `${failure.count} tests failed. The first: ${failure.name}`;
}

/**
 * The first `path/to/file.ext:line` in a failure's details: a path with a
 * directory, so a stack frame's bare `B.java:31` (a class, not a path) is no
 * location.
 */
export function locationIn(details: string | null): { path: string; line: number } | null {
  const match = /([\w.-]+(?:\/[\w.-]+)+\.\w+):(\d+)/.exec(details ?? "");
  return match === null ? null : { path: match[1]!, line: Number(match[2]) };
}

/** The view's section of that id. */
export function sectionOf(view: SourceView | null, id: string): ViewSection | null {
  return view?.sections.find((s) => s.id === id) ?? null;
}

function rowsOf(s: ViewSection | null): ViewRow[] {
  return (s?.blocks ?? []).flatMap((block) => (block.type === "entities" ? block.rows : []));
}

/**
 * The commit the run built (#1004): its declared revision, short as the Runs
 * table writes it, and the mirrored commit it opens, `null` where the mirror
 * holds none. `null` for a run with no revision.
 */
export function builtCommit(run: RunRow): { sha: string; entityId: string | null } | null {
  return run.revision === null ? null : { sha: shortSha(run.revision), entityId: run.commit_id };
}

/**
 * The mirrored repository a commit sits in (#1154), among the pipeline's own:
 * a commit's entity id is its repository's id, then `@` and the oid
 * (`gitea:owner/repo@<oid>`), so the repository is the one of `repositories`
 * whose entity id is that prefix. `null` for a commit whose id names none of
 * them, which the caller answers with its fallback.
 */
export function repositoryOfCommit(
  commitId: string | null,
  repositories: readonly { entity_id: string | null }[] | undefined,
): string | null {
  if (commitId === null) return null;
  const at = commitId.lastIndexOf("@");
  if (at <= 0) return null;
  const prefix = commitId.slice(0, at);
  return repositories?.find((repo) => repo.entity_id === prefix)?.entity_id ?? null;
}

/** The pipeline a run belongs to: the one of `pipelines` a row of its overview carries. */
export function pipelineOfRun(view: SourceView | null, pipelines: readonly string[]): string | null {
  return rowsOf(sectionOf(view, "overview")).find((r) => r.entity_id !== null && pipelines.includes(r.entity_id))?.entity_id ?? null;
}

/** How many artifacts the Overview's panel lists. */
export const FIRST_ARTIFACTS = 4;

/** The Artifacts panel: the first four, and how many there are. */
export function artifactsOf(view: SourceView | null): { first: ViewRow[]; total: number } {
  const s = sectionOf(view, "artifacts");
  const rows = rowsOf(s);
  return { first: rows.slice(0, FIRST_ARTIFACTS), total: s?.count ?? rows.length };
}

/** The commits in the run (*Changes*). */
export function changesOf(view: SourceView | null): ViewRow[] {
  return rowsOf(sectionOf(view, "changes"));
}

/** What the run ran with, from its view's *Parameters*. */
export function runValues(view: SourceView | null): StartValue[] {
  return (sectionOf(view, "parameters")?.blocks ?? []).flatMap((block) =>
    block.type === "settings" ? block.values.map((v) => ({ name: v.name, value: v.value, secret: v.secret })) : [],
  );
}

/** One row of the Parameters tab: the value in this run and the pipeline's default, `null` for none. */
export interface ParameterRow {
  name: string;
  value: string;
  secret: boolean;
  default: string | null;
}

/** The Parameters tab: each value the run ran with, beside the pipeline's default of that name. */
export function parameterRows(view: SourceView | null, settings: PipelineSettings | null): ParameterRow[] {
  const defaults = new Map((settings?.parameters ?? []).map((p) => [p.name, p.secret ? "" : p.value]));
  return runValues(view).map((v) => ({ name: v.name, value: v.value, secret: v.secret ?? false, default: defaults.get(v.name) ?? null }));
}

/**
 * *Re-run* (R-9.8, same settings): the run's branch, and every value it ran
 * with that is not the pipeline's default, a parameter the pipeline does not
 * declare included. A hidden value cannot be sent again, so its default runs.
 */
export function rerunRequest(
  pipelineId: string,
  run: RunRow,
  settings: PipelineSettings | null,
  view: SourceView | null,
): RunRequest {
  const fields = fieldsOf(settings);
  const ran = runValues(view);
  const declared = new Set(fields.map((f) => f.name));
  const extra = ran.filter((v) => !v.secret && !declared.has(v.name)).map((v) => ({ name: v.name, value: v.value }));
  const parameters = overridesOf(fields, valuesFrom(fields, ran), extra);
  const branch = run.branch === null ? "" : branchName(run.branch);
  return {
    payload: runPayload(pipelineId, branch, defaultBranch(settings), parameters, { clean_checkout: false, first_in_queue: false }),
    summary: `Re-run of #${run.number ?? ""}, same settings`,
    preset: null,
  };
}
