/**
 * Hash addressing (spec §2 "Entity addressing"), adopted as the navigation
 * contract.
 *
 * The one non-obvious rule has its own test: a `#` inside an entity key —
 * Gitea's `owner/repo#142`, TeamCity's `Build#1188` — truncates the fragment
 * unless it is percent-encoded. `hashFor` encodes, `parseHash` decodes, and
 * the two have to stay each other's inverse.
 */
import { expect, test, vi } from "vitest";

import { createRouter, hashFor, parseHash } from "./router.svelte";

test("parses the M1 addresses", () => {
  expect(parseHash("#/sources")).toEqual({ view: "sources" });
  expect(parseHash("#/settings")).toEqual({ view: "settings" });
  expect(parseHash("#/first-run")).toEqual({ view: "first-run" });
});

/**
 * The rooms UI's addresses have no view here any more (#1124): `legacyAddress`
 * in `tf/address.ts` rewrites them before the shell answers *Nothing lives at*.
 */
test("a rooms-UI address parses to unknown", () => {
  for (const hash of ["", "#", "#/", "#/ctx/all", "#/ticket/mock:PAY-231", "#/entity/mock:PAY-231"]) {
    expect(parseHash(hash)).toEqual({ view: "unknown", hash });
  }
});

/** M4 addresses parse, so the shell can say "arrives in M<n>" rather than
 * rendering a blank screen or, worse, treating `monitor` as a kind.
 *
 * `#/time` left this list with #279, `#/standup` with #288, `#/assets` and
 * `#/asset` with #428 and `#/route` with #432; each has a view of its own now,
 * and the tests around this one are where they went. The two **bare** ones are
 * here rather than there deliberately: an address that sets out to name an
 * asset or a route and names none is a typo, not the estate. */
test("a later milestone's address is known-unknown, not a kind", () => {
  for (const hash of ["#/monitor/db-1", "#/asset/", "#/asset", "#/route", "#/route/"]) {
    expect(parseHash(hash)).toEqual({ view: "unknown", hash });
  }
});

/**
 * A route's address is the Tree as well (#432), and it carries the route
 * rather than an asset.
 *
 * `assetId` is `null` because the address does not carry one: the view reads
 * the route and opens at the asset exposing it. The round trip is what keeps
 * the more specific address from being flattened to the asset's on the way
 * back out — a reader who copies a link to a route must get a link to the
 * route.
 */
test("a route's address reaches the Tree, carrying the route", () => {
  expect(parseHash("#/route/route:9a1b")).toEqual({
    view: "assets",
    tab: "tree",
    assetId: null,
    routeId: "route:9a1b",
  });
  expect(hashFor(parseHash("#/route/route:9a1b"))).toBe("#/route/route:9a1b");
  // The route wins over an asset id where an address carries both, because it
  // is the more specific of the two.
  expect(
    hashFor({ view: "assets", tab: "tree", assetId: "asset:7f2c", routeId: "route:9a1b" }),
  ).toBe("#/route/route:9a1b");
});

/**
 * The Assets view has an address of its own (#428), and `#/asset/<id>` is the
 * same view with a selection rather than a second one.
 *
 * The head owns the whole address, the rule `#/sources/x` and `#/time/whenever`
 * already follow: `#/assets/anything` is the Tree, because "arrives in a later
 * milestone" is the one thing it must not say now that it does not. The one
 * tail with a meaning of its own is `monitors` (#448), which is the sibling
 * tab and not a second view — same surface, different tab, so the address is
 * `#/assets/<tab>` and nothing more.
 */
test("the assets addresses reach the Tree, with and without a selection", () => {
  expect(parseHash("#/assets/tree")).toEqual({ view: "assets", tab: "tree", assetId: null });
  expect(parseHash("#/assets")).toEqual({ view: "assets", tab: "tree", assetId: null });
  expect(parseHash("#/assets/whatever")).toEqual({
    view: "assets",
    tab: "tree",
    assetId: null,
  });
  expect(parseHash("#/asset/asset:7f2c")).toEqual({
    view: "assets",
    tab: "tree",
    assetId: "asset:7f2c",
  });
  // Round trip: the id keeps its namespace colon and everything else is
  // encoded, the rule every entity address in this module follows.
  expect(hashFor(parseHash("#/asset/asset:7f2c"))).toBe("#/asset/asset:7f2c");
  expect(hashFor({ view: "assets", tab: "tree", assetId: null })).toBe("#/assets/tree");
});

