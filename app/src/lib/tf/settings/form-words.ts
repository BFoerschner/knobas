/** What a settings form's disabled primary says it waits for (Controls, the enabled rule): *Add a source*'s words. */

/** *Still needs the name and the command.*, or `null` with nothing to ask for. */
export function needsWords(needs: readonly string[]): string | null {
  if (needs.length === 0) return null;
  return `Still needs ${needs.length > 1 ? `${needs.slice(0, -1).join(", ")} and ${needs.at(-1)}` : needs[0]}.`;
}

/** Why a timeout can't be saved: the backend would quietly clamp it, so it is said here. Empty is its default. */
export function secondsWhy(typed: string | number | null): string | null {
  const text = String(typed ?? "").trim();
  if (text === "") return null;
  const seconds = Number(text);
  return Number.isInteger(seconds) && seconds >= 1 && seconds <= 120 ? null : "The timeout takes 1 to 120 seconds.";
}
