/**
 * The pipeline page's rules (#851, R-9.4 to R-9.6): pure functions over the
 * runs `pipeline_runs` and `older_pipeline_runs` answer and the settings
 * `pipeline_settings` reads, so every one is a test on plain data.
 *
 * Nothing here knows a source: a run passed or failed by its declared tone,
 * is in flight by its declared instants ({@link runTone}), and a pipeline's
 * word is the label its source's descriptor gives the kind it declares the
 * pipeline frame on (ADR-0022).
 */
import type { PipelineRow, PipelineSettings, PipelineTrigger, RunRow } from "../../ipc/entity";
import type { QueuedWrite, SourceDescriptor, SourceSummary, SourceSyncStatus, WriteOpPayload } from "../../ipc/sources";
import { ageLabel, secondsLabel } from "../../time/format";
import { signInFails } from "../../shell/health.svelte";
import type { WriteSource } from "../../shell/queued-toast";
import { matchesAll } from "../filter/filter";
import type { WriteFollower } from "../../shell/follow-write";
import { markQueued, type Mark } from "../queued.svelte";
import { reenterWords, secretNoun } from "../settings/settings";
import { pipelineHash, runTone, shortTime } from "./builds";

/**
 * A run started from this page, drawn *queued* until a read holds it
 * (R-9.8): its row's id is the write's, under a prefix no entity id has.
 */
export function startedHere(writeId: number): string {
  return `queued:${writeId}`;
}

/** Whether a row is a run started from this page and not yet read back. */
export function isStartedHere(run: RunRow): boolean {
  return run.entity_id.startsWith("queued:");
}

/**
 * The kind of mark a queued run is on its pipeline (#1449, `queued.svelte.ts`):
 * its text the branch it was queued on (`""` for the default), its base the
 * newest run the page held as it was queued.
 */
export const QUEUED_RUN = "run";

/** The op every run here is queued as (`RunBuild`), as `refused_writes` asks about it (#2040). */
export const RUN_OP = "run_build";

/**
 * The fields of a run that can need more than the run (#2055), by their
 * payload names: its parameter overrides and its place at the queue's top.
 * A TeamCity user may run builds and send neither.
 */
export const RUN_OVERRIDES = "parameters";
export const RUN_FIRST_IN_QUEUE = "first_in_queue";

/** What the Run… dialog asks `refused_writes` about the run's fields on `entity` (#2055). */
export function runFieldAsks(entity: string): { entity: string; op: string; field: string }[] {
  return [RUN_OVERRIDES, RUN_FIRST_IN_QUEUE].map((field) => ({ entity, op: RUN_OP, field }));
}

/** The branch a run write names: `null` for the default, as the Run… dialog sends it (#852). */
export function runBranch(payload: WriteOpPayload): string | null {
  return "RunBuild" in payload ? payload.RunBuild.branch : null;
}

/**
 * A run queued on `pipeline`, from its page or a run page (#1449): marked on
 * the pipeline until a read holds it ({@link runLanded}), followed until the
 * queue holds or refuses it. `held` is the runs the page holds, newest first;
 * before its read arrives, the pipeline's own recent runs stand in.
 */
export function markQueuedRun(
  pipeline: Pick<PipelineRow, "entity_id" | "name" | "recent">,
  write: Pick<QueuedWrite, "id">,
  branch: string | null,
  held: readonly RunRow[] | null,
  follower: WriteFollower,
): void {
  markQueued(pipeline.entity_id, QUEUED_RUN, branch ?? "", {
    write,
    base: (held ?? pipeline.recent)[0]?.entity_id ?? null,
    many: true,
    following: { name: `Run of ${pipeline.name}` },
    follower,
  });
}

