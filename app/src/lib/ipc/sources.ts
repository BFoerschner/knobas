/** Sources, sync and diagnostics — `crates/knobas-app/src/commands/sources.rs`. */
import { Channel, invoke } from "@tauri-apps/api/core";

/** What one sync run did — `knobas_sync::SyncReport`. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
  /**
   * Rows a **full** sync tombstoned because the run did not see them
   * (hard-delete reconciliation). Always 0 for an incremental run, for an
   * adapter whose full sync is a bounded window, and for a full sync that
   * emitted nothing.
   */
  swept: number;
  /** Opaque to the frontend; hand it back to resume an incremental sync. */
  cursor: string;
}

/**
 * Register the mock source if it is not registered yet, then run a full sync
 * from it. Idempotent: calling it twice leaves one source and the same items.
 */
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}

/**
 * Whether a source's credential works — `knobas_sync::AuthState`.
 *
 * `unauthorized` is the one the UI must act on: it is what puts *Re-enter
 * password* on the row rather than a shrug, and the scheduler never retries it
 * on its own (interfaces §8 P7). `missing_secret` is configured-but-no-secret,
 * which is a different prompt from a secret that was rejected.
 */
export type AuthState =
  | "ok"
  | "unauthorized"
  | "unreachable"
  | "missing_secret"
  | "unknown";

/**
 * One source's credential health — `knobas_sync::CredentialHealth`, the
 * payload of `EVENTS.sourceHealth` and the return of `credentialHealth()`.
 */
export interface CredentialHealth {
  source_id: string;
  state: AuthState;
  /** RFC 3339, or null if it has never been checked. */
  checked_at: string | null;
  /** One line for the sources view. Never a secret. */
  detail: string | null;
  /** RFC 3339 — feeds the PAT expiry countdown. Null if the source will not say. */
  secret_expires_at: string | null;
}

/** How a run ended — `knobas_sync::SyncOutcome`. */
export type SyncOutcome = "ok" | "unauthorized" | "unreachable" | "error";

/**
 * Why a run happened — `knobas_sync::SyncTrigger`, the `trigger` of a
 * `knobas.sync_run` row.
 *
 * The diagnostics list labels a run with it: a scheduled poll, the user's
 * *Sync now*, or the initial sync the first-run wizard drew a progress bar
 * for.
 */
export type SyncTrigger = "schedule" | "manual" | "first_run";

/**
 * The coarse state one source's syncing is in — `knobas_sync::SourceSyncStatus`,
 * the payload of `EVENTS.syncState`.
 *
 * Coarse by rule: a transition each, at most a handful per run. Per-item
 * progress is {@link SyncProgress} on a channel and nowhere else.
 */
export interface SourceSyncStatus {
  source_id: string;
  running: boolean;
  run_id: number | null;
  /** RFC 3339, on the `running: true` transition. */
  started_at: string | null;
  /** RFC 3339, on the terminal transition. */
  last_finished_at: string | null;
  last_outcome: SyncOutcome | null;
  /** Always null until stream F's scheduler exists. */
  next_run_at: string | null;
  /** Always null until stream F writes and honours backoff. */
  backoff_until: string | null;
}

/** Where a run is — `knobas_sync::SyncPhase`. */
export type SyncPhase = "started" | "fetching" | "writing" | "finished" | "failed";

/** One progress message — `knobas_sync::SyncProgress`. */
export interface SyncProgress {
  /** `knobas.sync_run.id`, so two runs on two channels stay distinguishable. */
  run_id: number;
  source_id: string;
  phase: SyncPhase;
  items: number;
  elapsed_ms: number;
  message: string | null;
}

/**
 * Start a sync of one configured source; resolves with its `sync_run.id` as
 * soon as the run is recorded, **not** when it finishes — watch
 * `EVENTS.syncState` for that.
 *
 * Use {@link syncNowWithProgress} when you are drawing per-item progress.
 * There are two functions rather than one optional argument because
 * `Option<Channel<_>>` is not a valid Tauri 2.11 command argument (`Channel`
 * has no `Deserialize` impl); the Rust side splits for the same reason, and
 * the evidence is in `crates/knobas-app/tests/ipc.rs`.
 */
export function syncNow(sourceId: string): Promise<number> {
  return invoke<number>("sync_now", { sourceId });
}

/**
 * {@link syncNow}, reporting per-item progress on `progress`.
 *
 * The channel is required. A caller that only needs to know a run started
 * should call {@link syncNow} and listen to `EVENTS.syncState`: per-item
 * progress goes on the channel and nowhere else, and events carry coarse state
 * only.
 */
export function syncNowWithProgress(
  sourceId: string,
  progress: Channel<SyncProgress>,
): Promise<number> {
  return invoke<number>("sync_now_with_progress", { sourceId, progress });
}
