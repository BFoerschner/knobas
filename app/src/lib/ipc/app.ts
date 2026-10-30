/**
 * App lifecycle and status — `crates/knobas-app/src/commands/app.rs`.
 *
 * The types are generated from the Rust (`./generated/app.ts`, contract §2.6
 * as amended by #1325): a field is explained in its Rust doc comment and
 * nowhere else. The `invoke` functions stay hand-written, pinned to the Rust
 * by a test in that module which `include_str!`s this file.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AppStatus, RenderContent, RenderRun, UserRenderer, UserRendererDraft } from "./generated/app";

export type {
  AppStatus,
  CommandOutput,
  DbState,
  RenderContent,
  RendererInput,
  RenderRun,
  UserRenderer,
  UserRendererDraft,
} from "./generated/app";

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

/**
 * Record that the §14a first-run wizard finished.
 *
 * `AppStatus.first_run` is `source_count === 0 && !completed` — two facts,
 * because they are different ones. Without this, a person who finished the
 * wizard and later removed their only source would be shown it again.
 *
 * The flag lives in `knobas.setting`, not in the webview's storage: §14 makes
 * the database what an export carries, and a flag outside it would not survive
 * the round trip.
 *
 * Idempotent — finishing twice is the same as finishing once.
 */
export function completeFirstRun(): Promise<void> {
  return invoke<void>("complete_first_run");
}

/**
 * *Pages kept open* (#2247): how many pages the shell keeps mounted and hidden
 * besides the one in front, 0 to 15, 5 until set.
 */
export function pagesKeptOpen(): Promise<number> {
  return invoke<number>("pages_kept_open");
}

/**
 * Store *Pages kept open*, and get back what is now stored (held to 15), so
 * Settings › Performance draws what the database holds.
 */
export function setPagesKeptOpen(count: number): Promise<number> {
  return invoke<number>("set_pages_kept_open", { count });
}

/** The command renderers saved in Settings › Renderers (#2262), in order. */
export function userRenderers(): Promise<UserRenderer[]> {
  return invoke<UserRenderer[]>("user_renderers");
}

/** Save the whole list of command renderers; answers it as stored (ids, lowercase fences, held timeouts). */
export function setUserRenderers(renderers: UserRendererDraft[]): Promise<UserRenderer[]> {
  return invoke<UserRenderer[]>("set_user_renderers", { renderers });
}

/**
 * Run saved renderer `id` with `content` on its stdin (ADR-0033). A failure,
 * a timeout or too much output is an answer (`RenderRun`), not a rejection.
 */
export function runRenderer(id: string, content: RenderContent): Promise<RenderRun> {
  return invoke<RenderRun>("run_renderer", { id, content });
}
