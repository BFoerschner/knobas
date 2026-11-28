/**
 * Builds' *Recent* (2026-11-25, `docs/decisions/2026-11-25-recent-runs.md`). Björn: "I want a view in builds that puts
 * the most recent builds at the top, hiding all the folders". The newest runs across every pipeline, newest first by
 * when each started or, not started, was queued ({@link runMade}), with no folds: each row names its pipeline and the
 * folders it sits in as quiet text. The rules are pure functions here, so each is a test on plain data.
 */
import type { PipelineRow, RecentFilter, RecentPipeline, RecentRun, RunRow } from "../../ipc/entity";
import { joined, type Joined } from "../filter/filter";
import { parseTfHash } from "../route";
import { REPO_FOLDER, groupsOf, runMade } from "./builds";
import { RESULTS, runMarks, runText, type RunMarks } from "./pipeline";

/** The word `#/builds/<word>` takes for *Recent*: no pipeline's entity id is one word with no `:`. */
export const RECENT = "recent";

/** *Recent*'s address. */
export const RECENT_HASH = `#/builds/${RECENT}`;

/** How many runs a read of *Recent* asks for: the head read again, and each older page. */
export const RECENT_PAGE = 200;

/**
 * How many runs the first read asks for: the rows that fill the window at the tallest screen the open budget draws
 * (about 50 at 1440 px), drawn before the rest of {@link RECENT_PAGE} is read behind them (#2569,
 * `docs/decisions/2026-11-27-builds-first-open.md`).
 */
export const RECENT_FIRST = 60;

/** The most runs *Recent* holds: past them, each pipeline's own page reads older history from its source. */
export const RECENT_HOLD = 2_000;

/** Whether an address under `#/builds` is *Recent*. */
export function isRecent(rest: readonly string[]): boolean {
  return rest[0] === RECENT && rest.length === 1;
}

/**
 * The word `#/builds/<word>/…` takes for a folder (#2570): as {@link RECENT}, no pipeline's entity id is one word with no
 * `:`. What follows is the folder's place, one segment a level: its source's id and its groups (*By project*), or
 * {@link REPO_FOLDER} and the repository's entity id (*By repository*; none for *No repository*).
 */
export const FOLDER = "folder";

export { REPO_FOLDER };

/**
 * Whose runs the one table lists (#2570, the mockup's rule 1): every pipeline's (*All pipelines*, `#/builds/recent`),
 * or a folder's. A pipeline keeps its own page.
 */
export type RunScope = { kind: "all" } | { kind: "folder"; path: readonly string[] };

/** The run scope an address under `#/builds` names, or `null` for a pipeline's or none. */
export function runScopeOf(rest: readonly string[]): RunScope | null {
  if (isRecent(rest)) return { kind: "all" };
  if (rest[0] === FOLDER && rest.length > 1) return { kind: "folder", path: rest.slice(1) };
  return null;
}

/** Whether an address is a run scope's: *All pipelines* or a folder, not a pipeline, a run or a bare `#/builds`. */
export function isRunScopeHash(hash: string): boolean {
  const route = parseTfHash(hash);
  return route.view === "full" && route.head === "builds" && runScopeOf(route.rest) !== null;
}

/** A run scope's address, each segment of a folder's place encoded. */
export function runScopeHash(scope: RunScope): string {
  return scope.kind === "all" ? RECENT_HASH : `#/builds/${FOLDER}/${scope.path.map(encodeURIComponent).join("/")}`;
}

/**
 * The pipelines a folder lists, by id, in `list`'s order: by project, those whose source is the place's first segment
 * and whose groups start with the rest; by repository, those whose runs built that repository, or built none mirrored.
 * Read against every pipeline: the rail's tools narrow the rail, never the table (the mockup's rule 2).
 */
