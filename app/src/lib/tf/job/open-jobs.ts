/**
 * Which job an item belongs to (#834, shared with Code in #837): a list of
 * items from everywhere names each item's job as its own ticket, else the
 * first open job (a live ticket context, the current one first) whose
 * `job_reach` holds it; nothing reaches it, *no job*.
 *
 * {@link readOpenJobs} makes the reads, {@link jobIndex} answers from them.
 */
import type { ContextRow, EntityDetail, Reached } from "../../ipc/entity";
import { keyOf, resolveTicket, type SourceFacts } from "./job";

/** One job and what it reaches: a ticket entity id and its `job_reach` read. */
export interface OpenJob {
  id: string;
  reached: readonly Pick<Reached, "entity_id">[];
}

/** Which job an item belongs to, and whether a job holds it. */
export interface JobIndex {
  /** The job key the row names, or `null` for *no job*. */
  jobOf(item: { entity_id: string | null; kind: string | null }): string | null;
  /** Whether the job with this key holds the item: its own ticket, or what it reaches. */
  holds(job: string, item: { entity_id: string | null }): boolean;
}

/**
 * The jobs' reaches, inverted. An item about a ticket belongs to that ticket;
 * anything else to the first job (in the order given) whose reach holds it.
 */
export function jobIndex(jobs: readonly OpenJob[]): JobIndex {
  const owner = new Map<string, string>();
  const held = new Map<string, Set<string>>();
  for (const job of jobs) {
    const key = keyOf(job.id);
    const ids = new Set([job.id, ...job.reached.map((row) => row.entity_id)]);
    if (!held.has(key)) held.set(key, ids);
    for (const id of ids) if (!owner.has(id)) owner.set(id, key);
  }
  return {
    jobOf(item) {
      if (item.entity_id === null) return null;
      if (item.kind === "ticket") return keyOf(item.entity_id);
      return owner.get(item.entity_id) ?? null;
    },
    holds(job, item) {
      return item.entity_id !== null && (held.get(job)?.has(item.entity_id) ?? false);
    },
  };
}

/** The reads {@link readOpenJobs} makes. */
export interface OpenJobPorts {
  listContexts: () => Promise<ContextRow[]>;
  listSources: () => Promise<SourceFacts[]>;
  getEntity: (entityId: string) => Promise<EntityDetail>;
  jobReach: (entityId: string) => Promise<Reached[]>;
}

/**
 * How many jobs' reaches are read to name a row's job: the open jobs (the
 * ticket contexts opening a job promotes), newest first.
 */
export const JOB_LIMIT = 30;

/**
 * The open jobs, the current one (a ticket key) first, each with what it
 * reaches. A read that fails reaches nothing; nothing here throws.
 */
export async function readOpenJobs(ports: OpenJobPorts, current: string | null): Promise<OpenJob[]> {
  const contexts = await ports.listContexts().catch(() => [] as ContextRow[]);
  let ids = contexts
    .filter((row) => row.kind === "ticket" && row.anchor_id !== null && row.archived_at === null)
    .map((row) => row.anchor_id as string);
  if (current !== null) {
    let id = ids.find((candidate) => keyOf(candidate) === current) ?? null;
    if (id === null) {
      const sources = await ports.listSources().catch(() => [] as SourceFacts[]);
      const found = await resolveTicket(current, sources, ports.getEntity).catch(() => null);
      id = found?.row.entity_id ?? null;
    }
    if (id !== null) ids = [id, ...ids.filter((candidate) => candidate !== id)];
  }
  ids = ids.slice(0, JOB_LIMIT);
  const answers = await Promise.allSettled(ids.map((id) => ports.jobReach(id)));
  return ids.map((id, at) => {
    const answer = answers[at]!;
    return { id, reached: answer.status === "fulfilled" ? answer.value : [] };
  });
}
