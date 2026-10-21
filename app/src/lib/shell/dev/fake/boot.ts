/**
 * `?fake-ipc`: The boot screen and the first run: `app_status` (held on one state by
 * `?fake-db=starting|migrating|failed`, and on the wizard by `?fake-first-run`) and
 * the wizard's two writes.
 */
import type { Handler } from "../fake-tauri";

/** Everything in `AppStatus` that is not the lifecycle state. */
const DEMO_STATUS = {
  first_run: false,
  demo: true,
  source_count: 1,
  app_version: "0.1.0-fixture",
};

/**
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state.
 *
 * Without it the fake answers `ready` immediately, which is right for QA of
 * every other screen and useless for QA of this one — the loading screen would
 * be gone before the shutter opened.
 */
function fakeDbState(params: URLSearchParams): unknown {
  switch (params.get("fake-db")) {
    case "starting":
      return { state: "starting", detail: "first run: downloading and initialising PostgreSQL" };
    case "migrating":
      return { state: "migrating" };
    case "failed":
      return { state: "failed", message: "port 50861 is in use by another process" };
    default:
      return { state: "ready" };
  }
}

/** This feature's commands. */
export function handlers(params: URLSearchParams): Record<string, Handler> {
  // `?fake-first-run` opens the §14a wizard, which the fixture otherwise never
  // shows: a first run with no source yet, until the wizard finishes (#1396).
  let firstRun = params.has("fake-first-run");
  return {
    ping: () => "pong",
    app_status: () => ({
      db: fakeDbState(params),
      ...DEMO_STATUS,
      ...(firstRun ? { first_run: true, source_count: 0 } : {}),
    }),
    frontend_ready: () => null,
    retry_database: () => null,
    demo_load: () => ({ source_id: "mock", upserted: 21, deleted: 0, swept: 0, cursor: "" }),
    complete_first_run: () => {
      firstRun = false;
      return null;
    },
  };
}
