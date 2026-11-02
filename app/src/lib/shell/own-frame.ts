/**
 * The app never runs inside a frame of its own origin (#2461, spec #2432
 * story 5).
 *
 * A docs URL embed's frame keeps its page's origin (`allow-scripts
 * allow-same-origin`, `render/embed.ts`). `embedUrl` refuses an address that
 * could be the app's origin, but the framed page can still navigate itself,
 * or a frame of its own, to it, or be redirected there. That frame would load
 * the app's own document at the app's origin: sharing the app's storage, able
 * to lift its own sandbox, its address picked by the page. The app's policy
 * says `frame-ancestors 'none'` (`tauri.conf.json`), which the bundled app
 * serves as a header; the dev server sends none, and this check holds in
 * either. Every page the app builds (`index.html`, `capture.html`,
 * `d2-frame.html`) imports `own-frame-guard.ts` first, whose throw stops the
 * rest of its modules before they run.
 *
 * Only an ancestor of the same origin counts. A cross-origin ancestor (a
 * browser tool that frames the dev server) cannot be read, and D2's frame,
 * an opaque origin under the app, cannot read its parent either.
 */
export function framedByOwnOrigin(w: Window = window): boolean {
  const own = w.location.origin;
  for (let at: Window = w; at.parent !== at; at = at.parent) {
    try {
      if (at.parent.location.origin === own) return true;
    } catch {
      // Another origin's window: the browser keeps it unreadable.
    }
  }
  return false;
}

/** Throws when {@link framedByOwnOrigin}: the page then runs nothing else. */
export function refuseOwnFrame(w: Window = window): void {
  if (framedByOwnOrigin(w)) throw new Error("knobas does not run inside a frame of its own origin (#2461).");
}
