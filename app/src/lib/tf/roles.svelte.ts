/**
 * What the shell still reads of the source list's role claims, now that the
 * mirror answers a role and the shell asks for one and filters nothing
 * (ADR-0029 as amended on 2026-10-19, #1293, #1300; `CONTEXT.md` **Role**).
 *
 * No surface keeps or drops a row here. Every read a role's surface is drawn
 * from is narrowed to the role's fillers by the mirror: the pipelines, the
 * Docs tree and the Monitors roster by themselves, the entity list and the
 * ticket board by the `role` they are asked with. A page draws what its read
 * answered; `lib/shell/house-rules.test.ts` pins that no page under `lib/tf/`
 * filters rows by source.
 *
 * What is left is not a row filter:
 *
 * - {@link SourceRoles.answers}: whether a source answers an optional read of
 *   a role (`translate_query`, #1208), so Tickets offers a saved view's
 *   editable copy only where it is claimed. It needs the list the shell read,
 *   which `TfShell.svelte` hands to {@link sourceRoles} once the database
 *   answers and again when a source is added or removed or a sync finishes.
 * - {@link claims}: whether a source claims a role, read off a source the
 *   caller holds (the job's Builds staleness warning, `job.ts`).
 *
 * A role a source claims is not filled where its records said otherwise
 * (#1199, ADR-0028): a role whose declared paths missed on the source's
 * records (`SourceSummary.role_misses`) answers no optional read.
 */
import type { RoleClaim, RoleMiss } from "../ipc/sources";

/** The built-in roles a surface is drawn for, as `knobas_source::role` spells them. */
export const CODE_HOST = "code-host";
export const CI = "ci";
export const WIKI = "wiki";
export const MONITORING = "monitoring";
export const TICKET_SYSTEM = "ticket-system";
/** The ticket system's optional read that turns a saved view's query into conditions (#1208). */
export const TRANSLATE_QUERY = "translate_query";

/** What a surface needs to know of one source: its id and the roles it claims. */
export interface ClaimingSource {
  id: string;
  /** What a person calls it: what a card names when two sources hold its key (#1476). */
  display_name?: string;
  /** Carried as the source list has it and never read: a surface draws by role. */
  adapter_kind?: string;
  roles?: readonly RoleClaim[];
  /** The claimed roles whose declared paths missed on the source's records (#1199). */
  role_misses?: readonly RoleMiss[];
}

/** Whether `source` claims `role`. A source that names no claims claims nothing. */
export function claims(source: ClaimingSource, role: string): boolean {
  return (source.roles ?? []).some((claim) => claim.role === role);
}

/** Whether `source` fills `role`: claims it, and its records did not miss one of its paths (#1199). */
export function fills(source: ClaimingSource, role: string): boolean {
  return claims(source, role) && !(source.role_misses ?? []).some((miss) => miss.role === role);
}

export interface SourceRoles {
  /** The source list the shell read last. */
  take(sources: readonly ClaimingSource[]): void;
  /** Back to knowing nothing: no source answers an optional read. */
  forget(): void;
  /**
   * Whether `sourceId` answers the optional read `read` of `role` (#1208,
   * `translate_query`): it fills the role and its claim names the read. Not
   * knowing is no: an optional read is offered only where it is claimed, as
   * an undeclared write is not drawn (ADR-0020).
   */
  answers(role: string, sourceId: string, read: string): boolean;
  /** What a person calls `sourceId` (#1476): its display name, or the id when the list never said. */
  nameOf(sourceId: string): string;
}

export function createSourceRoles(): SourceRoles {
  const state = $state<{ sources: ClaimingSource[] | null }>({ sources: null });

  return {
    take(sources) {
      state.sources = sources.map((row) => ({
        id: row.id,
        ...(row.display_name === undefined ? {} : { display_name: row.display_name }),
        roles: [...(row.roles ?? [])],
        role_misses: [...(row.role_misses ?? [])],
      }));
    },
    forget() {
      state.sources = null;
    },
    answers(role, sourceId, read) {
      const source = state.sources?.find((row) => row.id === sourceId);
      if (source === undefined || !fills(source, role)) return false;
      return (source.roles ?? []).some((claim) => claim.role === role && claim.reads.includes(read));
    },
    nameOf(sourceId) {
      return state.sources?.find((row) => row.id === sourceId)?.display_name ?? sourceId;
    },
  };
}

/** The one the window uses. */
export const sourceRoles = createSourceRoles();
