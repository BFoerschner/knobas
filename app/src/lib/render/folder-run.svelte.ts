/**
 * A folder render's ask and listen loop (ADR-0044, ADR-0046), lifted out of `TfFolderRender` so the Code page's
 * generated folders run through the same code.
 *
 * It asks the backend (`folder_render`) and is answered at once: what is kept for the folder as it is now, else the
 * last drawing of it, and where the run is. It asks with `open` only while it is shown (`start`): a kept or hidden page
 * draws what is kept and starts nothing. Each `folder-render:moved` for this folder asks again with `look`, its
 * progress taken without an ask. `press` is *Run again* or *Stop*. Leaving drops a run still waiting for a place
 * (`folder_render_leave`).
 *
 * Built inside a component's script: its effects are the component's. `at` answering `null` asks nothing.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";
import { untrack } from "svelte";

import { EVENTS, ipcErrorMessage } from "../ipc";
import {
  folderRender,
  folderRenderFrame,
  folderRenderLeave,
  folderRenderOutput,
  folderRenderStop,
  type FolderAsk,
  type FolderRenderMoved,
  type FolderRenderView,
} from "../ipc/app";
import { PressHold } from "../tf/press-hold.svelte";

/** What a folder render reaches the backend through: the IPC, a fake in tests. */
export interface FolderRenderPorts {
  render(at: FolderAsk): Promise<FolderRenderView>;
  stop(entityId: string, renderer: string, folder: string, lane?: string): Promise<void>;
  leave(entityId: string, renderer: string, folder: string, lane?: string): Promise<void>;
  output(token: string): Promise<Uint8Array>;
  frame(document: string): Promise<string>;
  listen(event: string, handler: (event: { payload: unknown }) => void): Promise<() => void>;
}

export const ipcFolderRenderPorts: FolderRenderPorts = {
  render: folderRender,
  stop: folderRenderStop,
  leave: folderRenderLeave,
  output: folderRenderOutput,
  frame: folderRenderFrame,
  listen: (event, handler) => tauriListen(event, handler),
};

/** Where a run is asked for. */
export interface FolderRunAt {
  entityId: string;
  renderer: string;
  /** The renderer's command: a new one asks again (Settings' *Try it*). */
  command: string;
  folder: string;
  commit: string | null;
  paths: string | null;
  /** Whether a run may start: the page is shown and a person opened it. */
  start: boolean;
  /** The run's lane (ux26): absent for the Files tab; a side of a comparison. */
  lane?: string;
  /**
   * What moves when the folder's state moves without its commit (ux29: git's list's stamp on the working tree, whose
   * commit is always `~working-tree`), so the render asks again; absent at a commit.
   */
  stamp?: string;
}

export class FolderRun {
  view = $state<FolderRenderView | null>(null);
  refused = $state<string | null>(null);
  /** Moves every second while a tool runs, so its time counts up. */
  #tick = $state(0);
  #askedAt = 0;
  #hold = new PressHold();
  readonly #at: () => FolderRunAt | null;
  readonly #ports: FolderRenderPorts;

  /** The run's time so far, in ms, while the tool runs; 0 otherwise. */
  readonly elapsed = $derived.by(() => {
    const running = this.view?.running ?? null;
    return running?.state === "running" ? running.elapsed_ms + this.#tick * 1000 : 0;
  });

  constructor(at: () => FolderRunAt | null, ports: FolderRenderPorts) {
    this.#at = at;
    this.#ports = ports;

    // The folder, the renderer or the commit changed, or the working tree on disk: ask again, starting a run where the
    // page is shown.
    let place: string | null = null;
    $effect(() => {
      const now = this.#at();
      if (now === null) return;
      const asked = JSON.stringify([now.entityId, now.renderer, now.command, now.folder, now.commit, now.paths, now.lane ?? ""]);
      void now.stamp;
      const kind = now.start ? "open" : "look";
      untrack(() => {
        // A new folder or commit starts blank; a new state of the same one keeps its drawing until the answer lands.
        if (asked !== place) this.view = null;
        place = asked;
        this.#answer(kind);
      });
    });

    // A page shown again after it was kept hidden opens the render: a run starts where nothing is kept.
    // Read first in the effect, not here: the component's own state may not be declared yet.
    let wasShown: boolean | null = null;
    $effect(() => {
      const now = this.#at()?.start ?? false;
      if (now && wasShown === false) untrack(() => this.#answer("open"));
      wasShown = now;
    });

    // The run moved or ended: ask again. Keyed by the slot alone, so a new state of the folder keeps the listener.
    const slotKey = $derived.by(() => {
      const now = this.#at();
      return now === null ? null : JSON.stringify([now.entityId, now.renderer, now.folder, now.lane ?? ""]);
    });
    $effect(() => {
      if (slotKey === null) return;
      const [entityId, renderer, folder, lane] = JSON.parse(slotKey) as [string, string, string, string];
      const slot = { entityId, renderer, folder, lane };
      let live = true;
      let off: (() => void) | null = null;
      void this.#ports
        .listen(EVENTS.folderRender, ({ payload }) => {
          const moved = payload as FolderRenderMoved;
          if (moved.entity_id !== slot.entityId || moved.renderer !== slot.renderer || moved.folder !== slot.folder || moved.lane !== slot.lane) return;
          if (moved.running !== null && this.view !== null) {
            // Progress: the state alone, no ask.
            this.view = { ...this.view, running: moved.running };
            return;
          }
          untrack(() => this.#answer("look"));
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        });
      return () => {
        live = false;
        off?.();
        void this.#ports.leave(slot.entityId, slot.renderer, slot.folder, slot.lane || undefined).catch(() => {});
      };
    });

    // The run's clock.
    $effect(() => {
      if (this.view?.running?.state !== "running") return;
      const every = setInterval(() => (this.#tick += 1), 1000);
      return () => clearInterval(every);
    });
  }

  #answer(kind: FolderAsk["ask"]) {
    const now = this.#at();
    if (now === null) return;
    const asked = ++this.#askedAt;
    this.#ports.render({ entityId: now.entityId, renderer: now.renderer, folder: now.folder, commit: now.commit, paths: now.paths, ask: kind, ...(now.lane ? { lane: now.lane } : {}) }).then(
      (next) => {
        if (asked !== this.#askedAt) return;
        this.view = next;
        this.refused = null;
      },
      (cause: unknown) => {
        if (asked === this.#askedAt) this.refused = ipcErrorMessage(cause);
      },
    );
  }

  /** Ask again with `look`: a token the backend no longer held is read back from the store (ADR-0046). */
  look(): void {
    this.#answer("look");
  }

  /** *Run again* and *Stop* are one button (focus stays on it), held after a press so a double press acts once. */
  press(): void {
    if (this.#hold.held()) return;
    this.#hold.hold();
    const now = this.#at();
    if (now === null) return;
    // The press answers in its own frame; the backend's answer (and its events) replace this.
    if (this.view?.running) {
      this.view = { ...this.view, running: null };
      void this.#ports.stop(now.entityId, now.renderer, now.folder, now.lane || undefined);
    } else {
      if (this.view !== null) this.view = { ...this.view, running: { state: "queued" }, failure: null };
      this.#answer("again");
    }
  }
}
