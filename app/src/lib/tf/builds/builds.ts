/**
 * The Builds list's rules (#848, R-9.1, R-9.4a): pure functions over the
 * rows `pipelines` answers, so every one is a test on plain data.
 *
 * Nothing here knows a source. A run is *failed* by its declared tone, *in
 * flight* by its declared instants (started and not finished, or neither),
 * and a pipeline is named by its declared name (ADR-0022).
 */
import type { JobPipeline, PipelineReason, PipelineRow, RunRow } from "../../ipc/entity";
import { clockLabel } from "../../time/format";
import { matchesAll } from "../filter/filter";
import { originLabel } from "../job/job";

/** How many runs the strip under a pipeline's name shows (R-9.4a). */
export const STRIP = 12;

/** The list's two chips, in prototype-6's words (R-9.1). */
export const CHIPS = ["Failing", "Running"] as const;
export type Chip = (typeof CHIPS)[number];

/** Whether the pipeline's latest run failed: its name is drawn in err tone. */
export function isFailing(pipeline: PipelineRow): boolean {
  return pipeline.recent[0]?.state?.tone === "err";
}

/** Whether one of the pipeline's runs started and has not finished. */
export function isRunning(pipeline: PipelineRow): boolean {
  return pipeline.recent.some((run) => run.started !== null && run.finished === null);
}

/**
 * A run's block in the strip: finished, its declared tone; in flight,
 * running once it started and queued before.
 */
export function runTone(run: RunRow): "ok" | "err" | "warn" | "muted" | "run" | "q" {
  if (run.finished === null && run.started !== null) return "run";
  if (run.finished === null && run.started === null) return "q";
  const tone = run.state?.tone;
  return tone === "ok" || tone === "err" || tone === "warn" ? tone : "muted";
}

/** The last {@link STRIP} runs, oldest first, so the newest is on the right. */
export function stripOf(pipeline: PipelineRow): RunRow[] {
  return pipeline.recent.slice(0, STRIP).reverse();
}

/**
 * When the latest run started, or was last touched if it has not started,
 * as prototype-6's row writes it: *11:45* today, *yesterday 16:20*, else the
 * date. Empty for a pipeline nothing ran.
 */
export function latestTime(pipeline: PipelineRow, now: Date = new Date()): string {
  const latest = pipeline.recent[0];
  const at = latest?.started ?? latest?.updated_at ?? null;
  return at === null ? "" : shortTime(at, now);
}

/** An instant as a row writes it: *11:45* today, *yesterday 16:20*, else the date. */
export function shortTime(iso: string, now: Date = new Date()): string {
  return clockLabel(iso, now, true);
}

/** A run's address: its pipeline's entity id, then its own (#853 draws it). */
export function runHash(pipelineId: string, runId: string): string {
  return `#/build/${encodeURIComponent(pipelineId)}/${encodeURIComponent(runId)}`;
}

/** The pipelines the filter's words and the chips keep, in the list's order. */
export function keptPipelines(list: readonly PipelineRow[], text: string, chips: readonly string[]): PipelineRow[] {
  return list.filter(
    (pipeline) =>
      matchesAll(`${pipeline.name} ${groupsOf(pipeline).join(" ")}`, text) &&
      (!chips.includes("Failing") || isFailing(pipeline)) &&
      (!chips.includes("Running") || isRunning(pipeline)),
  );
}

/** A pipeline's address: the entity id, as every full-view address carries one. */
export function pipelineHash(entityId: string): string {
  return `#/builds/${encodeURIComponent(entityId)}`;
}

/**
 * The pipeline an address selects. A bare `#/builds` selects the first
 * failing one, else the first, as prototype-6 opens on a failure; with the
 * job's pipelines given (*This job*, #850), the job's first failing one, else
 * its first. An id nothing carries selects none.
 */
