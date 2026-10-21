/**
 * The one toast a queued write raises (#1438, R-15.1): "Queued: <what>." with
 * *Show queue*, and "<Source> sign-in is failing, so it waits there." while
 * the source's credential needs the reader. Never a promise of the next sync:
 * a write goes the moment it is queued, paused sync or not.
 */
import { beforeEach, expect, test } from "vitest";

import type { AuthState, CredentialHealth } from "../ipc/sources";
import { createHealth } from "./health.svelte";
import { toastQueued, toastStarted } from "./queued-toast";
import { toasts } from "./toasts.svelte";
import { writeQueuePanel } from "./write-queue.svelte";

const TEAMCITY = { id: "teamcity", display_name: "TeamCity" };

function reading(state: AuthState): CredentialHealth {
  return { source_id: "teamcity", state, checked_at: "2026-10-21T09:00:00Z", detail: null, secret_expires_at: null };
}

function storeWith(state: AuthState | null) {
  const store = createHealth({ credentialHealth: () => Promise.resolve([]), listen: () => Promise.resolve(() => {}) });
  if (state !== null) store.patch(reading(state));
  return store;
}

beforeEach(() => {
  toasts.items.length = 0;
  writeQueuePanel.open = false;
});

test("a healthy source's write says what was queued, and Show queue opens the queue", () => {
  toastQueued("re-run Payout_IntegrationTests #1187", TEAMCITY, storeWith("ok"));
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Queued: re-run Payout_IntegrationTests #1187.");
  expect(toast.action?.label).toBe("Show queue");
  toast.action!.run();
  expect(writeQueuePanel.open).toBe(true);
});

test("a rejected credential says the write waits for the sign-in", () => {
  toastQueued("re-run Payout_IntegrationTests #1187", TEAMCITY, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe(
    "Queued: re-run Payout_IntegrationTests #1187. TeamCity sign-in is failing, so it waits there.",
  );
});

test("no stored secret is a failing sign-in too", () => {
  toastQueued("set env on Payout_IntegrationTests", TEAMCITY, storeWith("missing_secret"));
  expect(toasts.items.at(-1)!.text).toBe(
    "Queued: set env on Payout_IntegrationTests. TeamCity sign-in is failing, so it waits there.",
  );
});

test("an unreachable server, an untested credential, or no reading at all names no sign-in", () => {
  for (const state of ["unreachable", "unknown", null] as const) {
    toastQueued("PAY-231 to Done", TEAMCITY, storeWith(state));
    expect(toasts.items.at(-1)!.text).toBe("Queued: PAY-231 to Done.");
  }
});

test("a write whose source the page does not know says what was queued and nothing more", () => {
  toastQueued("comment on PAY-231", null, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe("Queued: comment on PAY-231.");
});

test("no toast promises the next sync", () => {
  for (const state of ["ok", "unknown", "unauthorized", "missing_secret", "unreachable", null] as const) {
    toastQueued("merge PR #142", TEAMCITY, storeWith(state));
  }
  expect(toasts.items.every((toast) => !toast.text.includes("next sync"))).toBe(true);
});

test("Start work's toast adds the sign-in sentence only when something waits (#1426)", () => {
  toastStarted("PAY-240", 2, TEAMCITY, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe("Started work on PAY-240. 2 writes queued. TeamCity sign-in is failing, so it waits there.");
  toastStarted("PAY-240", 0, TEAMCITY, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe("Started work on PAY-240.");
});
