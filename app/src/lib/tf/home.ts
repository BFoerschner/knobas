/**
 * Where the app opens (#881): Tickets, at its own address rather than at an
 * empty one, so the address bar, `Back` and the nav all agree from the first
 * frame. An address the window was opened on is kept.
 */
export const HOME = "#/tickets";

/** Replace a blank address with `HOME`, leaving nothing behind it in history. */
export function homeIfBlank(place: { hash: string; replace: (url: string) => void }): void {
  if (place.hash === "" || place.hash === "#" || place.hash === "#/") place.replace(HOME);
}
