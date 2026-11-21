/**
 * The window's own look (ADR-0055) — `crates/knobas-app/src/commands/window.rs`. Pinned to the Rust by a test in
 * that module which `include_str!`s this file.
 */
import { invoke } from "./invoke";

/** The window's mode and each side's page colour (`#RRGGBB`), kept for the bare window before the page paints. */
export function windowLookSet(mode: "system" | "light" | "dark", light: string, dark: string): Promise<void> {
  return invoke<void>("window_look_set", { mode, light, dark });
}
