/**
 * Where the app opens (#881, spec #2227): the Job page, at its own bare
 * address `#/job` rather than at an empty one, so the address bar, `Back` and
 * the nav all agree from the first frame. A bare `#/job` draws the chosen job
 * (`job/chosen.svelte.ts`) and keeps its address; with none chosen it says how
 * a ticket becomes the job. An address the window was opened on is kept.
 */
export const HOME = "#/job";

/** Replace a blank address with `HOME`, leaving nothing behind it in history. */
export function homeIfBlank(place: { hash: string; replace: (url: string) => void }): void {
  if (place.hash === "" || place.hash === "#" || place.hash === "#/") place.replace(HOME);
}
