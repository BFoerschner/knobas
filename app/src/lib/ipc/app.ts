/**
 * App lifecycle and status — `crates/knobas-app/src/commands/app.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a state or a field added on one side only fails
 * `cargo test`, not merely `svelte-check`.
 */
import { invoke } from "@tauri-apps/api/core";

/**
 * How far the database has got — `commands::app::DbState`, tagged on `state`.
 *
 * A discriminated union, so `message` and `detail` are reachable only after
 * the check that identifies which state this is.
 */
export type DbState =
  | { state: "starting"; detail: string | null }
  | { state: "migrating" }
  | { state: "ready" }
  | { state: "failed"; message: string };

/** What the shell needs to decide what to draw — `commands::app::AppStatus`. */
export interface AppStatus {
  db: DbState;
  /** No source configured and the first run was never completed (§14a). */
  first_run: boolean;
  /** The `--demo` profile: its own data dir, port and keychain suffix (P13). */
  demo: boolean;
  source_count: number;
  app_version: string;
}

/**
 * Liveness probe.
 *
 * Answers as soon as the webview can call, which is now *before* the database
 * is up: a `"pong"` says the backend is running and nothing about PostgreSQL.
 * Ask {@link appStatus} for that.
 */
export function ping(): Promise<string> {
  return invoke<string>("ping");
}

/** The current lifecycle state, plus the counts the shell shows beside it. */
export function appStatus(): Promise<AppStatus> {
  return invoke<AppStatus>("app_status");
}

/**
 * Arm event emission: the backend replays the current `db:state` (gotcha 9).
 *
 * Call it **after** registering the `db:state` listener, never before — that
 * ordering is the entire reason this command exists.
 */
export function frontendReady(): Promise<void> {
  return invoke<void>("frontend_ready");
}

/**
 * Start the database again after a failure — the boot screen's *Retry*.
 *
 * Idempotent on the backend: a second call while a bring-up is already running
 * starts nothing, which is the right answer to "start the database" when it is
 * already starting.
 */
export function retryDatabase(): Promise<void> {
  return invoke<void>("retry_database");
}
