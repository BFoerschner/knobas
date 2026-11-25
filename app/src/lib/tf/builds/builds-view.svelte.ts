/**
 * Which Builds view a bare `#/builds` opens (2026-11-25, Fable's ruling 2): *Recent* once the reader pressed it, kept
 * across restarts in the webview's `localStorage` as the diff layout is, until they press *Pipelines*. Only a press on
 * the switch writes it: a link to a pipeline opens that pipeline and changes nothing.
 */
import { keptValue, windowStorage, type KeptStorage } from "../kept-value";

export type BuildsView = "pipelines" | "recent";

const KEY = "knobas.builds.view";

class KeptView {
  #store;
  #value = $state<BuildsView>("pipelines");

  constructor(storage: () => KeptStorage | null) {
    this.#store = keptValue(storage, KEY);
    this.reload();
  }

  get value(): BuildsView {
    return this.#value;
  }

  /** The reader's press on the switch. */
  choose(view: BuildsView): void {
    this.#value = view;
    if (view === "recent") this.#store.write(view);
    else this.#store.clear();
  }

  /** Read what is kept again: a new session, which is what each test is. */
  reload(): void {
    this.#value = this.#store.read() === "recent" ? "recent" : "pipelines";
  }
}

export const buildsView = new KeptView(windowStorage);
