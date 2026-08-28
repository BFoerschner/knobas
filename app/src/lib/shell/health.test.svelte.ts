/**
 * Credential health as one live fact, shared by everything that draws it.
 *
 * Before this module there were two independent copies of it — the launcher's
 * per-opening `launcher_home.sources` fetch and a `sources` prop nothing
 * supplied — and the failure that produced (issue #27) is the one worth
 * pinning here: a reading that is *correct today* because the second copy
 * happens to be empty, and wrong the day something fills it.
 */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { AuthState, CredentialHealth } from "../ipc/sources";
import { createHealth, expiryNote, isActionable } from "./health.svelte";

function row(source_id: string, state: AuthState, extra: Partial<CredentialHealth> = {}): CredentialHealth {
  return {
    source_id,
    state,
    checked_at: "2026-08-22T10:00:00Z",
    detail: null,
    secret_expires_at: null,
    ...extra,
  };
}

/** A `listen()` stand-in that hands the test the handler it registered. */
function fakeListen() {
  const handlers: ((payload: CredentialHealth) => void)[] = [];
  let unlistened = 0;
  return {
    handlers,
    get unlistened() {
      return unlistened;
    },
    emit(payload: CredentialHealth) {
      for (const handler of handlers) handler(payload);
    },
    listen: (_event: string, handler: (event: { payload: CredentialHealth }) => void) => {
      handlers.push((payload) => handler({ payload }));
      return Promise.resolve(() => {
        unlistened += 1;
      });
    },
  };
}

test("seeds from credentialHealth and orders by source id", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.resolve([row("teamcity", "ok"), row("gitea", "unauthorized")]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(health.all.length).toBe(2));
  expect(health.all.map((h) => h.source_id)).toEqual(["gitea", "teamcity"]);
  expect(health.get("gitea")?.state).toBe("unauthorized");
  expect(health.get("nobody")).toBeNull();
  stop();
});

test("a source:health event patches one source and leaves the others alone", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.resolve([row("jira", "ok"), row("gitea", "ok")]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(1));
  await vi.waitFor(() => expect(health.all.length).toBe(2));

  events.emit(row("gitea", "unauthorized", { detail: "401 from /api/v1/user" }));
  flushSync();

  expect(health.get("gitea")?.state).toBe("unauthorized");
  expect(health.get("gitea")?.detail).toBe("401 from /api/v1/user");
  // The point of the test: the *other* source is untouched. A store that
  // replaced the whole map on an event would pass a "gitea is unauthorized"
  // assertion and lose jira.
  expect(health.get("jira")?.state).toBe("ok");
  stop();
});

test("an event for a source the seed never mentioned adds it", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(1));

  // A source added in another window, or by the first-run wizard: the strip
  // must show it without waiting for a re-seed.
  events.emit(row("jira", "ok"));
  flushSync();
  expect(health.all.map((h) => h.source_id)).toEqual(["jira"]);
  stop();
});

test("stopping unsubscribes, and a late listen resolution does not leak", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(1));
  stop();
  expect(events.unlistened).toBe(1);
});

test("stopping before listen resolves still unsubscribes", async () => {
  let resolveListen: ((unlisten: () => void) => void) | null = null;
  let unlistened = 0;
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: () =>
      new Promise<() => void>((resolve) => {
        resolveListen = resolve;
      }),
  });
  const stop = health.start();
  stop();
  // `listen` is itself an `invoke`, so it resolves a tick or more after the
  // component that asked for it may already be gone.
  await vi.waitFor(() => expect(resolveListen).not.toBeNull());
  resolveListen!(() => {
    unlistened += 1;
  });
  await vi.waitFor(() => expect(unlistened).toBe(1));
});

test("a seed that resolves after stop is discarded", async () => {
  let resolveSeed: ((rows: CredentialHealth[]) => void) | null = null;
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () =>
      new Promise<CredentialHealth[]>((resolve) => {
        resolveSeed = resolve;
      }),
    listen: events.listen,
  });
  const stop = health.start();
  stop();
  await vi.waitFor(() => expect(resolveSeed).not.toBeNull());
  resolveSeed!([row("jira", "ok")]);
  await Promise.resolve();
  flushSync();
  expect(health.all).toEqual([]);
});

