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
  expect(parseHash("#/settings")).toEqual({ view: "settings" });
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

/**
 * A note has a stable address like anything else (#46 story 17).
 *
 * `note` was a **reserved view word** until #46 -- listed with `inbox` and
 * `time` so that a later milestone's address rendered "arrives in M<n>". Notes
 * arrived, so it is a kind now, and this is the test that fails if it is ever
 * put back: a reserved `note` makes every note in the app unopenable, and the
 * symptom is a slide-over that says the address is from a later milestone.
 */
test("a note's address is a kind, because notes are a kind now", () => {
  expect(parseHash("#/note/note:7f2c-4b2f")).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "note", entityId: "note:7f2c-4b2f" },
  });
  // ...and the kind-agnostic alias reaches it too.
  expect(parseHash("#/entity/note:7f2c-4b2f")).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: null, entityId: "note:7f2c-4b2f" },
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

/** M3–M4 addresses parse, so the shell can say "arrives in M<n>" rather than
 * rendering a blank screen or, worse, treating `standup` as a kind. */
test("a later milestone's address is known-unknown, not a kind", () => {
  for (const hash of ["#/time", "#/standup", "#/assets/board", "#/monitor/db-1"]) {
    expect(parseHash(hash)).toEqual({ view: "unknown", hash });
  }
});

/**
 * The flow's address is its identity, so it carries a ticket id and reaches a
 * view of its own -- not the "arrives in a later milestone" pane it reached
 * before #44, and not the open-kind branch, which would send `start-work` to
 * `get_entity` as if it were a kind.
 */
test("the start-work address reaches its own view, carrying the ticket", () => {
  expect(parseHash("#/start-work/gitea:acme/repo%23142")).toEqual({
    view: "start-work",
    key: "gitea:acme/repo#142",
  });
  // Bare, it names no ticket, so there is nothing for the stepper to be about.
  expect(parseHash("#/start-work")).toEqual({ view: "unknown", hash: "#/start-work" });
});

/**
 * `#/inbox` was one of those until #45 and is a view now.
 *
 * The second assertion is the half graduating a word is exactly when somebody
 * stops thinking about: `inbox` must still never be read as an entity *kind*.
 * It reads as the view with its tail ignored, which is what `#/sources/x` and
 * `#/settings/x` do — a head-only view owns its whole address — and the thing
 * that must not happen is a detail slide-over over an entity called
 * `mock:PAY-231` of kind `inbox`.
 */
test("the inbox has an address, and `inbox` is still not a kind", () => {
  expect(parseHash("#/inbox")).toEqual({ view: "inbox", ctx: null });
  expect(parseHash("#/inbox/mock:PAY-231")).toEqual({ view: "inbox", ctx: null });
});

/**
 * `#/inbox/ctx/<id>` opens the inbox pre-filtered to one context (#47) — the
 * address the room's "N here" chip hands out, so what it advertises is what
 * it opens. A bare `ctx` tail is the whole stream, not a filter on nothing.
 */
test("the inbox address can carry a context filter", () => {
  expect(parseHash("#/inbox/ctx/ctx:5b1c")).toEqual({ view: "inbox", ctx: "ctx:5b1c" });
  expect(parseHash("#/inbox/ctx")).toEqual({ view: "inbox", ctx: null });
  expect(parseHash("#/inbox/ctx/")).toEqual({ view: "inbox", ctx: null });
});

test("round-trips every address it produces", () => {
  for (const hash of [
    "#/ctx/all",
    "#/ctx/src:jira",
    "#/ticket/mock:PAY-231",
    "#/entity/mock:PAY-231",
    "#/note/note:7f2c-4b2f",
    "#/sources",
    "#/settings",
    "#/first-run",
    "#/start-work/mock:PAY-231",
    "#/inbox",
    "#/inbox/ctx/ctx:5b1c",
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

/**
 * An adapter is free to declare a kind called `settings` (§3a: kinds are
 * open), and if it did, `#/settings/mock:s-1` would be an entity address —
 * which is why the word is reserved as well as handled: the view owns it, and
 * a kind by that name is unaddressable rather than ambiguous.
 */
test("settings is a view even with something after it, never a kind", () => {
  expect(parseHash("#/settings/mock:s-1")).toEqual({ view: "settings" });
});

/** Settings is not a room either, so Esc returns where the reader came from. */
test("visiting settings does not forget the room behind it", () => {
  location.hash = "#/ctx/src:jira";
  const router = createRouter();
  const stop = router.start();

  router.go("#/settings");
  expect(router.route).toEqual({ view: "settings" });
  expect(router.ctx).toBe("src:jira");
  router.back();
  expect(location.hash).toBe("#/ctx/src:jira");

  stop();
});

/**
 * `replace` is the navigation for a room that stopped existing under the
 * reader (#241): the address it leaves is *All work*'s, and the dead one is
 * not one step back in history — a `back` that landed on it would only fall
 * through to *All work* again, with the address bar naming a room that is
 * gone.
 *
 * jsdom grows `history.length` on a fragment navigation and leaves it alone
 * on `location.replace`, which is what lets the second assertion tell the two
 * apart.
 */
test("replace rewrites the address without leaving the old one in history", () => {
  location.hash = "#/ctx/proj:mock:PAY";
  const router = createRouter();
  const stop = router.start();
  const entries = history.length;

  router.replace({ view: "room", ctx: "all", detail: null });

  expect(location.hash).toBe("#/ctx/all");
  expect(router.ctx).toBe("all");
  expect(history.length, "the dead address must not be one step back").toBe(entries);
  router.back();
  expect(location.hash).toBe("#/ctx/all");

  stop();
});

/**
 * A detail address does not carry its room, so handing the room to *All
 * work* under an open slide-over changes what the router remembers and
 * nothing the address bar shows: the detail stays open, and `Esc` now lands
 * in *All work* rather than on the dead id.
 */
test("replace under an open detail keeps the detail and moves the remembered room", () => {
  location.hash = "#/ctx/proj:mock:PAY";
  const router = createRouter();
  const stop = router.start();
  router.go("#/ticket/mock:PAY-231");
  const entries = history.length;

  router.replace({
    view: "room",
    ctx: "all",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });

  expect(location.hash).toBe("#/ticket/mock:PAY-231");
  expect(router.ctx).toBe("all");
  expect(router.route).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });
  expect(history.length).toBe(entries);
  router.back();
  expect(location.hash).toBe("#/ctx/all");

  stop();
});
