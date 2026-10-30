/**
 * A read the shell makes once the database can answer (*Pages kept open*,
 * the command renderers, the renderer defaults, the pipelines): call it while
 * a component is set up. It waits for `ready`, reads `port` once (the port as
 * it was then, untracked), and hands the answer to `hold`. A refused read is
 * swallowed: whatever was held, the default first, stands. No port (a test
 * that wires none) reads nothing.
 */
import { untrack } from "svelte";

export function onReady<T>(ready: () => boolean, port: () => (() => Promise<T>) | undefined, hold: (value: T) => void): void {
  $effect(() => {
    if (!ready()) return;
    const read = untrack(port);
    if (read === undefined) return;
    void read()
      .then(hold)
      .catch(() => {});
  });
}
