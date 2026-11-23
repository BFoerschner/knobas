/**
 * One bridge listener per event for the whole window, shared by every page that hears it (the ux30 end gate, Windows).
 * Each `listen` of Tauri's is a message to the backend, and so is each `unlisten`: a run opened from a repository's
 * Builds tab took the pipeline page's three listeners down and set the run page's three up for the same events, six
 * messages in the task that mounts the run, 14 ms of a sixth-speed main thread in WebView2. Here the first page to hear
 * an event sets its listener up; a page that stops hearing it leaves the set. The listener goes once no page hears the
 * event when the update's microtasks are done, so a page that leaves and one that comes in the same update share it and
 * send nothing, and nothing outlives the last page that heard it (`shell/residue.test.svelte.ts`).
 *
 * Each handler hears the events in the order they came, in the order the handlers joined. A handler that throws does not
 * keep the others from hearing it; its error is thrown again on its own.
 */
import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";

type Listen = (event: string, handler: EventCallback<unknown>) => Promise<UnlistenFn>;

/** One page's hearing: its own entry, so a handler that joins twice leaves once for each. */
interface Hearing {
  handler: EventCallback<unknown>;
}

interface Shared {
  handlers: Set<Hearing>;
  ready: Promise<UnlistenFn>;
}

export class SharedListeners {
  readonly #listen: Listen;
  readonly #events = new Map<string, Shared>();

  constructor(listen: Listen) {
    this.#listen = listen;
  }

  /** As Tauri's `listen`: resolves once the event is heard, to what stops `handler` hearing it. */
  async listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
    let shared = this.#events.get(event);
    if (shared === undefined) {
      const handlers = new Set<Hearing>();
      const ready = this.#listen(event, (heard) => {
        for (const each of [...handlers]) {
          try {
            each.handler(heard);
          } catch (error) {
            queueMicrotask(() => {
              throw error;
            });
          }
        }
      });
      shared = { handlers, ready };
      this.#events.set(event, shared);
      // A listener the bridge refused is asked for again by the next page that hears the event.
      ready.catch(() => {
        if (this.#events.get(event) === shared) this.#events.delete(event);
      });
    }
    const own: Hearing = { handler: handler as EventCallback<unknown> };
    shared.handlers.add(own);
    try {
      await shared.ready;
    } catch (error) {
      shared.handlers.delete(own);
      throw error;
    }
    return () => {
      if (!shared.handlers.delete(own) || shared.handlers.size > 0) return;
      queueMicrotask(() => {
        if (shared.handlers.size > 0 || this.#events.get(event) !== shared) return;
        this.#events.delete(event);
        shared.ready.then(
          (stop) => stop(),
          () => {},
        );
      });
    };
  }
}
