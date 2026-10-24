/**
 * The reads and the write the pipeline page makes (#851), injectable so a
 * test needs no bridge. Each is wrapped rather than referenced, so importing
 * this module calls nothing.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import {
  artifactPreview,
  downloadRunFile,
  entityView,
  getEntity,
  listEntities,
  listProjects,
  olderPipelineRuns,
  owedTickets,
  pipelineSettings,
  runArtifacts,
  runLog,
  runPresets,
  runTests,
  saveRunPreset,
  deleteRunPreset,
  submitWrite,
  testHistory,
} from "../../ipc/entity";
import type {
  ArtifactEntry,
  ArtifactPreview,
  DownloadTarget,
  LinkEntry,
  PipelineSettings,
  Project,
  RunLog,
  RunPreset,
  RunRow,
  RunTests,
  SourceView,
  TestOutcome,
} from "../../ipc/entity";
import {
  listAdapters,
  listSources,
  syncStatus,
  type QueuedWrite,
  type SourceDescriptor,
  type SourceSummary,
  type SourceSyncStatus,
  type WriteOpPayload,
} from "../../ipc/sources";
import { EVENTS } from "../../ipc";
import { writeFollower, type WriteFollower } from "../../shell/follow-write";
import { openExternal } from "../../shell/open-external";
import { isEntityId, resolveTicket } from "../job/job";
import { branchOptions } from "../repo/repo";
import type { StartValue } from "./run-dialog";

export interface PipelinePorts {
  /** Runs older than `before`, asked of the source (`older_pipeline_runs`). */
  older: (pipelineId: string, before: string) => Promise<RunRow[]>;
  /** The settings, read live (`pipeline_settings`). */
  settings: (pipelineId: string) => Promise<PipelineSettings | null>;
  /** The adapters' descriptors: the source's word for a pipeline. */
  adapters: () => Promise<SourceDescriptor[]>;
  /** The configured sources, with their credential health. */
  sources: () => Promise<SourceSummary[]>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  /** The names of the branches the job reaches. */
  jobBranches: (job: string) => Promise<string[]>;
  /** Queue a write; `linkTo` links what it makes to that entity once it lands (`submit_write`'s `link_to`, #984, #1451). */
  submitWrite: (payload: WriteOpPayload, linkTo?: string) => Promise<QueuedWrite>;
  openExternal: (url: string) => Promise<void>;
  /** The pipeline's Run… presets (`run_presets`, #852). */
  presets: (pipelineId: string) => Promise<RunPreset[]>;
  /** Save one, replacing the pipeline's preset of that name (`save_run_preset`). */
  savePreset: (preset: RunPreset) => Promise<RunPreset>;
  /** Remove one by name (`delete_run_preset`, #996). */
  deletePreset: (pipelineId: string, name: string) => Promise<void>;
  /** The branch names of a mirrored repository, the default first (its view's *Branches*). */
  repoBranches: (repoId: string) => Promise<string[]>;
  /** The parameters a run ran with (its view's *Parameters*), for *#N settings*. */
  runParameters: (runId: string) => Promise<StartValue[]>;
  /** The shell's events: `sync:state` says a source's sync finished, and the page reads again (#1450). */
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
  /** What follows a queued run until the queue holds or refuses it (#1449). */
  follower: () => WriteFollower;
}

/** How many branches the job's read takes: a job reaches a handful. */
const BRANCH_LIMIT = 200;

/**
 * The branches `job` reaches, by name: its ticket resolved as the job page
 * resolves it, then `list_entities` scoped by the job reach to the `branch`
 * kind. A branch row's title is its name. Nothing for a job no source holds.
 */
export async function readJobBranches(job: string): Promise<string[]> {
  const sources = isEntityId(job) ? [] : await listSources();
  const ticket = await resolveTicket(job, sources, getEntity);
  if (ticket === null) return [];
  const page = await listEntities(
    {
      sources: [],
      kinds: ["branch"],
      updated_within_days: null,
      order: "updated_desc",
      context: null,
      project: null,
      include_deleted: false,
      reach: ticket.row.entity_id,
    },
    BRANCH_LIMIT,
    0,
  );
  return page.rows.map((row) => row.title);
}

/** A repository's branches, from its source view's *Branches* section (#838's read). */
export async function readRepoBranches(repoId: string): Promise<string[]> {
  const view = await entityView(repoId, "branches");
  const section = view?.sections.find((s) => s.id === "branches") ?? null;
  return branchOptions(section).map((option) => option.name);
}

