/**
 * Dependencies from a ticket (#874, R-11.10, spec #808 *Dependencies*: "From
 * a ticket, the job-reach read filters it"): the top-bar read, kept to the
 * ticket itself, what the job-reach read (`job_reach`, R-2.8) answers, and
 * the job's pipelines (`job_pipelines`, #850), since the reach holds a
 * pipeline's runs and Dependencies draws the pipeline. A relation stays when
 * both its ends do. Pure: the reads in, a `Dependencies` of the same shape out.
 */
import type { Dependencies, DepItem } from "../../ipc/assets";
import type { JobPipeline, Reached } from "../../ipc/entity";

/** The ticket a map opens from: its entity id and the key a person reads. */
export interface MapTicket {
  id: string;
  key: string;
}

export function reachedPart(
  read: Dependencies,
  ticket: MapTicket,
  reach: readonly Reached[],
  pipelines: readonly JobPipeline[],
): Dependencies {
  const keep = new Set<string>([ticket.id]);
  for (const row of reach) keep.add(row.entity_id);
  for (const row of pipelines) keep.add(row.pipeline_id);
  const items = read.items.filter((entry) => keep.has(entry.id));
  // A ticket with no confirmed link to the estate is in no read: draw it by its key.
  if (!items.some((entry) => entry.id === ticket.id)) {
    const own: DepItem = {
      id: ticket.id,
      kind: "ticket",
      name: ticket.key,
      health: null,
      state: null,
      monitors: 0,
      environment: null,
      owner: null,
    };
    items.unshift(own);
  }
  const relations = read.relations.filter((relation) => keep.has(relation.from) && keep.has(relation.to));
  return { layers: read.layers, kinds: read.kinds, items, relations };
}
