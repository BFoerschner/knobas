/**
 * The diff view's code run once while the app is idle, before a page needs it (the forge-rounds pass, 2026-11-15).
 *
 * A pull request's *Files changed* first opened spent about half its page time at CPU 6x running code for the first
 * time in the app's life: the diff view, its cards and lines, the file list, the highlighter's worker. The module is in
 * the app's one bundle, so nothing is left to load early; what is left is the engine compiling and warming that code.
 * This draws a small made-up diff with it off screen, hidden and inert, after the window has drawn and gone idle, and
 * takes it down two frames later, so the first diff a reader opens runs warm code. It reads nothing, writes nothing,
 * takes no keys (its page says it is hidden) and leaves no node behind.
 */
import { flushSync, mount, unmount } from "svelte";
import type { DiffFile, DiffLine } from "../../ipc/entity";
import { pageContext } from "../kept-pages.svelte";
import TfDiffView from "./TfDiffView.svelte";

const context = (old: number, nu: number, text: string): DiffLine => ({ kind: "context", old, new: nu, text });
const removed = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });
const added = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });

/** Two files of one hunk each, every kind of line: the shapes a pull request's diff draws. */
export function warmFiles(): DiffFile[] {
  const code = (path: string, word: string): DiffFile => {
    const lines: DiffLine[] = [];
    for (let n = 1; n <= 24; n += 1) {
      if (n % 6 === 0) lines.push(removed(n, `  return ${word}(${n - 1});`), added(n, `  return ${word}(${n}, true);`));
      else lines.push(context(n, n, `  const ${word}${n} = ${word}(${n}); // ${n}`));
    }
    return { path, old_path: null, change: "modified", additions: 4, deletions: 4, hunks: [{ header: `@@ -1,24 +1,24 @@ ${word}`, lines }], truncated: false, threads: [] };
  };
  return [code("src/main/java/Warm.java", "warm"), code("internal/warm/warm.go", "ready")];
}

/** After the window has drawn and is idle; WebKit before 18.4 has no `requestIdleCallback`. Never under jsdom, which lays nothing out. */
function whenIdle(go: () => void): void {
  if (/jsdom/i.test(globalThis.navigator?.userAgent ?? "")) return;
  if (typeof requestIdleCallback === "function") requestIdleCallback(go, { timeout: 4000 });
  else setTimeout(go, 1500);
}

/** Two frames, so the view's first measures and its highlighter's first ask run too. */
function twoFrames(go: () => void): void {
  requestAnimationFrame(() => requestAnimationFrame(go));
}

let warmed = false;

/** Draws the diff view once off screen while the app is idle, then takes it down; once a session. */
export function warmDiffView(later: (go: () => void) => void = whenIdle, after: (go: () => void) => void = twoFrames): void {
  if (warmed || typeof document === "undefined") return;
  warmed = true;
  later(() => {
    const host = document.createElement("div");
    host.setAttribute("aria-hidden", "true");
    host.inert = true;
    host.className = "tf-warm";
    host.style.cssText = "position:fixed;left:-20000px;top:0;width:1200px;height:800px;overflow:hidden;visibility:hidden;pointer-events:none;contain:strict";
    document.body.appendChild(host);
    let view: ReturnType<typeof mount> | null = null;
    try {
      view = mount(TfDiffView, { target: host, props: { files: warmFiles() }, context: pageContext(() => false) });
      flushSync();
    } catch {
      // A warm-up that fails costs the warmth, never the app.
    }
    after(() => {
      try {
        if (view !== null) void unmount(view);
      } finally {
        host.remove();
      }
    });
  });
}

/** Tests only: a new session. */
export function forgetWarmth(): void {
  warmed = false;
}
