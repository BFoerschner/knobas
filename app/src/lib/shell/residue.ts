/**
 * What a component left behind after it was unmounted.
 *
 * ## The family of bugs this exists for
 *
 * A component installs something on a shared object — a `window` listener, an
 * interval, a coalescing timer, a Tauri event subscription — and returns a
 * teardown from its `$effect` that removes it again. **Every test in this
 * codebase watches what the component draws, and none of them watches what it
 * removes.** So the teardown can be deleted, or gutted to `() => {}`, and the
 * suite stays green: three such holes were found in one review of this stream,
 * in two components, and they are the same hole the `generate_handler!`
 * survivor was — *nothing pins that the thing which must be called is called* —
 * one layer up.
 *
 * Asserting "the teardown removes the listener" three times, once per
 * component, would leave the fourth component to be caught by the next
 * reviewer. This measures the **consequence** instead: mount it, use it,
 * unmount it, and see whether anything of it is still attached. That holds for
 * a component nobody has written yet, and it holds however the teardown is
 * spelled.
 *
 * ## What is watched, and what deliberately is not
 *
 * **`window` listeners and the four timer functions.** Not element-local
 * listeners: Svelte attaches and removes those itself, and a listener on a
 * node being discarded with the node is not a leak.
 *
 * Not `document` either, and that one is a real limitation rather than a
 * simplification: Svelte 5 delegates `onclick`/`onkeydown` by adding **one**
 * `document` listener per event type, the first time any component needs it,
 * and never removing it. That is correct of Svelte and indistinguishable here
 * from a component that added its own — measured, not assumed: mounting a
 * component with an `onclick` leaves `click on document` and `keydown on
 * document` behind for ever. Watching `document` would therefore either fail
 * on the framework or need a copy of Svelte's delegated-event list, which is
 * an internal that will drift.
 *
 * That costs nothing today because this codebase puts global handlers on
 * `window` on purpose — `keys.ts` and `Modal.svelte` both record why (a
 * propagation stopped anywhere below `window` never reaches the global
 * handler). A component that installs a `document` listener of its own is
 * outside this net, and should not be doing that.
 *
 * Timers are counted as *outstanding*: a `setTimeout` that has already fired
 * is not residue, an interval is residue until it is cleared, and a
 * `setTimeout` still armed when its component is gone is exactly the case that
 * writes into a store nothing is watching any more.
 *
 * This module is imported only from tests. Nothing in the app graph reaches
 * it, so Rollup drops it — the same arrangement as the `*.fixture.svelte`
 * files.
 */

/** What is still attached after a component is gone. Empty is the only pass. */
export interface Residue {
  /** e.g. `"pointerdown (capture) on window"`, once per unbalanced add. */
  listeners: string[];
  /** e.g. `"setInterval"`, once per timer still armed. */
  timers: string[];
}

export interface ResidueTracker {
  /**
   * Let everything in flight land: promise chains, and any timer already due.
   *
   * Uses the *real* `setTimeout` captured before patching, so waiting is not
   * itself counted as residue.
   */
  settle(): Promise<void>;
  /** What is still attached, right now. */
  residue(): Residue;
  /** Restore the globals. Always call it, or the next test inherits the patch. */
  stop(): void;
}

/** Whether a listener was registered in the capture phase. */
function captures(options?: boolean | EventListenerOptions): boolean {
  return typeof options === "boolean" ? options : (options?.capture ?? false);
}

/**
 * Start watching. Call [`ResidueTracker.stop`] in a `finally` or an
 * `afterEach`: the globals stay patched until it runs.
 */
export function trackResidue(): ResidueTracker {
  const realSetTimeout = globalThis.setTimeout;
  const realClearTimeout = globalThis.clearTimeout;
  const realSetInterval = globalThis.setInterval;
  const realClearInterval = globalThis.clearInterval;

  /** Unbalanced `addEventListener`s, keyed by what a reader needs to see. */
  const listeners = new Map<string, number>();
  /** Timers armed and not yet fired or cleared. */
  const timers = new Map<unknown, string>();

  const targets: { name: string; target: EventTarget }[] = [{ name: "window", target: window }];
  const originals = new Map<
    EventTarget,
    { add: EventTarget["addEventListener"]; remove: EventTarget["removeEventListener"] }
  >();

  for (const { name, target } of targets) {
    const add = target.addEventListener.bind(target);
    const remove = target.removeEventListener.bind(target);
    originals.set(target, { add, remove });

    target.addEventListener = ((
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | AddEventListenerOptions,
    ) => {
      const key = `${type}${captures(options) ? " (capture)" : ""} on ${name}`;
      listeners.set(key, (listeners.get(key) ?? 0) + 1);
      add(type, listener, options);
    }) as EventTarget["addEventListener"];

    target.removeEventListener = ((
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | EventListenerOptions,
    ) => {
      const key = `${type}${captures(options) ? " (capture)" : ""} on ${name}`;
      const live = listeners.get(key) ?? 0;
      // Never below zero: a `removeEventListener` for something that was never
      // added would otherwise cancel out a real leak elsewhere.
      if (live > 0) listeners.set(key, live - 1);
      remove(type, listener, options);
    }) as EventTarget["removeEventListener"];
  }

  globalThis.setTimeout = ((handler: TimerHandler, ms?: number, ...args: unknown[]) => {
    // The handler is wrapped so a timer that *fires* stops being outstanding.
    // A fired timeout is not a leak; one still armed when its owner is gone is.
    const id: ReturnType<typeof setTimeout> = realSetTimeout(
      (...fired: unknown[]) => {
        timers.delete(id);
        if (typeof handler === "function") {
          (handler as (...a: unknown[]) => void)(...fired);
        }
      },
      ms,
      ...args,
    );
    timers.set(id, "setTimeout");
    return id;
    // Through `unknown`: `@types/node`'s `setTimeout` carries a `__promisify__`
    // property that a plain function cannot have, and reproducing it would be
    // ceremony around a wrapper that only counts.
  }) as unknown as typeof globalThis.setTimeout;

  globalThis.setInterval = ((handler: TimerHandler, ms?: number, ...args: unknown[]) => {
    const id = realSetInterval(handler, ms, ...args);
    timers.set(id, "setInterval");
    return id;
  }) as typeof globalThis.setInterval;

  globalThis.clearTimeout = ((id?: Parameters<typeof clearTimeout>[0]) => {
    timers.delete(id);
    realClearTimeout(id);
  }) as typeof globalThis.clearTimeout;

  globalThis.clearInterval = ((id?: Parameters<typeof clearInterval>[0]) => {
    timers.delete(id);
    realClearInterval(id);
  }) as typeof globalThis.clearInterval;

  return {
    async settle() {
      // Three macrotask hops. Each one drains the microtask queue with it, so
      // this covers a `listen().then().then()` chain and the `.catch` after it.
      for (let round = 0; round < 3; round += 1) {
        await new Promise<void>((resolve) => {
          realSetTimeout(resolve, 0);
        });
      }
    },

    residue() {
      const stuck: string[] = [];
      for (const [key, count] of listeners) {
        for (let n = 0; n < count; n += 1) stuck.push(key);
      }
      return { listeners: stuck.sort(), timers: [...timers.values()].sort() };
    },

    stop() {
      for (const { target } of targets) {
        const original = originals.get(target);
        if (!original) continue;
        target.addEventListener = original.add;
        target.removeEventListener = original.remove;
      }
      globalThis.setTimeout = realSetTimeout;
      globalThis.clearTimeout = realClearTimeout;
      globalThis.setInterval = realSetInterval;
      globalThis.clearInterval = realClearInterval;
    },
  };
}
