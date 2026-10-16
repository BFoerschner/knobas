/**
 * Which view of Dependencies is on, and what the reader has changed in it
 * since it was saved (#873), held for the session as module `$state`: a draft
 * is never written anywhere until *Save changes* or *Save as a new view…*
 * sends it, and a restart forgets it. It replaces #871's hidden-layer set.
 */
import type { DepView, DepViews, DepViewSettings } from "../../ipc/assets";
import { cloneSettings, sameSettings } from "./views";

export interface DepViewState {
  /** Every saved view and the two defaults, once read. */
  readonly saved: DepViews | null;
  /** The saved view in use, or `null` before the first read. */
  readonly current: DepView | null;
  /** What draws: the current view's settings with the reader's changes. */
  readonly draft: DepViewSettings | null;
  /** Whether the draft differs from what was saved. */
  readonly dirty: boolean;
  /** Take a read or a write's answer; opens the default view the first time. */
  take(views: DepViews): void;
  /** Open a saved view, dropping the draft. */
  open(id: string): void;
  /** Change the draft. */
  change(edit: (draft: DepViewSettings) => void): void;
  /** Take a saved view back: the draft becomes it, and it is on. */
  kept(view: DepView): void;
}

export function createDepViewState(): DepViewState {
  let saved = $state.raw<DepViews | null>(null);
  let currentId = $state<string | null>(null);
  let draft = $state.raw<DepViewSettings | null>(null);

  const current = () => saved?.views.find((view) => view.id === currentId) ?? null;

  function open(id: string) {
    const view = saved?.views.find((v) => v.id === id) ?? saved?.views[0] ?? null;
    currentId = view?.id ?? null;
    draft = view ? cloneSettings(view.settings) : null;
  }

  return {
    get saved() {
      return saved;
    },
    get current() {
      return current();
    },
    get draft() {
      return draft;
    },
    get dirty() {
      const view = current();
      return view !== null && draft !== null && !sameSettings(view.settings, draft);
    },
    take(views) {
      saved = views;
      if (currentId === null || !views.views.some((view) => view.id === currentId)) open(views.opens_from_top);
    },
    open,
    change(edit) {
      if (draft === null) return;
      const next = cloneSettings(draft);
      edit(next);
      draft = next;
    },
    kept(view) {
      if (saved !== null) {
        const at = saved.views.findIndex((v) => v.id === view.id);
        const views = at < 0 ? [...saved.views, view] : saved.views.map((v) => (v.id === view.id ? view : v));
        saved = { ...saved, views };
      }
      currentId = view.id;
      draft = cloneSettings(view.settings);
    },
  };
}

/** The window's one state. Tests build their own. */
export const depViewState = createDepViewState();
