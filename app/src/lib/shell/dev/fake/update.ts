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
import type { FakeBridge, Handlers } from "../fake-tauri";
import { onFakeReset } from "../fake-seed";

const AT = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();
const MB = 1_000_000;

/** The pieces every state is made of. */
function base(): UpdateState {
  return {
    current: "3.2.9-ux27",
    current_is_pre_release: true,
    current_schema: 101,
    set_up: true,
    check: true,
    channel: "pre_releases",
    standing: { standing: "in_place", kind: "nsis" },
    checks_itself: true,
    checked_at: AT(12),
    next_check_at: new Date(Date.now() + 348 * 60_000).toISOString(),
    check_failed: null,
    newest: { version: "3.2.9-ux27", pre_release: true, published_at: AT(60 * 26), page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9-ux27" },
    offer: null,
    phase: { phase: "idle" },
    marked: false,
    last: { from: "3.2.9-ux26", to: "3.2.9-ux27", at: AT(60 * 26), installed: true, backup: "knobas-20261120-110148.knobas" },
    just_updated: false,
  };
}

const OFFER = {
  version: "3.2.9-ux28",
  pre_release: true,
  published_at: AT(240),
  bytes: 89 * MB,
  changes_schema: true,
  page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9-ux28",
};

/** One named state. */
export function fakeUpdate(name: string | null): UpdateState {
  const up = base();
  const offered: UpdateState = {
    ...up,
    newest: { version: OFFER.version, pre_release: true, published_at: OFFER.published_at, page_url: OFFER.page_url },
    offer: OFFER,
    marked: true,
  };
  switch (name) {
    case "checks-off":
      return { ...up, check: false, next_check_at: null };
    case "checking":
      return { ...up, phase: { phase: "checking" } };
    case "available":
      return offered;
    case "downloading":
      return { ...offered, phase: { phase: "downloading", received: 41 * MB, total: 89 * MB } };
    case "ready":
      return { ...offered, phase: { phase: "ready" } };
    case "installing":
      return { ...offered, phase: { phase: "installing", step: "backing_up" } };
    case "signature":
      return { ...offered, phase: { phase: "failed", stage: "signature", message: "the signature does not match the file", received: null } };
    case "download-failed":
      return { ...offered, phase: { phase: "failed", stage: "download", message: "no bytes for 30s", received: 41 * MB } };
    case "not-installed":
      return { ...offered, phase: { phase: "ready" }, last: { from: "3.2.9-ux27", to: "3.2.9-ux28", at: AT(30), installed: false, backup: "knobas-20261121-143012.knobas" } };
    case "check-failed":
      return {
        ...up,
        channel: "releases",
        current: "3.2.9",
        current_is_pre_release: false,
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9" },
        check_failed: { kind: "offline", message: "error sending request", at: AT(2) },
      };
    case "package":
      return {
        ...offered,
        current: "3.2.8",
        current_is_pre_release: false,
        channel: "releases",
        standing: { standing: "by_hand", why: "package", kind: "deb" },
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9" },
        offer: { ...OFFER, version: "3.2.9", pre_release: false, bytes: 63 * MB, page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9" },
        last: null,
      };
    case "pre-on-releases":
      return {
        ...up,
        current: "3.2.10-ux3",
        channel: "releases",
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9" },
        last: null,
      };
    case "development":
      return { ...up, standing: { standing: "by_hand", why: "development", kind: null }, checks_itself: false, next_check_at: null, checked_at: null, newest: null, last: null };
    case "not-set-up":
      return { ...up, set_up: false, checks_itself: false, next_check_at: null, checked_at: null, newest: null, last: null };
    default:
      return up;
  }
}

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
      return { version, markdown: NOTES.replaceAll("3.2.9-ux28", version), page_url: `https://github.com/BFoerschner/knobas/releases/tag/v${version}` };
    },
  };
}
