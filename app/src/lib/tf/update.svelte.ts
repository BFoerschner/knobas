/**
 * knobas updates itself (ADR-0052): the updater's state in the window, seeded once when the app mounts (before the
 * database is up, so the start screen's line and the cog's dot are right from their first frame) and moved by
 * `update:state`. Every press goes to the backend and comes back as its state; nothing is guessed here.
 *
 * After an update went in, one toast says so with *What's new*, which opens Settings › Updates.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import {
  updateCancel,
  updateCheck,
  updateDownload,
  updateInstall,
  updateLater,
  updateNotes,
  updateSet,
  updateState,
  type Channel,
  type UpdateNotes,
  type UpdateState,
} from "../ipc/update";
import { push } from "../shell/toasts.svelte";
import { updatedToast } from "./settings/updates";

export interface UpdatePorts {
  state: () => Promise<UpdateState>;
  check: () => Promise<UpdateState>;
  set: (check: boolean | null, channel: Channel | null) => Promise<UpdateState>;
  download: () => Promise<UpdateState>;
  cancel: () => Promise<UpdateState>;
  install: () => Promise<UpdateState>;
  later: () => Promise<UpdateState>;
  notes: (version: string) => Promise<UpdateNotes>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

const REAL_PORTS: UpdatePorts = {
  state: updateState,
  check: updateCheck,
  set: updateSet,
  download: updateDownload,
  cancel: updateCancel,
  install: updateInstall,
  later: updateLater,
  notes: updateNotes,
  listen: (event, handler) => tauriListen(event, handler),
};

export interface Updates {
  /** `null` until the first read answers. */
  readonly state: UpdateState | null;
  /** Read once and listen; answers the teardown. */
  start(): () => void;
  check(): Promise<void>;
  setCheck(on: boolean): Promise<void>;
  setChannel(channel: Channel): Promise<void>;
  download(): Promise<void>;
  cancel(): Promise<void>;
  install(): Promise<void>;
  later(): Promise<void>;
  notes(version: string): Promise<UpdateNotes>;
  /** Tests only. */
  reset(): void;
}

export function createUpdates(ports: UpdatePorts = REAL_PORTS, onUpdated: (state: UpdateState) => void = toastUpdated): Updates {
  let state = $state<UpdateState | null>(null);
  let toasted = false;

  const apply = (next: UpdateState | null | undefined) => {
    // A bridge that answers nothing (a test's, an older backend) leaves the state as it was.
    if (next === null || next === undefined || typeof next !== "object") return;
    state = next;
    if (next.just_updated && !toasted) {
      toasted = true;
      onUpdated(next);
    }
  };
  const run = async (call: () => Promise<UpdateState>) => {
    try {
      apply(await call());
    } catch (cause) {
      // A refused press (a phase that moved meanwhile) changes nothing: the next event says where it is.
      console.info("knobas: an update press was refused", cause);
    }
  };

  return {
    get state() {
      return state;
    },
    start() {
      let dead = false;
      let off: (() => void) | null = null;
      void ports.state().then((first) => {
        // An event that landed first is newer than the seed.
        if (!dead && state === null) apply(first);
      }, () => {});
      void ports
        .listen(EVENTS.updateState, ({ payload }) => apply(payload as UpdateState | null))
        .then((stop) => (dead ? stop() : (off = stop)))
        .catch(() => {});
      return () => {
        dead = true;
        off?.();
      };
    },
    check: () => run(ports.check),
    setCheck: (on) => run(() => ports.set(on, null)),
    setChannel: (channel) => run(() => ports.set(null, channel)),
    download: () => run(ports.download),
    cancel: () => run(ports.cancel),
    install: () => run(ports.install),
    later: () => run(ports.later),
    notes: (version) => ports.notes(version),
    reset() {
      state = null;
      toasted = false;
    },
  };
}

/**
 * The one toast after an update went in, once the shell is drawn: the state is read while the start screen still shows,
 * and a toast pushed then would run out its time before anyone could see it (the Mac's end-to-end run, 2026-11-21).
 */
function toastUpdated(state: UpdateState): void {
  let tries = 0;
  const shown = () => document.querySelector(".tf-nav") !== null;
  const wait = () => {
    if (shown() || tries++ > 480) toastNow(state);
    else setTimeout(wait, 250);
  };
  wait();
}

function toastNow(state: UpdateState): void {
  push({
    text: updatedToast(state),
    action: { label: "What's new", run: () => (location.hash = "#/settings/updates") },
    ms: 12_000,
  });
}

/** The window's updater state. */
export const updates = createUpdates();
