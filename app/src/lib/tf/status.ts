/**
 * The status bar's per-source readings (#809, R-1.5), in prototype-6's words:
 *
 *     Jira synced 4 min ago · Confluence 4 min ago ·
 *     TeamCity: sign-in failed 38 min ago (401) [Re-enter password]
 *
 * The first source says *synced* and the rest leave the verb out, as the
 * prototype does. A source whose credential the reader has to fix (rejected,
 * or none stored) is drawn in err tone and carries *Re-enter password*, or
 * *Re-enter token* for a token source (#1405). An
 * unreachable server is said in plain words and nothing more: the scheduler
 * retries it on its own (P7), and there is nothing to re-enter.
 */
import type { AuthState, CredentialHealth, SourceSummary, SourceSyncStatus } from "../ipc/sources";
import { ago } from "../shell/time";
import { reenterWords, secretNoun } from "./settings/settings";

export interface SourceReading {
  id: string;
  text: string;
  /**
   * The credential needs the reader: err tone and this link, *Re-enter
   * password* or *Re-enter token*. `null` for a source that needs nothing.
   */
  fix: string | null;
}

/** What a credential the reader has to fix is called, by state, in the source's noun. */
const FAILURE: Partial<Record<AuthState, (when: string, noun: string) => string>> = {
  unauthorized: (when) => `sign-in failed ${when} (401)`,
  missing_secret: (_, noun) => `no ${noun} stored`,
};

/** What a reading needs of a source: its name, and what its credential is called. */
type Named = Pick<SourceSummary, "id" | "display_name"> & Partial<Pick<SourceSummary, "auth_kind">>;

export function sourceReadings(
  sources: Named[] | null,
  health: CredentialHealth[],
  sync: SourceSyncStatus[],
  now: Date = new Date(),
): SourceReading[] {
  // The source list names them; until it answers, the health store's ids
  // are still every source there is, and an id is a true name.
  const named: Named[] =
    sources ?? health.map((row) => ({ id: row.source_id, display_name: row.source_id }));
  return named.map((source, index) => {
    const state = health.find((row) => row.source_id === source.id);
    const run = sync.find((row) => row.source_id === source.id);
    const name = source.display_name || source.id;
    const failure = state ? FAILURE[state.state] : undefined;
    if (state && failure) {
      const failedAt =
        run?.last_outcome === "unauthorized" ? run.last_finished_at : state.checked_at;
      const text = `${name}: ${failure(ago(failedAt, now), secretNoun(source.auth_kind))}`;
      return { id: source.id, text, fix: reenterWords(source.auth_kind) };
    }
    if (run?.running) return { id: source.id, text: `${name} syncing…`, fix: null };
    if (!run?.last_finished_at) {
      return { id: source.id, text: `${name} not synced yet`, fix: null };
    }
    if (state?.state === "unreachable") {
      return { id: source.id, text: `${name} unreachable, synced ${ago(run.last_finished_at, now)}`, fix: null };
    }
    const verb = index === 0 ? " synced" : "";
    return { id: source.id, text: `${name}${verb} ${ago(run.last_finished_at, now)}`, fix: null };
  });
}

/** *3 queued writes*, *1 queued write*, *No queued writes*. */
export function queuedWords(pending: number): string {
  if (pending === 0) return "No queued writes";
  return `${pending} queued write${pending === 1 ? "" : "s"}`;
}

/**
 * The bar's queue control (#1409): *1 queued write · 3 need you*.
 *
 * `decisions` is held plus refused (`decisionsIn`): writes that never move on
 * their own, which the bar is where to ask about (CONTEXT.md, *Pending
 * write*). `decide` is `null` at zero, and with nothing waiting either the
 * control reads the idle *No queued writes*. With decisions and nothing
 * waiting the count says *0 queued writes*, so the label still names what the
 * control opens.
 */
export function queueLabel(pending: number, decisions: number): { queued: string; decide: string | null } {
  if (decisions === 0) return { queued: queuedWords(pending), decide: null };
  return { queued: pending === 0 ? "0 queued writes" : queuedWords(pending), decide: `${decisions} need you` };
}