/**
 * The Monitors tab is a destination (#448), which is the whole reason the tab
 * rides in the address: a reader who has filtered the roster to *down* can
 * hand somebody the link they are looking at.
 *
 * It round-trips, and it does **not** carry an asset: `#/asset/<id>` opens the
 * Tree at that asset, and there is no asset a monitor roster is "at".
 */
test("the monitors address reaches the same view on its other tab, and round-trips", () => {
  expect(parseHash("#/assets/monitors")).toEqual({
    view: "assets",
    tab: "monitors",
    assetId: null,
  });
  expect(hashFor({ view: "assets", tab: "monitors", assetId: null })).toBe("#/assets/monitors");
  expect(hashFor(parseHash("#/assets/monitors"))).toBe("#/assets/monitors");
  // An asset's address is the Tree's, whatever tab the reader was on: a
  // selection is a thing the roster does not have.
  expect(hashFor({ view: "assets", tab: "monitors", assetId: "asset:7f2c" })).toBe(
    "#/asset/asset:7f2c",
  );
});

/**
 * The digest has an address of its own (#288), and the head owns the whole of
 * it.
 *
 * A tail is *this morning's* digest all the same: the digest is defined
 * against today and there is nothing for a date segment to mean.
 */
test("the standup address reaches its own view, tail or no tail", () => {
  expect(parseHash("#/standup")).toEqual({ view: "standup" });
  expect(parseHash("#/standup/2026-08-31")).toEqual({ view: "standup" });
  expect(hashFor({ view: "standup" })).toBe("#/standup");
});

/**
 * The flow's address is its identity, so it carries a ticket id and reaches a
 * view of its own -- not the "arrives in a later milestone" pane it reached
 * before #44.
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

/**
 * The day review's address (#279). `time` was a reserved M3 word until this
 * ticket; the second and third assertions are the half graduating a word is
 * exactly when somebody stops thinking about.
 *
 * A tail that is not a day is **today**, not `unknown`: the head owns the
 * whole address, the way `#/sources/x` and `#/settings/x` already do, and
 * "arrives in a later milestone" is the one thing this address must not say
 * any more.
 */
test("the day review has an address, and `time` is still not a kind", () => {
  expect(parseHash("#/time/2026-09-03")).toEqual({ view: "time", day: "2026-09-03" });
  expect(parseHash("#/time")).toEqual({ view: "time", day: null });
  expect(parseHash("#/time/whenever")).toEqual({ view: "time", day: null });
  // The shape is checked, not the calendar -- `Date` in the view owns that.
  expect(parseHash("#/time/2026-02-31")).toEqual({ view: "time", day: "2026-02-31" });
  // And the thing that must not happen: a detail slide-over over an entity of
  // kind `time`.
  expect(parseHash("#/time/mock:PAY-231")).toEqual({ view: "time", day: null });
});

