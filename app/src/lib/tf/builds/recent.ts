/**
 * Builds' *Recent* (2026-11-25, `docs/decisions/2026-11-25-recent-runs.md`). Björn: "I want a view in builds that puts
 * the most recent builds at the top, hiding all the folders". The newest runs across every pipeline, newest first by
 * when each started or, not started, was queued ({@link runMade}), with no folds: each row names its pipeline and the
 * folders it sits in as quiet text. The rules are pure functions here, so each is a test on plain data.
 */
import type { RecentFilter, RecentPipeline, RecentRun, RunRow } from "../../ipc/entity";
import { joined, type Joined } from "../filter/filter";
import { groupsOf, runMade } from "./builds";
import { RESULTS, runMarks, runText, type RunMarks } from "./pipeline";

/** The word `#/builds/<word>` takes for *Recent*: no pipeline's entity id is one word with no `:`. */
export const RECENT = "recent";

/** *Recent*'s address. */
export const RECENT_HASH = `#/builds/${RECENT}`;

/** How many runs a read of *Recent* asks for: the first page, and each older page. */
export const RECENT_PAGE = 200;

/** The most runs *Recent* holds: past them, each pipeline's own page reads older history from its source. */
export const RECENT_HOLD = 2_000;

/** Whether an address under `#/builds` is *Recent*. */
export function isRecent(rest: readonly string[]): boolean {
  return rest[0] === RECENT && rest.length === 1;
}

/**
 * Two runs in *Recent*'s order: newer {@link runMade} first, then the greater id, as the backend's `run_made_idx`
 * orders them, so a page read again lays its runs where the last one did.
 */
export function recentOrder(a: RunRow, b: RunRow): number {
  const made = runMade(b) - runMade(a);
  if (made !== 0 && !Number.isNaN(made)) return made > 0 ? 1 : -1;
  return a.entity_id === b.entity_id ? 0 : a.entity_id < b.entity_id ? 1 : -1;
}

/**
 * What *Recent* holds after its head was read again: the head's runs, then those it held older than the head's last,
 * as they were. A run the head left out within its span left the list (deleted, or no longer kept by the filter). A
 * head that is the whole answer (`whole`) holds alone. Never more than {@link RECENT_HOLD}.
 */
export function withHead(held: readonly RecentRun[], head: readonly RecentRun[], whole: boolean): RecentRun[] {
  if (whole || head.length === 0) return head.slice(0, RECENT_HOLD);
  const last = head.at(-1)!.run;
  const fresh = new Set(head.map((one) => one.run.entity_id));
  const older = held.filter((one) => !fresh.has(one.run.entity_id) && recentOrder(one.run, last) > 0);
  return [...head, ...older].slice(0, RECENT_HOLD);
}

/** What *Recent* holds after an older page arrived: the page after what it holds, a run it holds already left out. */
export function withOlder(held: readonly RecentRun[], older: readonly RecentRun[]): RecentRun[] {
  const ids = new Set(held.map((one) => one.run.entity_id));
  return [...held, ...older.filter((one) => !ids.has(one.run.entity_id))].slice(0, RECENT_HOLD);
}

/** One run laid in where it is held, the rest as they were: a run in flight read again at its source. */
export function withRun(held: readonly RecentRun[], run: RunRow): RecentRun[] {
  return held.map((one) => (one.run.entity_id === run.entity_id ? { ...one, run } : one));
}

/** Where *Recent*'s filter stands, as its bar holds it. */
export interface RecentChoice {
  /** `job` keeps the runs on the job's branches, `mine` the runs the reader started; anything else keeps all. */
  scope: string | null;
  results: readonly string[];
  /** One source's id, or `null` for every CI source. */
  source: string | null;
}

/**
 * What the backend is asked (Fable's ruling 5): the scope, the results and the source narrow the read itself, so
 * *Failed* answers the newest failures wherever they lie. *This job* with its branches not read yet asks for none.
 */
export function recentFilterOf(choice: RecentChoice, jobBranches: readonly string[] | null): RecentFilter {
  return {
    sources: choice.source === null ? [] : [choice.source],
    results: choice.results.filter((result) => (RESULTS as readonly string[]).includes(result)),
    mine: choice.scope === "mine",
    branches: choice.scope === "job" ? [...(jobBranches ?? [])] : null,
  };
}

/**
 * The quiet text after a pipeline's name: the folders it sits in, outermost first, and its source's name before them
 * where two or more CI sources list runs, so two servers' *Payout › Build* read apart.
 */
export function placeWords(pipeline: Pick<RecentPipeline, "path" | "group" | "source_name"> | undefined, manySources: boolean): string {
  if (pipeline === undefined) return "";
  return [...(manySources ? [pipeline.source_name] : []), ...groupsOf(pipeline)].join(" › ");
}

/**
 * What the typed words rank a run by (#2535): its own words as a pipeline's Runs filter reads them ({@link runText}),
 * then its pipeline's name and place as the row draws them, and the source's name where the place does not hold it.
 */
export function recentText(run: RecentRun, pipelines: ReadonlyMap<string, RecentPipeline>, manySources: boolean): string {
  return recentFields(run, pipelines, manySources).text;
}

function recentFields({ run, pipeline_id }: RecentRun, pipelines: ReadonlyMap<string, RecentPipeline>, manySources: boolean): Joined {
  const pipeline = pipelines.get(pipeline_id);
  const fields = [runText(run), pipeline?.name ?? "", placeWords(pipeline, manySources)];
  if (pipeline !== undefined && !manySources) fields.push(pipeline.source_name);
  return joined(fields);
}

/** A run's matched letters over {@link recentText}, cut onto the row's cells, its pipeline's name and place too. */
export function recentMarks(
  { run, pipeline_id }: RecentRun,
  pipelines: ReadonlyMap<string, RecentPipeline>,
  manySources: boolean,
  pairs: readonly number[],
): RunMarks & { name: readonly number[]; place: readonly number[] } {
  const fields = recentFields({ run, pipeline_id }, pipelines, manySources);
  return { ...runMarks(run, pairs), name: fields.cut(pairs, 1), place: fields.cut(pairs, 2) };
}

/** The pipelines of a page by id, laid over those held: a page names only the pipelines of its own runs. */
export function withPipelines(held: ReadonlyMap<string, RecentPipeline>, page: readonly RecentPipeline[]): Map<string, RecentPipeline> {
  const all = new Map(held);
  for (const pipeline of page) all.set(pipeline.entity_id, pipeline);
  return all;
}
