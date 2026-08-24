/** App lifecycle and status — `crates/knobas-app/src/commands/app.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** Liveness probe: resolves to `"pong"` once the backend is up. */
export function ping(): Promise<string> {
  return invoke<string>("ping");
}
