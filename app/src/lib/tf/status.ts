/**
 * The status bar's per-source readings (#809, R-1.5), in prototype-6's words:
 *
 *     Jira synced 4 min ago · Confluence 4 min ago ·
 *     TeamCity: sign-in failed 38 min ago (401) [Re-enter password]
 *
 * The first source says *synced* and the rest leave the verb out, as the
 * prototype does. A source whose credential the reader has to fix (rejected,
 * or none stored) is drawn in err tone and carries *Re-enter password*. An
 * unreachable server is said in plain words and nothing more: the scheduler
 * retries it on its own (P7), and there is nothing to re-enter.
 */
import type { AuthState, CredentialHealth, SourceSummary, SourceSyncStatus } from "../ipc/sources";
import { ago } from "../shell/time";

export interface SourceReading {
  id: string;
  text: string;
  /** The credential needs the reader: err tone and *Re-enter password*. */
  failed: boolean;
}

/** What a credential the reader has to fix is called, by state. */
const FAILURE: Partial<Record<AuthState, (when: string) => string>> = {
  unauthorized: (when) => `sign-in failed ${when} (401)`,
  missing_secret: () => "no password stored",
};

export function sourceReadings(
  sources: Pick<SourceSummary, "id" | "display_name">[] | null,
  health: CredentialHealth[],
  sync: SourceSyncStatus[],
  now: Date = new Date(),
): SourceReading[] {
  // The source list names them; until it answers, the health store's ids
  // are still every source there is, and an id is a true name.
  const named =
    sources ?? health.map((row) => ({ id: row.source_id, display_name: row.source_id }));
  return named.map((source, index) => {
    const state = health.find((row) => row.source_id === source.id);
    const run = sync.find((row) => row.source_id === source.id);
    const name = source.display_name || source.id;
    const failure = state ? FAILURE[state.state] : undefined;
    if (state && failure) {
      const failedAt =
        run?.last_outcome === "unauthorized" ? run.last_finished_at : state.checked_at;
      return { id: source.id, text: `${name}: ${failure(ago(failedAt, now))}`, failed: true };
    }
    if (run?.running) return { id: source.id, text: `${name} syncing…`, failed: false };
    if (!run?.last_finished_at) {
      return { id: source.id, text: `${name} not synced yet`, failed: false };
    }
    if (state?.state === "unreachable") {
      return { id: source.id, text: `${name} unreachable, synced ${ago(run.last_finished_at, now)}`, failed: false };
    }
    const verb = index === 0 ? " synced" : "";
    return { id: source.id, text: `${name}${verb} ${ago(run.last_finished_at, now)}`, failed: false };
  });
}

/** *3 queued writes*, *1 queued write*, *No queued writes*. */
export function queuedWords(pending: number): string {
  if (pending === 0) return "No queued writes";
  return `${pending} queued write${pending === 1 ? "" : "s"}`;
}
