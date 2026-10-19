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
 * the role (`sourceRoles.keep`, #1196), and Builds reads the kinds the CI
 * role's adapters declare the `run` frame on, never one adapter's kind
 * (#1230).
 */
import type { EntityFilter, EntityOrder, EntityPage, EntityRow, KindTally, Reached } from "../../ipc/entity";
import { kindRegistry, type KindRegistry } from "../../shell/kind-registry.svelte";
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
}

export type Section = "code" | "builds" | "pages" | "monitors";

/**
 * The run kind Builds reads while the adapters' claims are not known: the
 * built-in CI adapter's, so a registry that failed to read draws what it drew
 * before #1230, as an unread source list narrows nothing.
 */
const UNKNOWN_RUN_KINDS = ["build"] as const;

/** What the sections need of the adapters' claims. */
export type FrameKinds = Pick<KindRegistry, "frameKinds" | "load">;

/** The four sections read from the mirror, as the claims read now stand. Assets come from the reach itself. */
export function sectionReads(registry: Pick<KindRegistry, "frameKinds"> = kindRegistry): Record<Section, SectionRead> {
  return {
    code: { role: CODE_HOST, kinds: CODE_KINDS, order: "updated_desc" },
    builds: { role: CI, kinds: registry.frameKinds(CI, "run") ?? UNKNOWN_RUN_KINDS, order: "latest_per_parent" },
    pages: { role: WIKI, kinds: ["page"], order: "updated_desc" },
    monitors: { role: MONITORING, kinds: ["monitor"], order: "updated_desc" },
  };
}

/**
 * One section's rows as the reach read answers them, before they are kept to
 * the sources that fill its role (`sourceRoles.keep(read.role, rows)`). No
 * kinds is no read, since an empty `kinds` asks for every kind: no adapter
 * claims CI with a run.
 */
export async function readSection(
  listEntities: ReachPorts["listEntities"],
  id: string,
  { kinds, order }: SectionRead,
): Promise<EntityRow[]> {
  if (kinds.length === 0) return [];
  const page = await listEntities(reachFilter(id, kinds, order), SECTION_LIMIT, 0);
  return page.rows;
}

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
 * tone: the Code, Builds, Pages and Monitors reads as the job makes them (each
 * kept to its role's sources, Builds of the CI role's run kinds), and
 * the reached assets the Assets section reads (with no tone: their health is
 * read one asset at a time, and only on the job). A read that fails counts
 * nothing, as its section draws nothing.
 */
export async function reachTally(
  ports: ReachPorts,
  id: string,
  { registry = kindRegistry, roles = sourceRoles }: { registry?: FrameKinds; roles?: Pick<SourceRoles, "keep"> } = {},
): Promise<KindTally[]> {
  await registry.load();
  const reads = Object.values(sectionReads(registry)).map((read) =>
    readSection(ports.listEntities, id, read)
      .then((rows) => roles.keep(read.role, rows))
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
