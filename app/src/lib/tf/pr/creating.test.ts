import { afterEach, expect, test } from "vitest";

import { echoEvents } from "../echo-events.fixture";
import { creatingHash, creatingOf, forgetCreates, noteCreate, readCreatingRoute } from "./creating.svelte";

const REPO = "gitea:tidewater/payout-service";
const PR = { entity: REPO, title: "Instant SEPA spike", body: "", head: "spike/sepa-instant", base: "main" };
const settle = () => Promise.resolve();

afterEach(forgetCreates);

test("a create's page is addressed by its write until the source answers, and the read-back names the pull request it made (#2584)", async () => {
  const events = echoEvents(settle);
  expect(noteCreate(7, PR, events.listen)).toBe("#/pr/new/7");
  expect(creatingHash(7)).toBe("#/pr/new/7");
  expect(creatingOf(7)).toMatchObject({ write: 7, repo: REPO, title: "Instant SEPA spike", head: "spike/sepa-instant", base: "main", made: null });

  // Another write's read-back, a refresh the reader asked for, and the create's own target are not what it made.
  await events.refreshed("gitea:tidewater/payout-service#151", 8);
  await events.refreshed("gitea:tidewater/payout-service#152", null);
  await events.refreshed(REPO, 7);
  expect(creatingOf(7)?.made).toBeNull();

  await events.refreshed("gitea:tidewater/payout-service#153", 7);
  expect(creatingOf(7)?.made).toBe("gitea:tidewater/payout-service#153");
});

test("the address reads back to its write; any other is not a create's (#2584)", () => {
  expect(readCreatingRoute(["new", "7"])).toBe(7);
  expect(readCreatingRoute(["gitea%3Atidewater%2Fpayout-service%23151"])).toBeNull();
  expect(readCreatingRoute(["new"])).toBeNull();
  expect(readCreatingRoute(["new", "x"])).toBeNull();
});