export function pipelineOf(
  list: readonly PipelineRow[],
  rest: readonly string[],
  onJob: readonly PipelineRow[] = [],
): PipelineRow | null {
  const id = rest[0];
  if (id !== undefined && id !== "") return list.find((pipeline) => pipeline.entity_id === id) ?? null;
  // With *This job* on, the job's own failure first, else its first (#850).
  if (onJob.length > 0) return onJob.find(isFailing) ?? onJob[0] ?? null;
  return list.find(isFailing) ?? list[0] ?? null;
}

// ---------------------------------------------------------------------------
// The project tree and pins (#849, R-9.3) and *This job* (#850, R-9.1, R-9.2)
// ---------------------------------------------------------------------------

/**
 * The groups a pipeline sits in, outermost first: its declared chain, else
 * its one group, else none (it is drawn at the top of its source's tree).
 */
export function groupsOf(pipeline: Pick<PipelineRow, "path" | "group">): string[] {
  if (pipeline.path.length > 0) return pipeline.path;
  return pipeline.group !== null ? [pipeline.group] : [];
}

/** The pipeline list's scope (#850): the job's pipelines, or every one. */
export type Scope = "job" | "all";

/** One fold of the tree: a group, its own pipelines and the groups inside it. */
export interface Fold {
  /** What the summary says: one group, or a chain of single children joined by ` › `. */
  name: string;
  /** Unique in the tree, for remembering whether it is open. */
  key: string;
  /** Its own pipelines, in the list's order, drawn before its subgroups. */
  pipelines: PipelineRow[];
  /** Its subgroups by name. */
  folds: Fold[];
  /** Every pipeline beneath it, at any depth. */
  count: number;
  /** How many of those are failing. */
  failing: number;
}

/** One source's tree: the pipelines at its top and its top folds. */
export interface SourceTree {
  source: string;
  sourceId: string;
  pipelines: PipelineRow[];
  folds: Fold[];
}

/** A tree node while {@link treeOf} is still filling it. */
interface Draft {
  name: string;
  pipelines: PipelineRow[];
  kids: Map<string, Draft>;
}

/** Groups by name, ignoring case. */
function byName(a: { name: string }, b: { name: string }): number {
  return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
}

function fold(node: Draft, parentKey: string): Fold {
  // A group holding nothing but one subgroup is drawn as one row with it
  // ("Payments › Payouts"), as deep as the chain of single children goes.
  let name = node.name;
  let at = node;
  while (at.pipelines.length === 0 && at.kids.size === 1) {
    at = [...at.kids.values()][0]!;
    name += ` › ${at.name}`;
  }
  const key = `${parentKey}/${name}`;
  const folds = [...at.kids.values()]
    .sort(byName)
    .map((kid) => fold(kid, key));
  return {
    name,
    key,
    pipelines: at.pipelines,
    folds,
    count: at.pipelines.length + folds.reduce((sum, f) => sum + f.count, 0),
    failing: at.pipelines.filter(isFailing).length + folds.reduce((sum, f) => sum + f.failing, 0),
  };
}

/**
 * The pipelines as their sources' project trees, as deep as each goes (R-9.3):
 * one tree per source in the list's order, a pipeline under its
 * {@link groupsOf}, folds by name, and in each fold its own pipelines before
 * its subgroups.
 */
export function treeOf(list: readonly PipelineRow[]): SourceTree[] {
  const roots = new Map<string, { name: string; root: Draft }>();
  for (const pipeline of list) {
    let tree = roots.get(pipeline.source_id);
    if (tree === undefined) {
      tree = { name: pipeline.source_name, root: { name: "", pipelines: [], kids: new Map() } };
      roots.set(pipeline.source_id, tree);
    }
    let at = tree.root;
    for (const group of groupsOf(pipeline)) {
      let kid = at.kids.get(group);
      if (kid === undefined) {
        kid = { name: group, pipelines: [], kids: new Map() };
        at.kids.set(group, kid);
      }
      at = kid;
    }
    at.pipelines.push(pipeline);
  }
  return [...roots.entries()].map(([sourceId, { name, root }]) => ({
    source: name,
    sourceId,
    pipelines: root.pipelines,
    folds: [...root.kids.values()]
      .sort(byName)
      .map((kid) => fold(kid, sourceId)),
  }));
}

