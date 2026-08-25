/**
 * Hash addressing (spec §2 "Entity addressing"), adopted as the navigation
 * contract.
 *
 * The one non-obvious rule has its own test: a `#` inside an entity key —
 * Gitea's `owner/repo#142`, TeamCity's `Build#1188` — truncates the fragment
 * unless it is percent-encoded. `hashFor` encodes, `parseHash` decodes, and
 * the two have to stay each other's inverse.
 */
import { expect, test } from "vitest";

import { createRouter, hashFor, parseHash } from "./router.svelte";

test("parses the M1 addresses", () => {
  expect(parseHash("#/ctx/all")).toEqual({ view: "room", ctx: "all", detail: null });
  expect(parseHash("#/sources")).toEqual({ view: "sources" });
  expect(parseHash("#/first-run")).toEqual({ view: "first-run" });
});

test("an empty hash is the default room, not an error", () => {
  for (const empty of ["", "#", "#/"]) {
    expect(parseHash(empty)).toEqual({ view: "room", ctx: "all", detail: null });
  }
});

/**
 * Open kinds (§3a): the router must not carry a closed list of kinds, or an
 * adapter that declares a kind nobody anticipated becomes unaddressable.
 */
test("any unreserved first segment is a kind, not a special view", () => {
  expect(parseHash("#/ticket/mock:PAY-231")).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });
  expect(parseHash("#/deployment/acme:d-7")).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "deployment", entityId: "acme:d-7" },
  });
});

/** The kind-agnostic alias, resolved to its canonical form by the caller. */
test("#/entity/<id> is a detail whose kind is not known yet", () => {
  expect(parseHash("#/entity/mock:PAY-231")).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: null, entityId: "mock:PAY-231" },
  });
});

/** M2–M4 addresses parse, so the shell can say "arrives in M<n>" rather than
 * rendering a blank screen or, worse, treating `inbox` as a kind. */
test("a later milestone's address is known-unknown, not a kind", () => {
  for (const hash of ["#/inbox", "#/time", "#/standup", "#/assets/board", "#/monitor/db-1"]) {
    expect(parseHash(hash)).toEqual({ view: "unknown", hash });
  }
});

test("round-trips every address it produces", () => {
  for (const hash of [
    "#/ctx/all",
    "#/ctx/src:jira",
    "#/ticket/mock:PAY-231",
    "#/entity/mock:PAY-231",
    "#/sources",
    "#/first-run",
    "#/inbox",
  ]) {
    expect(hashFor(parseHash(hash))).toBe(hash);
  }
});

test("percent-encoded ids survive the round trip", () => {
  const parsed = parseHash("#/pr/mock:payout-service%23142");
  expect(parsed.view === "room" && parsed.detail?.entityId).toBe("mock:payout-service#142");
  // ...and building the address back encodes it again, or the browser drops
  // everything from the `#` onwards.
  expect(hashFor(parsed)).toBe("#/pr/mock:payout-service%23142");
});

test("a slash inside a key is encoded rather than read as another segment", () => {
  const hash = hashFor({
    view: "room",
    ctx: "all",
    detail: { kind: "pr", entityId: "gitea:acme/payout-service#142" },
  });
  expect(hash).toBe("#/pr/gitea:acme%2Fpayout-service%23142");
  expect(parseHash(hash)).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "pr", entityId: "gitea:acme/payout-service#142" },
  });
});

/**
 * A detail address does not say which room it was opened over, so the router
 * remembers. Esc has to land back in the room the reader came from, not in a
 * default one.
 */
test("a detail keeps the room it was opened over", () => {
  expect(parseHash("#/ticket/mock:PAY-231", "src:jira")).toEqual({
    view: "room",
    ctx: "src:jira",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });
});

test("a malformed escape is treated as literal text rather than throwing", () => {
  // `decodeURIComponent("%zz")` throws. A hash the user typed must not take
  // the window down.
  const parsed = parseHash("#/ticket/mock:%zz");
  expect(parsed).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "ticket", entityId: "mock:%zz" },
  });
});

/**
 * The live router's memory of the room, as distinct from `parseHash`'s
 * `ctx` argument.
 *
 * A detail address does not carry the room it was opened over, so the router
 * holds it. Losing that is how `Esc` from a ticket opened in `src:jira` lands
 * the reader in `all` — a navigation they did not ask for, in the one gesture
 * that is supposed to undo.
 */
test("the router remembers the room a detail was opened over", () => {
  location.hash = "#/ctx/src:jira";
  const router = createRouter();
  const stop = router.start();

  expect(router.ctx).toBe("src:jira");

  router.go("#/ticket/mock:PAY-231");
  expect(router.ctx).toBe("src:jira");
  expect(router.route).toEqual({
    view: "room",
    ctx: "src:jira",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });

  router.back();
  expect(location.hash).toBe("#/ctx/src:jira");

  stop();
});

/** A view that is not a room leaves the remembered room alone. */
test("visiting the sources view does not forget the room behind it", () => {
  location.hash = "#/ctx/src:jira";
  const router = createRouter();
  const stop = router.start();

  router.go("#/sources");
  expect(router.ctx).toBe("src:jira");
  router.back();
  expect(location.hash).toBe("#/ctx/src:jira");

  stop();
});
