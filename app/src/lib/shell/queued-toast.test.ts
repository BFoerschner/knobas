/**
 * The one toast a queued write raises (#1438, R-15.1): "Queued: <what>." with
 * *Show queue*, and "<Source> sign-in is failing, so it waits there." while
 * the source's credential needs the reader. Never a promise of the next sync:
 * a write goes the moment it is queued, paused sync or not.
 */
import { beforeEach, expect, test } from "vitest";

import type { AuthState, CredentialHealth } from "../ipc/sources";
import { createHealth } from "./health.svelte";
import type { Hear, WriteFollower } from "./follow-write";
import type { QueuedWrite } from "../ipc/sources";
import { toastQueued, toastStarted } from "./queued-toast";
import { toasts } from "./toasts.svelte";
import { noteWithdrawn, reset as resetQueue, writeQueuePanel } from "./write-queue.svelte";

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

/** A follower the test speaks for: `hear(row)` is the queue's next read of the write. */
function heardFollower(): { follower: WriteFollower; hear: Hear; follows: () => number } {
  let heard: Hear = () => {};
  let open = 0;
  return {
    follower: {
      follow: (_id, hear) => {
        heard = hear;
        open += 1;
        return () => {
          open -= 1;
          heard = () => {};
        };
      },
    },
    hear: (row) => heard(row),
    follows: () => open,
  };
}

const HELD = { id: 7, state: "held", held_snapshot: null, detail: "changed at the source" } as unknown as QueuedWrite;

test("a queued toast says Sent in place once its write left the queue, and offers no queue that no longer holds it", () => {
  const { follower, hear, follows } = heardFollower();
  toastQueued("comment on Payout design", TEAMCITY, { store: storeWith(null), write: { id: 7 }, follower });
  expect(toasts.items.at(-1)!.text).toBe("Queued: comment on Payout design.");
  expect(toasts.items.at(-1)!.action?.label).toBe("Show queue");
  hear(undefined);
  expect(toasts.items).toHaveLength(1);
  expect(toasts.items[0]!.text).toBe("Sent: comment on Payout design.");
  expect(toasts.items[0]!.action).toBeUndefined();
  expect(follows()).toBe(0);
});

test("a write already sent as it answers says Sent at once, with nothing to follow", () => {
  const { follower, follows } = heardFollower();
  toastQueued("comment on Payout design", TEAMCITY, { store: storeWith(null), write: { id: 7, state: "sent" }, follower });
  expect(toasts.items.at(-1)!.text).toBe("Sent: comment on Payout design.");
  expect(follows()).toBe(0);
});

test("a write withdrawn in the queue takes its toast with it, because nothing was sent", () => {
  resetQueue();
  const { follower, follows } = heardFollower();
  toastQueued("comment on Payout design", TEAMCITY, { store: storeWith(null), write: { id: 7 }, follower });
  noteWithdrawn(7);
  expect(toasts.items).toHaveLength(0);
  expect(follows()).toBe(0);
});

test("a held write keeps saying Queued and offering the queue, which is where it is", () => {
  const { follower, hear, follows } = heardFollower();
  toastQueued("comment on Payout design", TEAMCITY, { store: storeWith(null), write: { id: 7 }, follower });
  hear(HELD);
  expect(toasts.items.at(-1)!.text).toBe("Queued: comment on Payout design.");
  expect(toasts.items.at(-1)!.action?.label).toBe("Show queue");
  expect(follows()).toBe(0);
});

test("a toast that has gone stops following its write", () => {
  const { follower, hear, follows } = heardFollower();
  toastQueued("comment on Payout design", TEAMCITY, { store: storeWith(null), write: { id: 7 }, follower });
  toasts.items.length = 0;
  hear({ ...HELD, state: "pending" } as QueuedWrite);
  expect(follows()).toBe(0);
});

test("a healthy source's write says what was queued, and Show queue opens the queue", () => {
  toastQueued("re-run Payout_IntegrationTests #1187", TEAMCITY, { store: storeWith("ok") });
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Queued: re-run Payout_IntegrationTests #1187.");
  expect(toast.action?.label).toBe("Show queue");
  toast.action!.run();
  expect(writeQueuePanel.open).toBe(true);
});

test("a rejected credential says the write waits for the sign-in", () => {
  toastQueued("re-run Payout_IntegrationTests #1187", TEAMCITY, { store: storeWith("unauthorized") });
  expect(toasts.items.at(-1)!.text).toBe(
    "Queued: re-run Payout_IntegrationTests #1187. TeamCity sign-in is failing, so it waits there.",
  );
});

test("no stored secret is a failing sign-in too", () => {
  toastQueued("set env on Payout_IntegrationTests", TEAMCITY, { store: storeWith("missing_secret") });
  expect(toasts.items.at(-1)!.text).toBe(
    "Queued: set env on Payout_IntegrationTests. TeamCity sign-in is failing, so it waits there.",
  );
});

test("an unreachable server, an untested credential, or no reading at all names no sign-in", () => {
  for (const state of ["unreachable", "unknown", null] as const) {
    toastQueued("PAY-231 to Done", TEAMCITY, { store: storeWith(state) });
    expect(toasts.items.at(-1)!.text).toBe("Queued: PAY-231 to Done.");
  }
});

test("a write whose source the page does not know says what was queued and nothing more", () => {
  toastQueued("comment on PAY-231", null, { store: storeWith("unauthorized") });
  expect(toasts.items.at(-1)!.text).toBe("Queued: comment on PAY-231.");
});

test("no toast promises the next sync", () => {
  for (const state of ["ok", "unknown", "unauthorized", "missing_secret", "unreachable", null] as const) {
    toastQueued("merge PR #142", TEAMCITY, { store: storeWith(state) });
  }
  expect(toasts.items.every((toast) => !toast.text.includes("next sync"))).toBe(true);
});

test("Start work's toast adds the sign-in sentence only when something waits (#1426)", () => {
  toastStarted("PAY-240", 2, TEAMCITY, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe("Started work on PAY-240. 2 writes queued. TeamCity sign-in is failing, so it waits there.");
  toastStarted("PAY-240", 0, TEAMCITY, storeWith("unauthorized"));
  expect(toasts.items.at(-1)!.text).toBe("Started work on PAY-240.");
});

test("once the write has gone, a toast given a where offers it in place of the queue (F-350)", () => {
  let hear: Hear = () => {};
  const follower: WriteFollower = { follow: (_id, heard) => ((hear = heard), () => {}) };
  const shown: string[] = [];
  toastQueued("re-run Payout_Lint #4811", null, { write: { id: 7 }, follower, where: { label: "Show in runs", run: () => shown.push("runs") } });
  expect(toasts.items.at(-1)!.action!.label).toBe("Show queue");

  hear(undefined);
  expect(toasts.items.at(-1)!.text).toBe("Sent: re-run Payout_Lint #4811.");
  expect(toasts.items.at(-1)!.action!.label).toBe("Show in runs");
  toasts.items.at(-1)!.action!.run();
  expect(shown).toEqual(["runs"]);
});

test("a write already sent offers its where straight away", () => {
  toastQueued("re-run Payout_Lint #4811", null, { write: { id: 7, state: "sent" }, where: { label: "Show in runs", run: () => {} } });
  expect(toasts.items.at(-1)!.text).toBe("Sent: re-run Payout_Lint #4811.");
  expect(toasts.items.at(-1)!.action!.label).toBe("Show in runs");
});