test("a failed seed leaves the store empty rather than throwing into the shell", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.reject({ code: "not_ready", message: "database is starting", source_id: null }),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(1));
  expect(health.all).toEqual([]);
  // ...and the subscription still works, so the store recovers the moment the
  // scheduler reports anything.
  events.emit(row("jira", "ok"));
  flushSync();
  expect(health.all.length).toBe(1);
  stop();
});

test("failing lists only the states a human can act on", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () =>
      Promise.resolve([
        row("a", "ok"),
        row("b", "unauthorized"),
        row("c", "unknown"),
        row("d", "unreachable"),
        row("e", "missing_secret"),
      ]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(health.all.length).toBe(5));
  // `unknown` is migration 0002's default — nothing has tested the credential
  // yet — so it is not a complaint. Counting it would put a red dot on every
  // source of a fresh install.
  expect(health.failing.map((h) => h.source_id)).toEqual(["b", "d", "e"]);
  expect(health.unauthorized).toBe(true);
  stop();
});

test("unauthorized is what the top strip's 401 reading keys on, not any failure", async () => {
  const events = fakeListen();
  const health = createHealth({
    credentialHealth: () => Promise.resolve([row("a", "unreachable")]),
    listen: events.listen,
  });
  const stop = health.start();
  await vi.waitFor(() => expect(health.all.length).toBe(1));
  expect(health.failing.length).toBe(1);
  expect(health.unauthorized).toBe(false);
  stop();
});

/**
 * The rule itself, read through the one function every surface reads it
 * through. Over `AuthState::ALL`'s spellings rather than over the three that
 * are `true`, so `ok` and `unknown` are asserted *not* actionable rather than
 * merely absent — a table that said `true` everywhere would pass a
 * three-member check.
 */
test("isActionable is the one spelling of `needs a human`", () => {
  const states: AuthState[] = [
    "ok",
    "unknown",
    "unauthorized",
    "unreachable",
    "missing_secret",
  ];
  expect(states.filter(isActionable).sort()).toEqual([
    "missing_secret",
    "unauthorized",
    "unreachable",
  ]);
});

/**
 * The bridge can hand over a state this build has never heard of — a variant
 * added on the Rust side against an older window. "Do not shout about it" is
 * the same safe answer `unknown` gets, and it is why the lookup is `=== true`
 * rather than bare.
 */
test("a state this build does not know is not shouted about", () => {
  expect(isActionable("expired_in_a_future_release" as AuthState)).toBe(false);
});

const NOW = new Date("2026-08-22T00:00:00Z");

test("a PAT expiring in 12 days reads amber; in 41 days it reads plain", () => {
  expect(expiryNote("2026-09-03T00:00:00Z", NOW)).toEqual({
    text: "PAT expires in 12 days",
    tone: "amber",
  });
  expect(expiryNote("2026-10-02T00:00:00Z", NOW)).toEqual({
    text: "PAT expires in 41 days",
    tone: "plain",
  });
  expect(expiryNote(null, NOW)).toBeNull();
});

test("an already-expired secret reads failed, not '-3 days'", () => {
  expect(expiryNote("2026-08-19T00:00:00Z", NOW)).toEqual({ text: "PAT expired", tone: "fail" });
  // The boundary itself: a secret that expires this instant has expired.
  expect(expiryNote("2026-08-22T00:00:00Z", NOW)).toEqual({ text: "PAT expired", tone: "fail" });
});

test("the last day is singular and amber, not '1 days'", () => {
  expect(expiryNote("2026-08-23T00:00:00Z", NOW)).toEqual({
    text: "PAT expires in 1 day",
    tone: "amber",
  });
});

test("a malformed stamp says nothing rather than NaN", () => {
  expect(expiryNote("whenever", NOW)).toBeNull();
});
