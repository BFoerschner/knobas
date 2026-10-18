/**
 * The Builds section's warnings (R-4.7) are for the sources that hold builds:
 * the ones claiming the CI role (#1196), never the ones whose adapter happens
 * to declare a kind named `build`.
 */
import { expect, test } from "vitest";

import type { CredentialHealth } from "../../ipc/sources";
import { staleBuilds } from "./job";

const NOW = new Date("2026-10-18T12:00:00Z");
const refused = (source_id: string): CredentialHealth => ({
  source_id,
  state: "unauthorized",
  checked_at: "2026-10-18T11:20:00Z",
  detail: null,
  secret_expires_at: null,
});

test("a source claiming ci warns with no build drawn; one only declaring a build kind does not", () => {
  const sources = [
    { id: "relay", display_name: "Relay CI", roles: [{ role: "ci", reads: ["run_log"] }] },
    { id: "tracker", display_name: "Tracker", kinds: [{ id: "build" }], roles: [{ role: "ticket-system", reads: [] }] },
  ];

  const warned = staleBuilds(sources, [refused("relay"), refused("tracker")], [], [], NOW);

  expect(warned.map((row) => row.text)).toEqual(["Relay CI data is 40 min old: sign-in failing."]);
});