/** The runs queued this session and not yet read back, as queued rows, newest first. */
export function queuedRunRows(marks: readonly Mark[], fallback: string | null): RunRow[] {
  return marks
    .map((mark, at): RunRow => ({
      entity_id: startedHere(mark.write ?? -1 - at),
      number: null,
      state: { text: "queued", tone: "info" },
      branch: mark.text !== "" ? mark.text : fallback,
      started: null,
      finished: null,
      duration_seconds: null,
      updated_at: null,
      web_url: null,
      // The reader queued it: *you*, as prototype-6's `startRun` draws it.
      trigger: "person",
      started_by: null,
      started_by_me: true,
      revision: null,
      commit_id: null,
      agent: null,
      current_step: null,
      tests: null,
    }))
    .reverse();
}

/**
 * Which queued runs a read holds (#1449), for `settleQueued`: a run newer
 * than the newest the page held as it was queued, on its branch (any, for
 * one queued on the default), that no change or schedule started. Each such
 * run holds one queued run, the oldest first, so two queued and one read
 * back leaves one queued row.
 */
export function runLanded(rows: readonly RunRow[]): (mark: Mark) => boolean {
  const taken = new Set<string>();
  return (mark) => {
    const at = mark.base === null ? -1 : rows.findIndex((row) => row.entity_id === mark.base);
    const newer = at < 0 ? rows : rows.slice(0, at);
    const held = [...newer].reverse().find(
      (row) =>
        !taken.has(row.entity_id) &&
        !isStartedHere(row) &&
        row.trigger !== "change" &&
        row.trigger !== "schedule" &&
        (mark.text === "" || row.branch === null || branchName(row.branch) === branchName(mark.text)),
    );
    if (held === undefined) return false;
    taken.add(held.entity_id);
    return true;
  };
}

/** The sync statuses with `row` in its source's place (#1450): what a `sync:state` event says replaces what was read. */
export function syncWith(rows: readonly SourceSyncStatus[], row: SourceSyncStatus): SourceSyncStatus[] {
  return rows.some((r) => r.source_id === row.source_id) ? rows.map((r) => (r.source_id === row.source_id ? row : r)) : [...rows, row];
}

/** How many runs the history chart draws (R-9.4). */
export const HISTORY = 30;

/** The chart's height in px, and the shortest bar it draws. */
export const CHART_HEIGHT = 56;
/** A bar's width in px, and the step from one bar to the next. */
export const BAR_WIDTH = 11;
export const BAR_STEP = BAR_WIDTH + 2;
const SHORTEST_BAR = 8;

/** How wide a row's duration bar is at the longest run, in px. */
export const DURBAR_WIDTH = 60;

/** The page's tabs, in prototype-6's order and words; `runs` is the default. */
export const TABS = [
  { id: "runs", name: "Runs" },
  { id: "params", name: "Parameters" },
  { id: "triggers", name: "Triggers" },
] as const;
export type Tab = (typeof TABS)[number]["id"];

/** The tab an address names: `#/builds/<pipeline>/<tab>`; anything else is Runs. */
export function tabOf(rest: readonly string[]): Tab {
  const asked = rest[1];
  return TABS.find((tab) => tab.id === asked)?.id ?? "runs";
}

/** A tab's address. Runs is the pipeline's own. */
export function tabHash(pipelineId: string, tab: Tab): string {
  const base = pipelineHash(pipelineId);
  return tab === "runs" ? base : `${base}/${tab}`;
}

/** Whether a run is over: it has finished. */
export function isFinished(run: RunRow): boolean {
  return run.finished !== null;
}

/** Whether a finished run passed: its declared tone is `ok`. */
export function passed(run: RunRow): boolean {
  return isFinished(run) && run.state?.tone === "ok";
}

/** The last {@link HISTORY} runs, oldest first, so the newest bar is on the right. */
export function historyOf(runs: readonly RunRow[]): RunRow[] {
  return runs.slice(0, HISTORY).reverse();
}

/** The longest duration among `runs`, at least one second. */
export function longest(runs: readonly RunRow[]): number {
  return Math.max(1, ...runs.map((run) => run.duration_seconds ?? 0));
}

/**
 * A bar's height in px: duration against the longest, never under
 * {@link SHORTEST_BAR}; a run with no duration yet (running, queued) is full
 * height, as prototype-6 draws one.
 */
