/**
 * `?fake-ipc`: The boot screen: `app_status` (held on one state by
 * `?fake-db=starting|migrating|failed`; with no source under `?fake-empty=sources`, a fresh profile, ux31) and
 * `demo_load`. `?fake-secret-store=memory` answers as a process under
 * `KNOBAS_SECRET_STORE=memory` does: credentials for this session only (#1585).
 */
import type { DbState } from "../../../ipc/app";
import type { Handlers } from "../fake-tauri";

/** Everything in `AppStatus` that is not the lifecycle state. */
const DEMO_STATUS = {
  demo: true,
  source_count: 1,
  app_version: "0.1.0-fixture",
  secrets_in_memory: false,
};

/**
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state.
 *
 * Without it the fake answers `ready` immediately, which is right for QA of
 * every other screen and useless for QA of this one — the loading screen would
 * be gone before the shutter opened.
 */
function fakeDbState(params: URLSearchParams): DbState {
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
export function handlers(params: URLSearchParams): Handlers {
  // A fresh profile counts no source; the shell reads its own lists, so this is the one number to agree with them.
  const noSources = (params.get("fake-empty") ?? "").split(",").includes("sources");
  return {
    ping: () => "pong",
    app_status: () => ({
      db: fakeDbState(params),
      ...DEMO_STATUS,
      ...(noSources ? { source_count: 0 } : {}),
      ...(params.get("fake-secret-store") === "memory" ? { secrets_in_memory: true } : {}),
    }),
    frontend_ready: () => null,
    retry_database: () => null,
    demo_load: () => ({ source_id: "mock", upserted: 21, deleted: 0, swept: 0, cursor: "" }),
  };
}
