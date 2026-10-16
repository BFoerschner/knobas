/**
 * The job's section reads (#817), in one place: the job page draws its
 * sections from them, and the Tickets preview's *Around this ticket* counts
 * them (#1030, story 31), so the two count the same rows.
 *
 * Every read goes through the job-reach read (`job_reach`, and
 * `list_entities` scoped by `reach`), which takes the ticket's entity id and
 * needs no stored context: nothing here promotes one.
 */
import type { EntityFilter, EntityOrder, EntityPage, KindTally, Reached } from "../../ipc/entity";
import { CODE_KINDS } from "./job";

/** How many rows a section reads: a job reaches tens, not thousands. */
export const SECTION_LIMIT = 200;
/** How many reached assets are read one by one for the Assets section. */
export const ASSET_LIMIT = 50;

/** The four sections read from the mirror, each with its kinds and order. Assets come from the reach itself. */
export const SECTION_READS = {
  code: { kinds: CODE_KINDS, order: "updated_desc" },
  builds: { kinds: ["build"], order: "latest_per_parent" },
  pages: { kinds: ["page"], order: "updated_desc" },
  monitors: { kinds: ["monitor"], order: "updated_desc" },
} as const satisfies Record<string, { kinds: readonly string[]; order: EntityOrder }>;

/** A read of `kinds`, scoped to what `id` reaches, or unscoped with `null`. */
export function reachFilter(id: string | null, kinds: readonly string[], order: EntityOrder): EntityFilter {
  return {
    sources: [],
    kinds: [...kinds],
    updated_within_days: null,
    order,
    context: null,
    project: null,
    include_deleted: false,
    reach: id,
  };
}

/** The two reads the sections are counted from. */
export interface ReachPorts {
  listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
  jobReach: (entityId: string) => Promise<Reached[]>;
}

/**
 * What the job's sections would hold for ticket `id`, counted by kind and
 * tone: the Code, Builds, Pages and Monitors reads as the job makes them, and
 * the reached assets the Assets section reads (with no tone: their health is
 * read one asset at a time, and only on the job). A read that fails counts
 * nothing, as its section draws nothing.
 */
export async function reachTally(ports: ReachPorts, id: string): Promise<KindTally[]> {
  const reads = Object.values(SECTION_READS).map(({ kinds, order }) =>
    ports
      .listEntities(reachFilter(id, kinds, order), SECTION_LIMIT, 0)
      .then((page) => page.rows)
      .catch(() => []),
  );
  const assets = ports
    .jobReach(id)
    .then((rows) => rows.filter((row) => row.kind === "asset").slice(0, ASSET_LIMIT))
    .catch(() => [] as Reached[]);
  const rows = (await Promise.all(reads)).flat();
  const counted = new Map<string, KindTally>();
  const add = (kind: string, tone: KindTally["tone"]) => {
    const key = `${kind}\u0000${tone ?? ""}`;
    const held = counted.get(key) ?? { kind, tone, count: 0 };
    held.count += 1;
    counted.set(key, held);
  };
  for (const row of rows) add(row.kind, row.state?.tone ?? null);
  for (const asset of await assets) add(asset.kind, null);
  return [...counted.values()];
}
