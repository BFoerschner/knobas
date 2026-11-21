/**
 * `?fake-ipc`: knobas updates itself (ADR-0052). The eight `update_*` commands over one state that
 * `?fake-update=<state>` picks (`up-to-date` when absent):
 *
 * `up-to-date`, `checks-off`, `available`, `downloading`, `ready`, `installing`, `signature`, `download-failed`,
 * `not-installed`, `check-failed`, `package`, `pre-on-releases`, `development`, `not-set-up`, `checking`.
 *
 * `?fake-update-then=<state>&fake-update-after=<ms>` lands a second state as `update:state` that long after the first read,
 * for `just moves` (an update found, a download ending, a check failing while the page is drawn). *Download* runs a fake
 * download of two seconds; *Restart to update* walks the install's steps and stays on *Closing*.
 */
import type { UpdateNotes, UpdateState } from "../../../ipc/update";
import { updateSample as fakeUpdate } from "../../../tf/settings/update-samples";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { onFakeReset } from "../fake-seed";

const MB = 1_000_000;

const NOTES = `A pre-release of the UX polish branch. It replaces 3.2.9-ux27.

### New since 3.2.9-ux27

- **knobas updates itself.** Settings › Updates shows a newer version, downloads it and restarts into it. Pre-release builds are a choice there.
- **A comparison drawn rendered** keeps its place when you switch between Unified, Split and Rendered.
- **The branch box** lists the working tree first where a repository is cloned on this computer.

### Fixed

- A right-click menu no longer stays open after a fast series of right-clicks.
- Settings › Renderers' *Add a default* row stays on the table's columns.
- A Builds header keeps its buttons on the title's line.`;

let bridge: FakeBridge | null = null;
let state: UpdateState = fakeUpdate(null);

onFakeReset(() => {
  bridge = null;
  state = fakeUpdate(null);
});

/** The installed bridge, which the fake download and install emit through. */
export function attach(installed: FakeBridge): void {
  bridge = installed;
}

function put(next: UpdateState): UpdateState {
  state = next;
  bridge?.emit("update:state", next);
  return next;
}

/** Read `?fake-update=` before the bridge is installed, and schedule `fake-update-then`. */
export function seed(params: URLSearchParams): void {
  state = fakeUpdate(params.get("fake-update"));
  const then = params.get("fake-update-then");
  if (then !== null) {
    const after = Number(params.get("fake-update-after") ?? 1000);
    setTimeout(() => put(fakeUpdate(then)), Number.isFinite(after) ? after : 1000);
  }
}

export function handlers(): Handlers {
  return {
    update_state: () => state,
    update_check: () => put({ ...state, checked_at: new Date().toISOString(), check_failed: null, phase: { phase: "idle" } }),
    update_set: (args) => {
      const check = args["check"] as boolean | null;
      const channel = args["channel"] as UpdateState["channel"] | null;
      return put({ ...state, check: check ?? state.check, channel: channel ?? state.channel });
    },
    update_download: () => {
      const total = state.offer?.bytes ?? 89 * MB;
      let received = 0;
      const tick = setInterval(() => {
        received = Math.min(total, received + total / 8);
        if (received >= total) {
          clearInterval(tick);
          put({ ...state, phase: { phase: "ready" } });
        } else put({ ...state, phase: { phase: "downloading", received, total } });
      }, 250);
      return put({ ...state, phase: { phase: "downloading", received: 0, total } });
    },
    update_cancel: () => put({ ...state, phase: { phase: "idle" } }),
    update_install: () => {
      const steps = ["checking", "backing_up", "closing"] as const;
      steps.forEach((step, at) => setTimeout(() => put({ ...state, phase: { phase: "installing", step } }), 400 * (at + 1)));
      return put({ ...state, phase: { phase: "installing", step: "checking" } });
    },
    update_later: () => put({ ...state, marked: false }),
    update_notes: (args): UpdateNotes => {
      const version = String(args["version"] ?? "");
      return { version, markdown: NOTES.replaceAll("3.2.9-ux28", version), page_url: `https://releases.example/knobas/v${version}` };
    },
  };
}
