/**
 * The new shell's colour theme (R-1.7, R-13a.8): one setting, which the top
 * bar's toggle and Settings › Themes both write (#831).
 *
 * `system` follows the OS through `prefers-color-scheme`; `light` and `dark`
 * override it. Kept since 2026-11-21 (ADR-0051, amending R-1.7's "for the
 * session"): a chosen theme that forgot its side on a restart read as a bug.
 * `knobas.theme.mode` in the webview's `localStorage`, absent for `system`,
 * read and written on `<html>` as `data-theme` as soon as this module loads,
 * so the first frame draws the kept side.
 */
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";

export type ThemeChoice = "system" | "light" | "dark";

export const MODE_KEY = "knobas.theme.mode";

export interface Theme {
  readonly choice: ThemeChoice;
  /** Settings › Themes: pick one. */
  set(choice: ThemeChoice): void;
  /**
   * The top bar's toggle: the opposite of what is drawn now, which under
   * `system` is what the OS says.
   */
  toggle(prefersDark: boolean): void;
  /** Tests only: back to following the system, nothing kept (#1743). */
  reset(): void;
}

/** Where the side is written: `<html>`. */
export type ModeRoot = Pick<Element, "setAttribute" | "removeAttribute">;

export function createTheme(storage: () => KeptStorage | null = () => null, root: () => ModeRoot | null = () => null): Theme {
  const kept = keptValue(storage, MODE_KEY);
  const read = kept.read();
  let choice = $state<ThemeChoice>(read === "light" || read === "dark" ? read : "system");
  const apply = () => {
    const html = root();
    if (!html) return;
    if (choice === "system") html.removeAttribute("data-theme");
    else html.setAttribute("data-theme", choice);
  };
  const pick = (next: ThemeChoice) => {
    choice = next;
    if (next === "system") kept.clear();
    else kept.write(next);
    apply();
  };
  apply();
  return {
    get choice() {
      return choice;
    },
    set: pick,
    reset: () => pick("system"),
    toggle(prefersDark) {
      const dark = choice === "system" ? prefersDark : choice === "dark";
      pick(dark ? "light" : "dark");
    },
  };
}

/** The one the window draws. */
export const theme = createTheme(windowStorage, () => (typeof document === "undefined" ? null : document.documentElement));