export function folderPipelines<P extends Pick<PipelineRow, "entity_id" | "source_id" | "path" | "group">>(
  list: readonly P[],
  path: readonly string[],
  reposOf: (pipeline: P) => readonly { id: string }[],
): string[] {
  const [head, ...groups] = path;
  const keeps =
    head === REPO_FOLDER
      ? (p: P) => (groups[0] === undefined ? reposOf(p).length === 0 : reposOf(p).some((repo) => repo.id === groups[0]))
      : (p: P) => p.source_id === head && groups.every((group, at) => groupsOf(p)[at] === group);
  return list.filter(keeps).map((p) => p.entity_id);
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
 * What the backend is asked (Fable's ruling 5): the scope, the results, the source and a folder's pipelines narrow the read itself, so
 * *Failed* answers the newest failures wherever they lie. *This job* with its branches not read yet asks for none.
 */
export function recentFilterOf(choice: RecentChoice, jobBranches: readonly string[] | null, pipelines: readonly string[] = []): RecentFilter {
  return {
    sources: choice.source === null ? [] : [choice.source],
    results: choice.results.filter((result) => (RESULTS as readonly string[]).includes(result)),
    mine: choice.scope === "mine",
    branches: choice.scope === "job" ? [...(jobBranches ?? [])] : null,
    // A folder's scope (#2570), sorted so the same folder asks the same read however its pipelines were listed.
    pipelines: [...pipelines].sort(),
  };
}

/**
 * The folders the run row says before a pipeline's name: those it sits in, outermost first, and its source's name before
 * them where two or more CI sources list runs, so two servers' *Payout › Build* read apart. At a folder by project
 * (`folder`, its place as {@link RunScope} has it) they are said from that folder down, and the source not at all: every
 * row shares both (#2572). A folder by repository is none of a pipeline's folders, so they are said whole there.
 */
export function placeWords(
  pipeline: Pick<RecentPipeline, "path" | "group" | "source_name"> | undefined,
  manySources: boolean,
  folder: readonly string[] | null = null,
): string {
  if (pipeline === undefined) return "";
  if (folder !== null && folder[0] !== REPO_FOLDER) return groupsOf(pipeline).slice(folder.length - 1).join(" › ");
  return [...(manySources ? [pipeline.source_name] : []), ...groupsOf(pipeline)].join(" › ");
}

/**
 * What the typed words rank a run by (#2535): its own words as a pipeline's Runs filter reads them ({@link runText}),
 * then its pipeline's name and place as the row draws them, and the source's name where the place does not hold it.
 */
export function recentText(
  run: RecentRun,
  pipelines: ReadonlyMap<string, RecentPipeline>,
  manySources: boolean,
  folder: readonly string[] | null = null,
): string {
  return recentFields(run, pipelines, manySources, folder).text;
}

function recentFields(
  { run, pipeline_id }: RecentRun,
  pipelines: ReadonlyMap<string, RecentPipeline>,
  manySources: boolean,
  folder: readonly string[] | null,
): Joined {
  const pipeline = pipelines.get(pipeline_id);
  const fields = [runText(run), pipeline?.name ?? "", placeWords(pipeline, manySources, folder)];
  if (pipeline !== undefined && !manySources) fields.push(pipeline.source_name);
  return joined(fields);
}

/** A run's matched letters over {@link recentText}, cut onto the row's cells, its pipeline's name and place too. */
export function recentMarks(
  { run, pipeline_id }: RecentRun,
  pipelines: ReadonlyMap<string, RecentPipeline>,
  manySources: boolean,
  pairs: readonly number[],
  folder: readonly string[] | null = null,
): RunMarks & { name: readonly number[]; place: readonly number[] } {
  const fields = recentFields({ run, pipeline_id }, pipelines, manySources, folder);
  return { ...runMarks(run, pairs), name: fields.cut(pairs, 1), place: fields.cut(pairs, 2) };
}

/**
 * The pipelines of a page by id, laid over those held: a page names only the pipelines of its own runs. A page that
 * names none new or changed keeps `held` itself, so the rows already drawn are not drawn again: the rest of Recent's
 * first page, laid behind its first rows, named the same pipelines and re-derived every drawn row (#2569b).
 */
export function withPipelines(held: ReadonlyMap<string, RecentPipeline>, page: readonly RecentPipeline[]): ReadonlyMap<string, RecentPipeline> {
  if (page.every((pipeline) => samePipeline(held.get(pipeline.entity_id), pipeline))) return held;
  const all = new Map(held);
  for (const pipeline of page) all.set(pipeline.entity_id, pipeline);
  return all;
}

/**
 * A pipeline's name as the run row cuts it (#2572, the mockup's *The run row*): what leads it and its last word. Only
 * the lead shrinks, so `CustomsGateway_Build` and `CustomsGateway_Contract` never read as the same `CustomsGatew…`.
 * The last word follows the last `_`, space, `-` or `.`, which stays with the lead. A name with none splits where its
 * last capital follows a small letter (`CustomsGatewayBuild`), and one with neither is its last word whole: an end cut.
 */
export function nameParts(name: string): [lead: string, last: string] {
  const split = /^(.*[^_\s.-].*[_\s.-])([^_\s.-]+)$/.exec(name) ?? /^(.*[a-z])([A-Z][^A-Z]*)$/.exec(name);
  return split === null ? ["", name] : [split[1]!, split[2]!];
}

/**
 * A run row's pipeline cell (#2572): its name, the folders said before it ({@link placeWords}), the address of the
 * pipeline's own folder for those words to link to (`null` when none are said), and the whole path as the cell's title.
 */
export function rowPipeline(
  pipeline: Pick<RecentPipeline, "source_id" | "source_name" | "name" | "path" | "group"> | undefined,
  manySources: boolean,
  folder: readonly string[] | null,
): { name: string; place: string; folder: string | null; whole: string } {
  if (pipeline === undefined) return { name: "", place: "", folder: null, whole: "" };
  const place = placeWords(pipeline, manySources, folder);
  const whole = [placeWords(pipeline, manySources), pipeline.name].filter((part) => part !== "").join(" › ");
  const link = place === "" ? null : runScopeHash({ kind: "folder", path: [pipeline.source_id, ...groupsOf(pipeline)] });
  return { name: pipeline.name, place, folder: link, whole };
}

/**
 * How wide the run table's folder column stands (#2572): `long` at *All pipelines* and at a folder by repository, where
 * the folders are said whole; `short` at a folder by project with folders below it; `none` at one without, where the
 * names take the whole cell. Read from the folder's pipelines, so it is known before a run is.
 */
export function folderColumn(pipelines: readonly Pick<PipelineRow, "path" | "group">[], folder: readonly string[] | null): "long" | "short" | "none" {
  if (folder === null || folder[0] === REPO_FOLDER) return "long";
  return pipelines.some((p) => groupsOf(p).length > folder.length - 1) ? "short" : "none";
}

/** Whether `held` is `page`'s pipeline as it is: every field the same. */
function samePipeline(held: RecentPipeline | undefined, page: RecentPipeline): boolean {
  return held !== undefined && (held === page || JSON.stringify(held) === JSON.stringify(page));
}