/** What a run ran with, from its source view's *Parameters* section: a hidden value says so. */
export async function readRunParameters(runId: string): Promise<StartValue[]> {
  const view = await entityView(runId, "parameters");
  const section = view?.sections.find((s) => s.id === "parameters");
  return (section?.blocks ?? []).flatMap((block) =>
    block.type === "settings" ? block.values.map((v) => ({ name: v.name, value: v.value, secret: v.secret })) : [],
  );
}

export const BRIDGE: PipelinePorts = {
  older: (pipelineId, before) => olderPipelineRuns(pipelineId, before),
  settings: (pipelineId) => pipelineSettings(pipelineId),
  adapters: () => listAdapters(),
  sources: () => listSources(),
  syncStatus: () => syncStatus(),
  jobBranches: (job) => readJobBranches(job),
  submitWrite: (payload, linkTo) => submitWrite(payload, linkTo),
  openExternal: (url) => openExternal(url),
  presets: (pipelineId) => runPresets(pipelineId),
  savePreset: (preset) => saveRunPreset(preset),
  deletePreset: (pipelineId, name) => deleteRunPreset(pipelineId, name),
  repoBranches: (repoId) => readRepoBranches(repoId),
  runParameters: (runId) => readRunParameters(runId),
  listen: (event, handler) => tauriListen(event, handler),
  follower: () => writeFollower(),
};

/** The run page's reads (#853): the pipeline page's, and the run's source view. */
export interface RunPorts extends PipelinePorts {
  /** The run's view (`entity_view`): the whole of it, or the one section named. */
  view: (runId: string, section?: string) => Promise<SourceView | null>;
  /** The last lines of the run's whole log, up to the cap, and its steps (`run_log`, #854). */
  log: (runId: string) => Promise<RunLog | null>;
  /** The run's tests, failed first (`run_tests`, #855). */
  tests: (runId: string) => Promise<RunTests | null>;
  /** One test's last ten outcomes (`test_history`, #855). */
  history: (runId: string, testId: string) => Promise<TestOutcome[] | null>;
  /** One artifact folder, `""` the root (`run_artifacts`, #856). */
  artifacts: (runId: string, path: string) => Promise<ArtifactEntry[] | null>;
  /** The head of one text artifact (`artifact_preview`, #856). */
  preview: (runId: string, path: string) => Promise<ArtifactPreview | null>;
  /** Stream one into the downloads folder; the saved path (`download_run_file`, D8). */
  download: (runId: string, target: DownloadTarget) => Promise<string | null>;
  /** The projects a ticket can go in, for *Create ticket* (`list_projects`, #1451). */
  projects: () => Promise<Project[]>;
  /** The run's confirmed links, either direction (`get_entity`'s `links`): the tickets filed from it (#1451). */
  links: (runId: string) => Promise<LinkEntry[]>;
  /** The tickets filed from the run that it does not link yet (`owed_tickets`, #1953): on their way after a reload. */
  owedTickets: (runId: string) => Promise<QueuedWrite[]>;
}

export const RUN_BRIDGE: RunPorts = {
  ...BRIDGE,
  view: (runId, section) => entityView(runId, section ?? null),
  log: (runId) => runLog(runId),
  tests: (runId) => runTests(runId),
  history: (runId, testId) => testHistory(runId, testId),
  artifacts: (runId, path) => runArtifacts(runId, path === "" ? null : path),
  preview: (runId, path) => artifactPreview(runId, path),
  download: (runId, target) => downloadRunFile(runId, target),
  projects: () => listProjects(),
  links: (runId) => getEntity(runId).then((detail) => detail.links),
  owedTickets: (runId) => owedTickets(runId),
};

/**
 * Hear `source`'s sync statuses (#1450), every source's where it is `null`
 * (#1451): `heard` with each, `finished` once a sync of it ends. Answers the
 * stop, which also stops a listen still being set up, so a page that goes
 * first leaves no listener behind.
 */
export function listenSynced(
  ports: Pick<PipelinePorts, "listen">,
  source: string | null,
  heard: (row: SourceSyncStatus) => void,
  finished: () => void,
): () => void {
  let dead = false;
  let off: (() => void) | null = null;
  ports
    .listen(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (dead || (source !== null && row.source_id !== source)) return;
      heard(row);
      if (!row.running) finished();
    })
    .then((stop) => (dead ? stop() : (off = stop)))
    .catch(() => {});
  return () => {
    dead = true;
    off?.();
  };
}
