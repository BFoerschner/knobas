/**
 * The bare window's look (ADR-0055, the decisions' 3.12): whenever the mode or a side's page colour changes, the
 * Rust side is told (`window_look_set`), keeps it in `window-look.json` and colours the window and its title bar, so
 * the next start shows the theme's page before the page exists. Told 300 ms after the last change, and only when
 * something changed. A run started with `knobas --default-theme` tells nothing: what was chosen draws again next time.
 * A browser with no backend ignores the failure.
 */
import { windowLookSet } from "../ipc/window";
import type { Theme as Mode } from "./theme.svelte";
import type { Themes } from "./themes.svelte";

export const TELL_AFTER = 300;

export function watchWindowLook(themes: Themes, mode: Mode, tell: typeof windowLookSet = windowLookSet, timers: { set: typeof setTimeout; clear: typeof clearTimeout } = { set: setTimeout, clear: clearTimeout }): () => void {
  if (themes.startedDefault) return () => {};
  let told = "";
  let waiting: ReturnType<typeof setTimeout> | null = null;
  return $effect.root(() => {
    $effect(() => {
      const look = { mode: mode.choice, light: themes.colours("light").paper, dark: themes.colours("dark").paper };
      const key = `${look.mode} ${look.light} ${look.dark}`;
      if (key === told) return;
      if (waiting !== null) timers.clear(waiting);
      waiting = timers.set(() => {
        waiting = null;
        told = key;
        tell(look.mode, look.light, look.dark).catch(() => {});
      }, TELL_AFTER);
    });
    return () => {
      if (waiting !== null) timers.clear(waiting);
    };
  });
}