test("round-trips every address it produces", () => {
  for (const hash of [
    "#/time",
    "#/time/2026-09-03",
    "#/standup",
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
  const parsed = parseHash("#/start-work/mock:payout-service%23142");
  expect(parsed).toEqual({ view: "start-work", key: "mock:payout-service#142" });
  // ...and building the address back encodes it again, or the browser drops
  // everything from the `#` onwards.
  expect(hashFor(parsed)).toBe("#/start-work/mock:payout-service%23142");
});

test("a slash inside a key is encoded rather than read as another segment", () => {
  const hash = hashFor({ view: "start-work", key: "gitea:acme/payout-service#142" });
  expect(hash).toBe("#/start-work/gitea:acme%2Fpayout-service%23142");
  expect(parseHash(hash)).toEqual({
    view: "start-work",
    key: "gitea:acme/payout-service#142",
  });
});

test("a malformed escape is treated as literal text rather than throwing", () => {
  // `decodeURIComponent("%zz")` throws. A hash the user typed must not take
  // the window down.
  expect(parseHash("#/start-work/mock:%zz")).toEqual({ view: "start-work", key: "mock:%zz" });
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

/**
 * `replace` rewrites the entry the reader is on (#241): the address it leaves
 * is not one step back in history.
 *
 * jsdom grows `history.length` on a fragment navigation and leaves it alone
 * on `location.replace`, which is what lets the assertion tell the two apart.
 */
test("replace rewrites the address without leaving the old one in history", () => {
  location.hash = "#/sources";
  const router = createRouter();
  const stop = router.start();
  const entries = history.length;

  router.replace({ view: "settings" });

  expect(location.hash).toBe("#/settings");
  expect(router.route).toEqual({ view: "settings" });
  expect(history.length, "the old address must not be one step back").toBe(entries);

  stop();
});

/**
 * Back and forward, like a browser (#693).
 *
 * The window cannot ask the webview whether there is anywhere to go forward
 * to, so the router keeps its own place in history: one step per `go` to a
 * new address, none for `replace`, and the place read back off the entry on a
 * traversal. jsdom traverses on a later task, so each step waits for the
 * router to have heard it.
 */
test("back and forward are available exactly where the history has somewhere to go", async () => {
  location.hash = "#/inbox";
  const router = createRouter();
  const stop = router.start();
  expect([router.canGoBack, router.canGoForward], "a fresh session").toEqual([false, false]);

  router.go("#/time/2026-09-01");
  router.go("#/time/2026-09-02");
  expect([router.canGoBack, router.canGoForward]).toEqual([true, false]);
  // The same address again is not a step.
  router.go("#/time/2026-09-02");

  router.goBack();
  await vi.waitFor(() => expect(location.hash).toBe("#/time/2026-09-01"));
  await vi.waitFor(() => expect(router.route).toEqual({ view: "time", day: "2026-09-01" }));
  expect([router.canGoBack, router.canGoForward]).toEqual([true, true]);

  router.goForward();
  await vi.waitFor(() => expect(location.hash).toBe("#/time/2026-09-02"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(false));
  expect(router.canGoBack).toBe(true);

  // `replace` rewrites where the reader stands and is not a step: back still
  // lands on the entry before it, and forward stays empty.
  router.replace({ view: "standup" });
  expect([router.canGoBack, router.canGoForward]).toEqual([true, false]);

  router.goBack();
  await vi.waitFor(() => expect(location.hash).toBe("#/time/2026-09-01"));
  router.goBack();
  await vi.waitFor(() => expect(location.hash).toBe("#/inbox"));
  await vi.waitFor(() => expect(router.canGoBack).toBe(false));
  expect(router.canGoForward).toBe(true);

  // A new address from the middle of history drops what was ahead of it.
  router.go("#/sources");
  expect([router.canGoBack, router.canGoForward]).toEqual([true, false]);

  // `replace` in the middle of history keeps what is ahead of it, as a
  // browser's does: it rewrites one entry and steps nowhere.
  router.goBack();
  await vi.waitFor(() => expect(location.hash).toBe("#/inbox"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(true));
  router.replace({ view: "settings" });
  expect([router.canGoBack, router.canGoForward]).toEqual([false, true]);
  router.goForward();
  await vi.waitFor(() => expect(location.hash).toBe("#/sources"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(false));

  // Nowhere to go is a no-op, not a step out of the session.
  router.goForward();
  expect(location.hash).toBe("#/sources");
  stop();
});

/**
 * The webview's `hashchange` for an address `go()` already read keeps the
 * route as it is (#703). A new object for the same place re-ran everything
 * that follows the route.
 */
test("the hashchange for an address go() already read keeps the same route", () => {
  location.hash = "#/inbox";
  const router = createRouter();
  const stop = router.start();

  router.go("#/start-work/gitea:tidewater/ledger-api");
  const read = router.route;
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  expect(router.route).toBe(read);

  router.go("#/start-work/gitea:tidewater/other");
  expect(router.route).not.toBe(read);

  stop();
});
