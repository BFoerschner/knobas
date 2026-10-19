/**
 * The job's section reads (#817), in one place: the job page draws its
 * sections from them, and the Tickets preview's *Around this ticket* counts
 * them (#1030, story 31), so the two count the same rows.
 *
 * Every read goes through the job-reach read (`job_reach`, and
 * `list_entities` scoped by `reach`), which takes the ticket's entity id and
 * needs no stored context: nothing here promotes one.
 *
 * Each section is a role's (ADR-0029): it asks the mirror for its role, so
 * the read itself narrows to the role's fillers and its rows are drawn as
 * read (ADR-0029 as amended on 2026-10-19). Builds names no kinds, and the
 * mirror answers the run kinds CI's fillers declare (#1295); Code, Pages and
 * Monitors name theirs within the role (#1296, #1297, #1298).
 */
import type { EntityFilter, EntityOrder, EntityPage, EntityRow, KindTally, Reached } from "../../ipc/entity";
import type { KindRegistry } from "../../shell/kind-registry.svelte";
import { CI, CODE_HOST, MONITORING, WIKI } from "../roles.svelte";
import { CODE_KINDS } from "./job";

/** How many rows a section reads: a job reaches tens, not thousands. */
export const SECTION_LIMIT = 200;
/** How many reached assets are read one by one for the Assets section. */
export const ASSET_LIMIT = 50;

/**
 * One section's read: the role it asks for, the kinds it names within the
 * role (none: the role's kinds) and their order.
 */
export interface SectionRead {
  role: string;
  kinds: readonly string[];
  order: EntityOrder;
}

export type Section = "code" | "builds" | "pages" | "monitors";

/** What the job needs of the adapters' claims: the CI role's run kinds, to route a run (#1256). */
export type FrameKinds = Pick<KindRegistry, "frameKinds" | "load">;

export { runKindsOf } from "./job";

/**
 * The four sections read from the mirror. Assets come from the reach itself.
 * Builds names no kinds: the CI role's run kinds are the mirror's answer, not
 * the shell's (#1295).
 */
export const SECTION_READS: Readonly<Record<Section, SectionRead>> = {
  code: { role: CODE_HOST, kinds: CODE_KINDS, order: "updated_desc" },
  builds: { role: CI, kinds: [], order: "latest_per_parent" },
  pages: { role: WIKI, kinds: ["page"], order: "updated_desc" },
  monitors: { role: MONITORING, kinds: ["monitor"], order: "updated_desc" },
};

/** One section's rows as the reach read answers them for its role, drawn as read. */
export async function readSection(listEntities: ReachPorts["listEntities"], id: string, read: SectionRead): Promise<EntityRow[]> {
  const page = await listEntities(reachFilter(id, read.kinds, read.order, read.role), SECTION_LIMIT, 0);
  return page.rows;
}

/** A read of `kinds`, scoped to what `id` reaches (or unscoped with `null`) and to `role` when named. */
export function reachFilter(id: string | null, kinds: readonly string[], order: EntityOrder, role: string | null = null): EntityFilter {
  return {
    sources: [],
    kinds: [...kinds],
    updated_within_days: null,
    order,
    context: null,
    project: null,
    include_deleted: false,
    reach: id,
    ...(role === null ? {} : { role }),
  };
}

/** The two reads the sections are counted from. */
export interface ReachPorts {
  listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
  jobReach: (entityId: string) => Promise<Reached[]>;
}

/**
 * What the job's sections would hold for ticket `id`, counted by kind and
 * tone: the Code, Builds, Pages and Monitors reads as the job makes them,
 * each for its role, and
 * the reached assets the Assets section reads (with no tone: their health is
 * read one asset at a time, and only on the job). A read that fails counts
 * nothing, as its section draws nothing.
 */
export async function reachTally(ports: ReachPorts, id: string): Promise<KindTally[]> {
  const reads = Object.values(SECTION_READS).map((read) => readSection(ports.listEntities, id, read).catch(() => []));
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
