/**
 * The M0 IPC surface: one function per `#[tauri::command]` in
 * `crates/knobas-app/src/commands.rs`, plus hand-written mirrors of the Rust
 * types that cross the bridge.
 *
 * These mirrors are written by hand on purpose. `tauri-specta`, which would
 * generate them, is still `2.0.0-rc.*` — an M0 dependency on a release
 * candidate buys nothing here, since the surface is five commands wide. If it
 * stabilises, generation can replace this file without changing a single
 * shape: what is declared below is exactly the serde output of the Rust types.
 *
 * Argument names are camelCase because Tauri renames command arguments that
 * way by default; `sourceId` here is `source_id` in Rust. *Struct fields* are
 * not renamed — they arrive in their Rust snake_case spelling, which is why
 * the interfaces below are snake_case while the functions are not.
 */
import { invoke } from "@tauri-apps/api/core";

/** One full-text search result — `knobas_db::search::SearchHit`. */
export interface SearchHit {
  /** `"<namespace>:<key>"`, e.g. `"mock:PAY-231"`. */
  entity_id: string;
  kind: string;
  source_id: string;
  title: string;
  /**
   * XSS-UNSAFE. `ts_headline` output: `<b>` marks wrapped around *unescaped*
   * source text. Render it as text (`{snippet}`), never with `{@html}`.
   */
  snippet: string;
  rank: number;
  /** RFC 3339 timestamp. */
  synced_at: string;
}

/** What one sync run did — `knobas_sync::SyncReport`. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
  /** Opaque to the frontend; hand it back to resume an incremental sync. */
  cursor: string;
}

/** One line of the activity log — `knobas_core::activity::ActivityRow`. */
export interface ActivityRow {
  id: number;
  /** RFC 3339 timestamp. */
  at: string;
  /** `"user"`, or `"sync:<source_id>"`. */
  actor: string;
  verb: string;
  entity_id: string | null;
  /** Free-form jsonb; always an object, never a JSON null. */
  detail: Record<string, unknown>;
}

/** Liveness probe: resolves to `"pong"` once the backend is up. */
export function ping(): Promise<string> {
  return invoke<string>("ping");
}

/**
 * Register the mock source if it is not registered yet, then run a full sync
 * from it. Idempotent: calling it twice leaves one source and the same items.
 */
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}

/** Run one sync for an already-registered source. In M0 only `"mock"` exists. */
export function syncNow(sourceId: string): Promise<SyncReport> {
  return invoke<SyncReport>("sync_now", { sourceId });
}

/**
 * The best `limit` matches for `q`, ranked. `q` is user text in the
 * `websearch_to_tsquery` dialect — quoted phrases, `or`, leading `-`.
 */
export function search(q: string, limit: number): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search", { q, limit });
}

/** The `limit` most recent activity lines, newest first. */
export function recentActivity(limit: number): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit });
}

/**
 * Every command rejects with the Rust `Err(String)` as-is, but `catch` binds
 * it as `unknown`. This turns whatever arrived into something displayable.
 */
export function ipcErrorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return JSON.stringify(error);
}
