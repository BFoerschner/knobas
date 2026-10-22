/**
 * The new shell's colour theme (R-1.7, R-13a.8): one setting, which the top
 * bar's toggle and Settings › Appearance both write (#831).
 *
 * `system` follows the OS through `prefers-color-scheme`; `light` and `dark`
 * override it for the session, as prototype-6's `S.stg.theme` does. Nothing
 * is stored: R-1.7 says "for the session".
 */
export type ThemeChoice = "system" | "light" | "dark";

export interface Theme {
  readonly choice: ThemeChoice;
  /** Settings › Appearance: pick one. */
  set(choice: ThemeChoice): void;
  /**
   * The top bar's toggle: the opposite of what is drawn now, which under
   * `system` is what the OS says.
   */
  toggle(prefersDark: boolean): void;
  /** Tests only: back to following the system (#1743). */
  reset(): void;
}

export function createTheme(): Theme {
  let choice = $state<ThemeChoice>("system");
  return {
    get choice() {
      return choice;
    },
    set(next) {
      choice = next;
    },
    reset() {
      choice = "system";
    },
    toggle(prefersDark) {
      const dark = choice === "system" ? prefersDark : choice === "dark";
      choice = dark ? "light" : "dark";
    },
  };
}

/** The one the window draws. */
export const theme = createTheme();
