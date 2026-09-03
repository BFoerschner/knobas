/**
 * The ⌘K launcher — stream E's frontend half.
 *
 * A shell mounts `<Launcher>`, binds `open`, and answers `onnavigate`.
 * Everything else — the hotkey, the debounce, the `Esc` ladder, the grammar
 * chips, the board — is the component's.
 */
export { default as Launcher } from "./Launcher.svelte";
export { type LauncherAction, navigationActions } from "./actions";
export { ancestorPath } from "./ancestors";
export { provenance, sourceMonogram, syncAge } from "./format";
export { type LauncherMode, type LauncherRow } from "./rows";
export { DEBOUNCE_MS, SEARCH_LIMIT } from "./session.svelte";
export { SYNTAX, type SyntaxEntry } from "./syntax";

/**
 * The shortcut the launcher binds, for a shell that wants to *show* it.
 *
 * `Mod` is ⌘ on macOS and Ctrl elsewhere, which is what the handler checks
 * (`metaKey || ctrlKey`). Declared here so a keyboard-help surface and the
 * top strip's hint cannot drift from what is actually bound.
 */
export const LAUNCHER_HOTKEY = "Mod+K";
