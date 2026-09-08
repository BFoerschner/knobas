/**
 * The capture window's entry point (issue #503).
 *
 * A second document rather than a route in the shell, and that is the whole
 * design: `knobas_app::capture` opens `capture.html`, which mounts one
 * component. Nothing on this page subscribes to health, lists contexts, starts
 * a timer or polls the inbox — the shortcut's promise is that a thought costs
 * one keystroke, and a window that booted the application to take two lines
 * would spend a second of it.
 *
 * `app.css` is imported here because this document is not `index.html` and
 * shares nothing with it but the stylesheet.
 */
import { mount } from "svelte";

import CaptureWindow from "./lib/capture/CaptureWindow.svelte";
import "./app.css";

const target = document.getElementById("capture");
if (!target) {
  throw new Error("capture.html is missing the #capture mount point");
}

/**
 * The dev fixture, on the same terms as `App.svelte`'s: behind
 * `import.meta.env.DEV`, so Rollup folds the branch away and the fixture
 * reaches no bundle, and behind `?fake-ipc`, so even a dev build talks to the
 * real backend without it.
 *
 * Awaited before the mount and not after, for the reason `App.svelte` states:
 * a dynamic import resolves on a later tick, and this component reads
 * `capture_context` as it is created. A real Tauri window injects its internals
 * before any script runs, so getting this order wrong would break only in
 * browser QA — the worst place for a difference to hide.
 */
async function start() {
  if (import.meta.env.DEV) {
    const dev = await import("./lib/shell/dev/fake-tauri");
    dev.installIfRequested();
  }
  mount(CaptureWindow, { target: target! });
}

void start();