export function barHeight(run: RunRow, max: number): number {
  if (run.duration_seconds === null) return CHART_HEIGHT;
  return Math.max(SHORTEST_BAR, Math.round((run.duration_seconds / max) * CHART_HEIGHT));
}

/** A row's duration bar in px, against the longest run. */
export function durbarWidth(seconds: number, max: number): number {
  return Math.max(1, Math.round((seconds / max) * DURBAR_WIDTH));
}

/** The facts beside the chart (R-9.4): *Passed*, *Usual duration*, *Last passing*. */
export interface HistoryFacts {
  /** How many of the finished runs passed, and of how many. */
  passed: number;
  finished: number;
  /** The median duration of the finished runs, or `null` with none. */
  usual: number | null;
  /** The newest run that passed, or `null`. */
  lastPassing: RunRow | null;
}

/** The facts over every run the page holds. */
export function historyFacts(runs: readonly RunRow[]): HistoryFacts {
  const done = runs.filter(isFinished);
  const durations = done
    .map((run) => run.duration_seconds)
    .filter((seconds): seconds is number => seconds !== null)
    .sort((a, b) => a - b);
  return {
    passed: done.filter(passed).length,
    finished: done.length,
    usual: durations.length > 0 ? durations[Math.floor(durations.length / 2)]! : null,
    lastPassing: runs.find(passed) ?? null,
  };
}

/** A run's result word, as the chart's tip and a bar's name say it. */
export function resultWord(run: RunRow): string {
  const tone = runTone(run);
  if (tone === "run") return "running";
  if (tone === "q") return "queued";
  return run.state?.text ?? "finished";
}

/** A bar's tip: *#1187 FAILURE, main · 4 m 12 s, today 10:05*. */
export function barTip(run: RunRow, now: Date = new Date()): string {
  const head = `${run.number !== null ? `#${run.number}` : "queued"} ${resultWord(run)}${run.branch ? `, ${run.branch}` : ""}`;
  const took = run.duration_seconds !== null ? secondsLabel(run.duration_seconds) : resultWord(run);
  const at = run.started ?? run.updated_at;
  return `${head} · ${took}${at ? `, ${shortTime(at, now)}` : ""}`;
}

/** The result chips of the Runs filter bar, in prototype-6's words. */
export const RESULTS = ["Failed", "Running", "Queued", "Passed"] as const;

/** Which result chip a run falls under, or `null` for one under none (a canceled run). */
export function resultOf(run: RunRow): (typeof RESULTS)[number] | null {
  const tone = runTone(run);
  if (tone === "run") return "Running";
  if (tone === "q") return "Queued";
  if (tone === "err") return "Failed";
  if (tone === "ok") return "Passed";
  return null;
}

/** How many characters of a sha a trigger names: prototype-6's `a41f2c`. */
const SHORT_SHA = 6;

/** A sha as a trigger and the run page's *Commit* write it: `a41f2c`. */
export function shortSha(sha: string): string {
  return sha.slice(0, SHORT_SHA);
}

/**
 * How a run was started, as the Runs table and the run page say it (#961):
 * *VCS change a41f2c* for a change, *Schedule*, the person's account, *you*
 * for the reader; *Chain*, *Chain, by jonas* for a step a chain queued
 * (#1146). A run whose trigger misses but names a person is that person; one
 * that names neither says nothing.
 */
export function triggerLabel(run: RunRow): string {
  if (run.trigger === "change") return run.revision !== null ? `VCS change ${shortSha(run.revision)}` : "VCS change";
  if (run.trigger === "schedule") return "Schedule";
  const who = run.started_by_me ? "you" : (run.started_by ?? "");
  if (run.trigger === "chain") return who === "" ? "Chain" : `Chain, by ${who}`;
  return who;
}

/** Where the Runs filter stands. */
export interface RunFilter {
  text: string;
  /**
   * `job` keeps the runs on the job's branches, `mine` the runs the reader
   * started; anything else keeps all.
   */
  scope: string | null;
  results: readonly string[];
  branch: string | null;
}

/**
 * The runs the filter keeps, in their order: the words over the run's
 * number, branch, commit and who or what started it, the scope, any of the
 * result chips, and the branch.
 */
