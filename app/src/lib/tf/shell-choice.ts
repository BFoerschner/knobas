/**
 * Which shell the window draws (#809, spec #808 story 11): today's rooms UI,
 * or the ticket-first shell that replaces it.
 *
 * **Read once, at startup.** The spec says the setting picks the shell at
 * startup, and the two shells own the same addresses (`#/inbox`, `#/time`,
 * `#/settings`) with different meanings, so switching reloads the window
 * rather than swapping roots under a live router.
 *
 * **In the webview's storage, not in `knobas.setting`.** The switch exists
 * only until TF5 removes the rooms UI, it has to be known before the database
 * is up (the boot screen is drawn by whichever shell is chosen), and it is not
 * something a share export should carry to a colleague. A missing or broken
 * storage reads as the rooms UI, which is what nothing chosen means.
 *
 * `?shell=tickets` overrides it in a dev build only, so a `?fake-ipc`
 * screenshot can photograph the new shell from a clean browser profile.
 */
export type ShellChoice = "rooms" | "tickets";

export const SHELL_KEY = "knobas.shell";

type Store = Pick<Storage, "getItem" | "setItem">;

/** This window's storage, or `null` where there is none. */
export function windowStorage(): Store | null {
  try {
    return typeof window === "undefined" ? null : (window.localStorage ?? null);
  } catch {
    return null;
  }
}

export function readShellChoice(
  storage: Store | null,
  devSearch: string | null = null,
): ShellChoice {
  const override = devSearch === null ? null : new URLSearchParams(devSearch).get("shell");
  if (override === "tickets" || override === "rooms") return override;
  try {
    return storage?.getItem(SHELL_KEY) === "tickets" ? "tickets" : "rooms";
  } catch {
    return "rooms";
  }
}

/** Store the choice and reload, which is how it takes effect. */
export function switchShell(
  choice: ShellChoice,
  storage: Store | null = windowStorage(),
  reload: () => void = () => location.reload(),
): void {
  storage?.setItem(SHELL_KEY, choice);
  reload();
}
