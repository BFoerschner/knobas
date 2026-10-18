/**
 * Which sources each surface draws: the ones that claim its role (#1196,
 * ADR-0029, `CONTEXT.md` **Role**).
 *
 * The shell draws a role, never a source. Code draws the sources that claim
 * `code-host`, Builds `ci`, Docs `wiki` and Monitors `monitoring`, whatever
 * their adapter kind, so a source of an adapter kind the shell has never seen
 * appears on every surface it claims. A source that claims none of them (the
 * compiled-in mock claims nothing, #1193) gets none of those surfaces.
 *
 * The claims ride on `list_sources` (`SourceSummary.roles`, §10.8), which the
 * shell reads once the database answers and again when a source is added or
 * removed (`TfShell.svelte`); it hands each answer to {@link sourceRoles}.
 * Module-level, like `kindRegistry` and `health`, because four pages read it
 * and the shell is the one place that already holds the list.
 *
 * Two cases narrow nothing, because the shell does not know their claims:
 * the time before the list has been read (or while it fails), and a row whose
 * source the list does not name, such as the items a removed source left
 * behind. Whether those stay drawn is not a question about roles.
 */
import type { RoleClaim } from "../ipc/sources";

/** The built-in roles a surface is drawn for, as `knobas_source::role` spells them. */
export const CODE_HOST = "code-host";
export const CI = "ci";
export const WIKI = "wiki";
export const MONITORING = "monitoring";

/** What a surface needs to know of one source: its id and the roles it claims. */
export interface ClaimingSource {
  id: string;
  /** Carried as the source list has it and never read: a surface draws by role. */
  adapter_kind?: string;
  roles?: readonly RoleClaim[];
}

/** Whether `source` claims `role`. A source that names no claims claims nothing. */
export function claims(source: ClaimingSource, role: string): boolean {
  return (source.roles ?? []).some((claim) => claim.role === role);
}

export interface SourceRoles {
  /** The source list the shell read last. */
  take(sources: readonly ClaimingSource[]): void;
  /** Back to knowing nothing: every surface draws what it reads. */
  forget(): void;
  /** Whether a surface of `role` draws the rows of `sourceId`. */
  draws(role: string, sourceId: string): boolean;
  /** `rows` as a surface of `role` draws them: the same array when nothing is dropped. */
  keep<T extends { source_id: string }>(role: string, rows: T[]): T[];
}

export function createSourceRoles(): SourceRoles {
  const state = $state<{ sources: ClaimingSource[] | null }>({ sources: null });

  function draws(role: string, sourceId: string): boolean {
    if (state.sources === null) return true;
    const source = state.sources.find((row) => row.id === sourceId);
    return source === undefined || claims(source, role);
  }

  return {
    take(sources) {
      state.sources = sources.map((row) => ({ id: row.id, roles: [...(row.roles ?? [])] }));
    },
    forget() {
      state.sources = null;
    },
    draws,
    keep(role, rows) {
      const kept = rows.filter((row) => draws(role, row.source_id));
      return kept.length === rows.length ? rows : kept;
    },
  };
}

/** The one the window uses. */
export const sourceRoles = createSourceRoles();