export function keptRuns(runs: readonly RunRow[], filter: RunFilter, jobBranches: readonly string[]): RunRow[] {
  return runs.filter((run) => {
    const words = `#${run.number ?? ""} ${run.branch ?? ""} ${run.revision ?? ""} ${triggerLabel(run)} ${run.started_by ?? ""}`;
    if (!matchesAll(words, filter.text)) return false;
    if (filter.scope === "job" && !(run.branch !== null && jobBranches.includes(run.branch))) return false;
    if (filter.scope === "mine" && !run.started_by_me) return false;
    if (filter.results.length > 0) {
      const result = resultOf(run);
      if (result === null || !filter.results.includes(result)) return false;
    }
    return filter.branch === null || run.branch === filter.branch;
  });
}

/** Every branch the runs built, in the order they first appear. */
export function branchesOf(runs: readonly RunRow[]): string[] {
  return [...new Set(runs.map((run) => run.branch).filter((branch): branch is string => branch !== null))];
}

/** The local calendar day of an instant, `YYYY-MM-DD`. */
function dayOf(at: Date): string {
  return `${at.getFullYear()}-${String(at.getMonth() + 1).padStart(2, "0")}-${String(at.getDate()).padStart(2, "0")}`;
}

/** One day's runs, under its heading. */
export interface DayGroup {
  label: string;
  runs: RunRow[];
}

/**
 * The runs by the day they started (or were queued), in order: *Today*,
 * *Yesterday*, then the date. A run with no instant at all joins the group
 * before it.
 */
export function byDay(runs: readonly RunRow[], now: Date = new Date()): DayGroup[] {
  const today = dayOf(now);
  const yesterday = dayOf(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1));
  const groups: DayGroup[] = [];
  for (const run of runs) {
    const at = run.started ?? run.updated_at;
    const day = at === null ? (groups.at(-1)?.label ?? "Today") : dayOf(new Date(at));
    const label = day === today ? "Today" : day === yesterday ? "Yesterday" : day;
    const last = groups.at(-1);
    if (last && last.label === label) last.runs.push(run);
    else groups.push({ label, runs: [run] });
  }
  return groups;
}

/**
 * The runs the page holds after an older page arrives: the older ones after
 * what it holds, a run it holds already left out.
 */
export function appendOlder(held: readonly RunRow[], older: readonly RunRow[]): RunRow[] {
  const ids = new Set(held.map((run) => run.entity_id));
  return [...held, ...older.filter((run) => !ids.has(run.entity_id))];
}

/**
 * The source's word for a pipeline (ADR-0022): the label its adapter gives
 * the kind it declares the pipeline frame on, lower-cased as a common noun
 * (*build configuration*). `null` where the descriptor says nothing.
 */
export function pipelineWord(descriptors: readonly SourceDescriptor[], adapterKind: string | null): string | null {
  const descriptor = descriptors.find((d) => d.adapter_kind === adapterKind);
  const kind = (descriptor?.payload_paths ?? []).find((paths) => paths.frame === "pipeline")?.kind;
  const label = (descriptor?.entity_kinds ?? []).find((info) => info.id === kind)?.label;
  return label ? label.charAt(0).toLowerCase() + label.slice(1) : null;
}

