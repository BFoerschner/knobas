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
import { credentialFix, secretNoun } from "./settings/settings";

/** A reading's state, which draws its dot. */
export type ReadingTone = "ok" | "info" | "warn" | "err" | "muted";

export interface SourceReading {
  id: string;
  text: string;
  tone: ReadingTone;
  /** When the source last finished a sync; `null` before its first. */
  syncedAt: string | null;
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
      return { id: source.id, text, tone: "err", syncedAt: run?.last_finished_at ?? null, fix: credentialFix(source.auth_kind, state.state) };
    }
    const syncedAt = run?.last_finished_at ?? null;
    if (run?.running) return { id: source.id, text: `${name} syncing…`, tone: "info", syncedAt, fix: null };
    if (!run?.last_finished_at) {
      return { id: source.id, text: `${name} not synced yet`, tone: "muted", syncedAt, fix: null };
    }
    if (state?.state === "unreachable") {
      return { id: source.id, text: `${name} unreachable, synced ${ago(run.last_finished_at, now)}`, tone: "warn", syncedAt, fix: null };
    }
    const verb = index === 0 ? " synced" : "";
    const synced = `${name}${verb} ${ago(run.last_finished_at, now)}`;
    // A good credential that left part of the source out (#2374): amber here as on its Settings row, so the two agree.
    if (state?.state === "ok" && state.detail) return { id: source.id, text: `${synced}, ${state.detail}`, tone: "warn", syncedAt, fix: null };
    return { id: source.id, text: synced, tone: "ok", syncedAt, fix: null };
  });
}

/**
 * The healthy readings as one line, past the two the bar has room to name:
 * *5 synced, oldest 4 min ago*. The oldest sync is the one age worth keeping,
 * because it says whether anything has gone stale. While a run goes it leads,
 * *Syncing 5 sources…* (F-645), since what is happening now is what the bar is
 * looked at for and the page's rows say the same. `text` is the part that has
 * to fit and `rest` what may drop out of sight. `null` for two or fewer, which
 * the bar names one by one.
 */
export function healthyRollup(healthy: readonly SourceReading[], now: Date = new Date()): { text: string; rest: string | null; tone: ReadingTone } | null {
  if (healthy.length <= 2) return null;
  const having = (tone: ReadingTone) => healthy.filter((reading) => reading.tone === tone);
  const synced = having("ok");
  const oldest = synced.map((reading) => reading.syncedAt).filter((at) => at !== null).sort()[0];
  const looks = having("warn").length;
  // What must stay: how many synced, and anything that wants the reader. The rest may drop out of sight.
  const going = having("info").length;
  const count = synced.length === 1 ? [`1 synced ${ago(oldest, now)}`] : synced.length > 1 ? [`${synced.length} synced`] : [];
  const first = [
    ...(going > 0 ? [`Syncing ${going} source${going === 1 ? "" : "s"}…`] : count),
    ...(looks > 0 ? [`${looks} need${looks === 1 ? "s" : ""} a look`] : []),
  ];
  const second = [
    ...(going > 0 ? count : []),
    ...(synced.length > 1 ? [`oldest ${ago(oldest, now)}`] : []),
    ...(having("muted").length > 0 ? [`${having("muted").length} not synced yet`] : []),
  ];
  const lines = first.length > 0 ? [first.join(", "), ...second] : second;
  const tone: ReadingTone = having("warn").length > 0 ? "warn" : having("info").length > 0 ? "info" : synced.length > 0 ? "ok" : "muted";
  return { text: lines[0]!, rest: lines.length > 1 ? lines.slice(1).join(", ") : null, tone };
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
