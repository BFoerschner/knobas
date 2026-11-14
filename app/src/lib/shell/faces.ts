/**
 * Every font face the sheet declares is loaded once the window is up, not when a page first draws a word in it
 * (the render-cost pass, 2026-11-14).
 *
 * A browser loads a face only when text needs it: JetBrains Mono 400 the first time a page draws a run's number or
 * a key, Source Serif the first time Docs draws a page. The face arrives a task after that page drew, and Chromium
 * then lays out every text of the page again ("Fonts changed"): on Builds' first open at 4x, a second layout of 1,391
 * of its 1,396 objects, 17 to 20 ms, in the frames that should be showing it. Loading every face while the app is
 * idle after its first draw pays that once, on the page the app opened on, before anything is pressed.
 *
 * The faces are the app's own files (`tf.css` imports them from npm), so loading the ones no page uses yet costs a
 * few local reads and no network.
 */

/** The page's font set, as much of it as this reads: jsdom has none. */
type Faces = { forEach(each: (face: { status: string; load(): Promise<unknown> }) => void): void };

/** Loads every face of `fonts` not yet loaded or loading, after `later` (the window idle). */
export function loadEveryFace(fonts: Faces | undefined = globalThis.document?.fonts, later: (go: () => void) => void = whenIdle): void {
  if (!fonts) return;
  later(() =>
    fonts.forEach((face) => {
      if (face.status === "unloaded") face.load().catch(() => undefined);
    }),
  );
}

/** After the window has drawn and is idle; WebKit before 18.4 has no `requestIdleCallback`. */
function whenIdle(go: () => void): void {
  if (typeof requestIdleCallback === "function") requestIdleCallback(go, { timeout: 2000 });
  else setTimeout(go, 500);
}
