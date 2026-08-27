/**
 * The provenance rule, which is the one piece of judgement in `format.ts`.
 *
 * Spec §4 pairs a row's sync age with its source's health, and the plan's
 * wording was *"replaced by the source's health when it is not `ok`"*. Taken
 * literally that is wrong in the loudest possible way, and the boundary is
 * what these pin.
 */
import { expect, test } from "vitest";

import type { AuthState, CredentialHealth } from "../ipc/sources";
import { provenance, sourceMonogram, syncAge } from "./format";

const NOW = new Date("2026-08-25T12:00:00Z");

function health(state: AuthState): CredentialHealth[] {
  return [{ source_id: "jira", state, checked_at: null, detail: null, secret_expires_at: null }];
}

test("a sync age reads the way §4 quotes it", () => {
  expect(syncAge("2026-08-25T11:56:00Z", NOW)).toBe("synced 4 min ago");
  expect(syncAge("2026-08-25T11:59:40Z", NOW)).toBe("synced just now");
});

test("only an actionable state replaces the age", () => {
  for (const state of ["unauthorized", "unreachable", "missing_secret"] as AuthState[]) {
    const got = provenance("jira", "2026-08-25T11:56:00Z", health(state), NOW);
    expect(got.failing, state).toBe(true);
    expect(got.text, state).not.toContain("synced");
    expect(got.text, state).toContain("jira · ");
  }
});

/**
 * `unknown` is migration 0002's default: nothing has tested the credential
 * yet, which is the state **every** source is in until stream F's scheduler
 * runs. Reading it as "not ok" would put a health complaint on every row of a
 * fresh install.
 */
test("a source nobody has tested yet still shows its sync age", () => {
  for (const state of ["ok", "unknown"] as AuthState[]) {
    const got = provenance("jira", "2026-08-25T11:56:00Z", health(state), NOW);
    expect(got.failing, state).toBe(false);
    expect(got.text, state).toBe("synced 4 min ago");
  }
});

test("a source with no health row at all shows its sync age", () => {
  const got = provenance("gitea", "2026-08-25T11:56:00Z", health("unauthorized"), NOW);
  expect(got.failing).toBe(false);
  expect(got.text).toBe("synced 4 min ago");
});

/**
 * Derived, never looked up. §3a forbids a table of adapter names, so the
 * monogram of an id knobas has never seen is the first two letters of it —
 * including where a human would have written something else (`teamcity` gives
 * `TE`, not the mockup's hand-written `TC`).
 */
test("a monogram is the first two letters of the id, upper-cased", () => {
  expect(sourceMonogram("jira")).toBe("JI");
  expect(sourceMonogram("teamcity")).toBe("TE");
  expect(sourceMonogram("jira-eu")).toBe("JI");
  expect(sourceMonogram("g")).toBe("G");
  expect(sourceMonogram("")).toBe("?");
});