/** A branch as a person names it: `refs/heads/main` is *main*. */
export function branchName(ref: string): string {
  return ref.replace(/^refs\/heads\//, "");
}

/** The default branch the settings name, from the first repository that names one. */
export function defaultBranch(settings: PipelineSettings | null): string | null {
  const ref = settings?.repositories.find((repo) => repo.default_branch)?.default_branch;
  return ref ? branchName(ref) : null;
}

/** What a trigger fires on, in one line: *branch filter +:\*, when daily at 02:00*. */
export function triggerDetail(trigger: PipelineTrigger): string {
  return trigger.facts.map((fact) => `${fact.label.toLowerCase()} ${fact.value}`).join(", ");
}

/** The header's trigger sentence: *Triggered by VCS and Schedule.*; empty with none switched on. */
export function triggersSentence(settings: PipelineSettings | null): string {
  const names = [...new Set((settings?.triggers ?? []).filter((t) => !t.disabled).map((t) => t.name))];
  if (names.length === 0) return "";
  const list = names.length === 1 ? names[0]! : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
  return `Triggered by ${list}.`;
}

/**
 * The job's branch to offer *Run on <branch>* for: the first of the job's
 * branches this pipeline has built. `null` when it has built none of them,
 * and the button runs the default branch.
 */
export function jobBranchFor(runs: readonly RunRow[], jobBranches: readonly string[]): string | null {
  const built = new Set(branchesOf(runs));
  return jobBranches.find((branch) => built.has(branch)) ?? null;
}

/** The warn line over a pipeline whose source's data is stale, with its fix where it has one. */
export interface StaleLine {
  text: string;
  /**
   * Where the fix is, for a credential the reader has to re-enter: the link's
   * words in the source's noun (#1405), *Re-enter token*, and the noun alone
   * for the run dialog's sentence.
   */
  fix: { href: string; words: string; secret: "password" | "token" } | null;
}

/**
 * The warn line (R-9.4): a source whose sign-in fails, or whose last sync
 * failed, has data that may be out of date, and a run started now waits in
 * the queue. Nothing for a source that synced. The source rows carry its
 * credential's state and what that credential is called.
 */
export function staleLine(
  pipeline: Pick<PipelineRow, "source_id" | "source_name">,
  sources: readonly Pick<SourceSummary, "id" | "health" | "auth_kind">[],
  sync: readonly SourceSyncStatus[],
  now: Date = new Date(),
): StaleLine | null {
  const source = sources.find((row) => row.id === pipeline.source_id);
  const state = source?.health;
  const run = sync.find((row) => row.source_id === pipeline.source_id);
  const name = pipeline.source_name;
  if (source && state && signInFails(state.state)) {
    const secret = secretNoun(source.auth_kind);
    const since = run?.last_outcome === "unauthorized" ? run.last_finished_at : state.checked_at;
    const why = state.state === "missing_secret" ? `no ${secret} stored` : "sign-in failing";
    const lead = since ? `${name} data is ${ageLabel(since, now)} old: ${why}.` : `${name} data is stale: ${why}.`;
    return {
      text: `${lead} Runs you start wait in the queue until it works again.`,
      fix: { href: `#/settings/source/${encodeURIComponent(pipeline.source_id)}`, words: reenterWords(source.auth_kind), secret },
    };
  }
  if (run && !run.running && (run.last_outcome === "unreachable" || run.last_outcome === "error")) {
    const why = run.last_outcome === "unreachable" ? `${name} cannot be reached` : "the last sync failed";
    // A failing sync retries on backoff, so its last finish is the last try,
    // not the last data: say when it was tried, never how old the data is.
    const tried = run.last_finished_at ? `, last tried ${ageLabel(run.last_finished_at, now)} ago` : "";
    return { text: `${name} data may be out of date: ${why}${tried}.`, fix: null };
  }
  return null;
}

/** A run's chip class: in flight is info, finished its declared tone. */
export function chipTone(run: RunRow): string {
  const tone = runTone(run);
  if (tone === "run" || tone === "q") return "tf-info";
  return tone === "ok" || tone === "err" || tone === "warn" ? `tf-${tone}` : "";
}

/**
 * What a run queued from a Builds page is, for the queued-write toast
 * (`toastQueued`): *run Payout_IntegrationTests. Re-run of #1187, same settings*,
 * as prototype-6's `startRun` words it.
 */
export function queuedRunWhat(pipeline: Pick<PipelineRow, "name">, summary: string): string {
  return `run ${pipeline.name}. ${summary}`;
}

/** The source a pipeline's writes go to, for the queued-write toast's sign-in sentence. */
export function pipelineSource(pipeline: Pick<PipelineRow, "source_id" | "source_name">): WriteSource {
  return { id: pipeline.source_id, display_name: pipeline.source_name };
}
