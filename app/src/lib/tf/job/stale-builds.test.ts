/**
 * The Builds section's warnings (R-4.7) are for the sources that hold builds:
 * the ones claiming the CI role (#1196), never the ones whose adapter happens
 * to declare a kind named `build`.
 */
import { expect, test } from "vitest";

import type { CredentialHealth } from "../../ipc/sources";
import { staleBuilds } from "./job";
import { sourceFacts } from "./source-facts.fixture";

const NOW = new Date("2026-10-18T12:00:00Z");
const refused = (source_id: string): CredentialHealth => ({
  source_id,
  state: "unauthorized",
  checked_at: "2026-10-18T11:20:00Z",
  detail: null,
  secret_expires_at: null,
});

test("a source claiming ci warns with no build drawn; one claiming only the ticket-system role does not", () => {
  const sources = [
    sourceFacts("relay", { display_name: "Relay CI", roles: [{ role: "ci", reads: ["run_log"] }] }),
    sourceFacts("tracker", { display_name: "Tracker", roles: [{ role: "ticket-system", reads: [] }] }),
  ];

  const warned = staleBuilds(sources, [refused("relay"), refused("tracker")], [], [], NOW);

  expect(warned.map((row) => row.text)).toEqual(["Relay CI data is 40 min old: sign-in failing."]);
});

test("the warning names the source's credential: no token stored, Re-enter token (#1405)", () => {
  const sources = [
    sourceFacts("relay", { display_name: "Relay CI", auth_kind: "Pat", roles: [{ role: "ci", reads: [] }] }),
    sourceFacts("jenkins", { display_name: "Jenkins", auth_kind: "UserPassword", roles: [{ role: "ci", reads: [] }] }),
  ];
  const missing = (source_id: string): CredentialHealth => ({ ...refused(source_id), state: "missing_secret" });

  const warned = staleBuilds(sources, [missing("relay"), missing("jenkins")], [], [], NOW);

  expect(warned).toEqual([
    { sourceId: "relay", text: "Relay CI data is 40 min old: no token stored.", fix: "Re-enter token" },
    { sourceId: "jenkins", text: "Jenkins data is 40 min old: no password stored.", fix: "Re-enter password" },
  ]);
});
