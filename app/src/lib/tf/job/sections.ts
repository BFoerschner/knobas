/**
 * The job's section reads (#817), in one place: the job page draws its
 * sections from them, and the Tickets preview's *Around this ticket* counts
 * them (#1030, story 31), so the two count the same rows.
 *
 * Every read goes through the job-reach read (`job_reach`, and
 * `list_entities` scoped by `reach`), which takes the ticket's entity id and
 * needs no stored context: nothing here promotes one.
 *
 * Each section is a role's (ADR-0029): its rows are the sources' that fill
 * the role. Builds asks the mirror for the CI role and names no kinds, so the
 * read itself narrows to CI's fillers and the run kinds they declare (#1295,
 * ADR-0029 as amended on 2026-10-19); the other sections are still kept to
 * their role's sources after reading (`sourceRoles.keep`, #1196).
 */
import type { EntityFilter, EntityOrder, EntityPage, EntityRow, KindTally, Reached } from "../../ipc/entity";
import type { KindRegistry } from "../../shell/kind-registry.svelte";
import { CI, CODE_HOST, MONITORING, sourceRoles, WIKI, type SourceRoles } from "../roles.svelte";
import { CODE_KINDS } from "./job";

/** How many rows a section reads: a job reaches tens, not thousands. */
export const SECTION_LIMIT = 200;
/** How many reached assets are read one by one for the Assets section. */
export const ASSET_LIMIT = 50;

/** One section's read: the role it draws, the kinds it asks for and their order. */
export interface SectionRead {
  role: string;
  kinds: readonly string[];
  order: EntityOrder;
  /**
   * Whether the read carries `role`, so the mirror narrows it to the role's
   * fillers (and, with no kinds, to the role's kinds) and its rows are drawn
   * as read (#1295). A section without it is kept to its role's sources after
   * reading, and reads nothing when it names no kinds.
   */
  asked: boolean;
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
  code: { role: CODE_HOST, kinds: CODE_KINDS, order: "updated_desc", asked: false },
  builds: { role: CI, kinds: [], order: "latest_per_parent", asked: true },
  pages: { role: WIKI, kinds: ["page"], order: "updated_desc", asked: false },
  monitors: { role: MONITORING, kinds: ["monitor"], order: "updated_desc", asked: false },
};

/**
 * One section's rows as the reach read answers them: a section that asks
 * for its role as drawn, the others before they are kept to the sources that
 * fill it (`sourceRoles.keep(read.role, rows)`). For those, no kinds is no
 * read, since an empty `kinds` with no role asks for every kind.
 */
export async function readSection(listEntities: ReachPorts["listEntities"], id: string, read: SectionRead): Promise<EntityRow[]> {
  if (!read.asked && read.kinds.length === 0) return [];
  const page = await listEntities(reachFilter(id, read.kinds, read.order, read.asked ? read.role : null), SECTION_LIMIT, 0);
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
 * tone: the Code, Builds, Pages and Monitors reads as the job makes them
 * (Builds asked for the CI role, the others kept to their role's sources), and
 * the reached assets the Assets section reads (with no tone: their health is
 * read one asset at a time, and only on the job). A read that fails counts
 * nothing, as its section draws nothing.
 */
export async function reachTally(
  ports: ReachPorts,
  id: string,
  { roles = sourceRoles }: { roles?: Pick<SourceRoles, "keep"> } = {},
): Promise<KindTally[]> {
  const reads = Object.values(SECTION_READS).map((read) =>
    readSection(ports.listEntities, id, read)
      .then((rows) => (read.asked ? rows : roles.keep(read.role, rows)))
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
