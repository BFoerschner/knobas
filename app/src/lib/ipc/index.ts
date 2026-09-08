/**
 * The M1 IPC surface: one module per Rust command module
 * (`crates/knobas-app/src/commands/`), re-exported here so callers import from
 * one place.
 *
 * These mirrors are written by hand on purpose. `tauri-specta`, which would
 * generate them, is still `2.0.0-rc.*`. What is declared here is exactly the
 * serde output of the Rust types: `DateTime<Utc>` → `string` (RFC 3339),
 * `Option<T>` → `T | null`, `i64`/`u64`/`f32` → `number`,
 * `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields,
 * snake_case enums → string unions, tagged enums → discriminated unions.
 *
 * Argument names are camelCase because Tauri renames command arguments that
 * way; `sourceId` here is `source_id` in Rust. *Struct fields* are not
 * renamed — they arrive in their Rust snake_case spelling.
 *
 * This file is orchestrator-owned and append-only.
 */
export * from "./app";
export * from "./backup";
export * from "./entity";
export * from "./search";
export * from "./sources";
export * from "./time";
export * from "./assets";

/**
 * Tauri event names — the mirror of `knobas_app::events`, pinned by a Rust
 * test that reads this file.
 */
export const EVENTS = {
  /** `DbState` during bring-up; replayed by `frontendReady()`. */
  dbState: "db:state",
  /** `SourceSyncStatus` on every sync-run transition. */
  syncState: "sync:state",
  /** `CredentialHealth`, on a health change only. */
  sourceHealth: "source:health",
  /** `ActivityRow`, coalesced to at most one per second. */
  activityNew: "activity:new",
  /** `ContextRow` — a context was created or promoted (#47). */
  contextsChanged: "contexts:changed",
  /** `NotificationClicked` — a desktop notification's body was clicked (#339). */
  notificationClicked: "notification:clicked",
  /**
   * The note's entity id — the capture window asked for its note to be opened
   * in the main window (#503). Sent to the `main` window by label, because the
   * capture window is closing and has no use for it.
   */
  captureOpenNote: "capture:open-note",
} as const;

/** Why a command failed — `knobas_app::IpcErrorCode`. */
export type IpcErrorCode =
  | "unauthorized"
  | "unreachable"
  | "not_found"
  | "conflict"
  | "invalid"
  | "not_ready"
  | "internal";

/** What every command rejects with — `knobas_app::IpcError`. */
export interface IpcError {
  code: IpcErrorCode;
  message: string;
  /** The source this failure belongs to, when it belongs to one. */
  source_id: string | null;
}

/**
 * Whether a rejection is a knobas command error. `invoke` rejects with
 * whatever the Rust `Err` serialized to, and `catch` binds it as `unknown`.
 */
export function isIpcError(error: unknown): error is IpcError {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  const candidate = error as Partial<IpcError>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

/** Whatever arrived, as something displayable. */
export function ipcErrorMessage(error: unknown): string {
  if (isIpcError(error)) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  // `JSON.stringify` returns `undefined` — not the string `"undefined"` — for
  // an undefined, a function or a symbol, which would break the return type
  // this function promises.
  return JSON.stringify(error) ?? String(error);
}
