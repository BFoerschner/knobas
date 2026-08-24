/** Sources, sync and diagnostics — `crates/knobas-app/src/commands/sources.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** What one sync run did — `knobas_sync::SyncReport`. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
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

/** Run one sync for an already-registered source. */
export function syncNow(sourceId: string): Promise<SyncReport> {
  return invoke<SyncReport>("sync_now", { sourceId });
}