/** Whether `id` is one of the fold's pipelines, at any depth. */
export function foldHolds(f: Fold, id: string | null): boolean {
  if (id === null) return false;
  return f.pipelines.some((p) => p.entity_id === id) || f.folds.some((kid) => foldHolds(kid, id));
}

/**
 * Whether a fold is open (R-9.3): as the reader last left it this session;
 * else open while something narrows the list (words, a chip, *This job*), or
 * when it holds a failure or the selected pipeline; else closed.
 */
export function foldOpen(remembered: boolean | undefined, f: Fold, selected: string | null, narrowed: boolean): boolean {
  if (remembered !== undefined) return remembered;
  return narrowed || f.failing > 0 || foldHolds(f, selected);
}

/**
 * One row of the pipeline list as the windowed list draws it (#2211): a
 * heading, a pipeline at its depth, or a fold's summary. `key` is unique: a
 * pinned pipeline is also in its fold.
 */
export type ListRow =
  | { kind: "head"; key: string; text: string }
  | { kind: "pipeline"; key: string; pipeline: PipelineRow; depth: number }
  | { kind: "fold"; key: string; fold: Fold; depth: number; open: boolean };

/**
 * The list flattened into the rows drawn, in order: *Pinned* and its
 * pipelines, then each tree under its heading (`heading` names it, or `null`
 * for none), its own pipelines before its folds, and under each fold's
 * summary its rows only while `open` says so.
 */
export function listRows(
  pinned: readonly PipelineRow[],
  trees: readonly SourceTree[],
  heading: (tree: SourceTree, at: number) => string | null,
  open: (f: Fold) => boolean,
): ListRow[] {
  const rows: ListRow[] = [];
  if (pinned.length > 0) {
    rows.push({ kind: "head", key: "head:pinned", text: "Pinned" });
    for (const pipeline of pinned) rows.push({ kind: "pipeline", key: `pin:${pipeline.entity_id}`, pipeline, depth: 0 });
  }
  const walk = (pipelines: readonly PipelineRow[], folds: readonly Fold[], depth: number) => {
    for (const pipeline of pipelines) rows.push({ kind: "pipeline", key: `tree:${pipeline.entity_id}`, pipeline, depth });
    for (const f of folds) {
      const isOpen = open(f);
      rows.push({ kind: "fold", key: `fold:${f.key}`, fold: f, depth, open: isOpen });
      if (isOpen) walk(f.pipelines, f.folds, depth + 1);
    }
  };
  trees.forEach((tree, at) => {
    const text = heading(tree, at);
    if (text !== null) rows.push({ kind: "head", key: `head:${tree.sourceId}:${text}`, text });
    walk(tree.pipelines, tree.folds, 0);
  });
  return rows;
}

/** The pinned pipelines among `list`, in its order. */
export function pinnedOf(list: readonly PipelineRow[]): PipelineRow[] {
  return list.filter((pipeline) => pipeline.pinned);
}

/**
 * What a reason says beside a pipeline on a job, in prototype-6's words:
 * *your branch*, *PR check*, *Parent PAY-230*, a link set's name, else
 * *linked to PAY-231*.
 */
export function reasonLabel(reason: PipelineReason, job: string): string {
  switch (reason.kind) {
    case "branch":
      return "your branch";
    case "pr_check":
      return "PR check";
    case "linked":
      // The parent's and a link set's labels are the job sections' (#866).
      return originLabel(reason) ?? `linked to ${job}`;
  }
}

/** A pipeline's reasons as one line, each label once, in the read's order. */
export function reasonsLine(reasons: readonly PipelineReason[], job: string): string {
  return [...new Set(reasons.map((reason) => reasonLabel(reason, job)))].join(", ");
}

/** The job's reasons by pipeline id. */
export function reasonsById(rows: readonly JobPipeline[]): Map<string, PipelineReason[]> {
  return new Map(rows.map((row) => [row.pipeline_id, row.reasons]));
}
