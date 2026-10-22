/**
 * Hash addressing — spec §2 "Entity addressing", adopted as the navigation
 * contract.
 *
 * | Address                | Meaning                                          |
 * |------------------------|--------------------------------------------------|
 * | `#/assets/tree`       | the Assets view's Tree (#428)                    |
 * | `#/asset/<entity_id>`  | the Tree, opened at one asset (#428)             |
 * | `#/route/<entity_id>`  | the Tree, at the asset exposing one route (#432) |
 * | `#/inbox`              | the inbox — one actionable stream (#45)          |
 * | `#/inbox/ctx/<id>`     | the inbox, pre-filtered to one context (#47)     |
 * | `#/time/<YYYY-MM-DD>`  | the day review for one day (#279)                |
 * | `#/time`               | the day review, on today                         |
 * | `#/standup`            | the standup digest, on today (#288)              |
 * | `#/sources`            | the sources view                                 |
 * | `#/settings`           | the settings view                                |
 * | `#/first-run`          | the §14a wizard                                  |
 *
 * **Deviation from the mockup, recorded here:** the mockup addressed
 * `#/ticket/PAY-231`. M1 ids are namespaced (`mock:PAY-231`) because P10 makes
 * the instance id part of the entity id, and two Jiras (`jira`, `jira-eu`)
 * would otherwise collide on one key. `:` is legal in a URI fragment, so the
 * address stays readable.
 *
 * `#/monitor/*` is M4.1's and parses to `unknown`;
 * `#/route/*` graduated out of that group with #432 and opens the Tree at the
 * asset exposing the route. `#/start-work/*` was one of them
 * until #44 and is now a view of its own.
 * `#/inbox` graduated the same way with #45, `#/time` with #279, `#/standup`
 * with #288, and `#/assets` and `#/asset` with #428.
 */

/**
 * The Assets view's tab: the Tree, and only the Tree.
 *
 * *Monitors* was its sibling from #448 until #1459: the roster it named went
 * with the rooms UI (#1087), and Monitors lives at `#/monitors` (R-11.3), where
 * `tf/address.ts`' `legacyAddress` sends `#/assets/monitors`. One member, and
 * still a union rather than a bare string, so a tab that ever returns has to
 * be named here and fails `svelte-check` wherever the tab is read.
 *
 * Never "board" — ADR-0009, which is why the tab is called *Tree* at all.
 */
export type AssetsTab = "tree";

export type Route =
  /**
   * The Assets view (#428): the estate, on one of its tabs.
   *
   * `assetId` is the asset the Tree opens at, and it is *not* a second view:
   * `#/asset/<id>` and `#/assets/tree` draw the same surface, one of them with
   * a selection. Story 35 — *opening an asset's address re-opens the Tree at
   * its path* — is that sentence, and a separate detail view would have made
   * it a second surface to keep in step with the first.
   */
  | {
      view: "assets";
      tab: AssetsTab;
      assetId: string | null;
      /**
       * The route `#/route/<id>` names (#432), when the address is a route's.
       *
       * **Optional rather than `string | null`**, and that is the one place
       * this union is not uniform. An assets address either names a route or
       * has nothing to say about routes at all: `#/assets/tree` and
       * `#/asset/<id>` are not "a route address with no route", they are
       * addresses of a different thing. Writing it as an optional says that,
       * and it keeps every existing construction of this variant — the top
       * strip's button, the Tree's own `addressOf` — unchanged.
       *
       * The Tree resolves it to the exposing asset (`get_route`) and opens
       * there with the route selected; `assetId` is `null` on the way in
       * because the address does not carry one.
       */
      routeId?: string;
    }
  | {
      view: "inbox";
      /** A stored context to pre-filter by (#47), or `null` for the whole stream. */
      ctx: string | null;
    }
  | { view: "sources" }
  | { view: "settings" }
  /**
   * The day review (#279): one day's blocks on a strip, editable.
   *
   * `day` is `YYYY-MM-DD` **in the reader's own timezone**, or `null` for
   * today. `null` rather than today's date filled in here on purpose: this
   * module is pure, and a parse that read the clock would give `#/time` a
   * different meaning depending on when it was parsed -- including across a
   * midnight the reader is sitting through. The view resolves `null` against
   * its own clock, which is also the only place a change of day can be
   * noticed.
   */
  | { view: "time"; day: string | null }
  /**
   * The standup digest (#288): yesterday, today and blockers, for today.
   *
   * No day segment, and that is a decision rather than an omission. A digest
   * is *this morning's* standup — `CONTEXT.md`'s **yesterday** is defined
   * relative to today and the running timer only means anything now — so
   * there is no date for the address to carry. `#/standup/<date>` is the
   * standup **protocol**'s address, which is a note per date and a different
   * surface; keeping this one bare leaves that segment free for it.
   */
  | { view: "standup" }
  | { view: "first-run" }
  /**
   * The start-work stepper for one ticket (#44). `key` is the ticket's entity
   * id — the flow starts from a ticket and is keyed on it, so the address is
   * the flow's identity as well as its location.
   */
  | { view: "start-work"; key: string }
  | { view: "unknown"; hash: string };

/**
 * A day in the address: `YYYY-MM-DD`, four-two-two.
 *
 * A shape check, not a calendar check. `#/time/2026-02-31` parses here and is
 * resolved by `Date` in the view, which is where a real calendar lives; what
 * this keeps out is `#/time/whenever`, which would otherwise become a day
 * whose bounds are two `Invalid Date`s and a strip that silently shows
 * nothing.
 */
const DAY = /^\d{4}-\d{2}-\d{2}$/;

/**
 * `decodeURIComponent` that survives what a person can type.
 *
 * A malformed escape (`%zz`) throws a `URIError`, and a hash comes from the
 * address bar as readily as from `go()`. Taking the window down over one is
 * not a trade worth making.
 */
function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

/**
 * Read an address.
 *
 * An address this module has no view for is `unknown`, including the ones the
 * rooms UI handed out (`#/`, `#/ctx/…`, `#/<kind>/<id>`); `legacyAddress` in
 * `tf/address.ts` rewrites those.
 */
export function parseHash(hash: string): Route {
  const path = hash.replace(/^#\/?/, "");
  const segments = path.split("/");
  const head = segments[0] ?? "";
  const tail = segments.slice(1).map(decode).join("/");

  if (head === "inbox") {
    // `#/inbox/ctx/<id>` opens the view with the per-context filter already
    // set (#47) — what the room's "N here" chip advertises has to be what it
    // opens. The bare address stays the whole stream.
    const ctx = segments[1] === "ctx" ? segments.slice(2).map(decode).join("/") : "";
    return { view: "inbox", ctx: ctx === "" ? null : ctx };
  }
  // A tail that is not a day is **today**, not `unknown`. The head owns the
  // whole address -- the rule `#/sources/x` and `#/settings/x` already follow
  // -- and "arrives in a later milestone" is the one thing `#/time/whenever`
  // must not say, now that it does not.
  if (head === "time") {
    const day = segments[1] ?? "";
    return { view: "time", day: DAY.test(day) ? day : null };
  }
  // The head owns the whole address -- `#/standup/anything` is the digest, the
  // same rule `#/sources/x` and `#/time/whenever` already follow.
  if (head === "standup") return { view: "standup" };
  // The head owns the whole address, so `#/assets/whatever` is the
  // Tree rather than "arrives in a later milestone" -- the rule `#/sources/x`
  // and `#/time/whenever` already follow. `#/asset` with no id is *not* the view: an address
  // that sets out to name an asset and does not is a typo, not the estate.
  if (head === "assets") return { view: "assets", tab: "tree", assetId: null };
  if (head === "asset") {
    return tail === ""
      ? { view: "unknown", hash }
      : { view: "assets", tab: "tree", assetId: tail };
  }
  // A route's address is the **Tree**, not a surface of its own (#432): a
  // route sits on the asset that exposes it, and story 14's *a ticket about a
  // certificate links to the route whose certificate it is* has to land
  // somewhere a reader can see what the route belongs to. `assetId` is `null`
  // because the address does not carry one; the view reads the route and
  // finds it. `#/route` with no id is a typo for the same reason `#/asset` is.
  if (head === "route") {
    return tail === ""
      ? { view: "unknown", hash }
      : { view: "assets", tab: "tree", assetId: null, routeId: tail };
  }
  if (head === "sources") return { view: "sources" };
  if (head === "settings") return { view: "settings" };
  if (head === "first-run") return { view: "first-run" };
  if (head === "start-work") {
    return tail === "" ? { view: "unknown", hash } : { view: "start-work", key: tail };
  }
  return { view: "unknown", hash };
}

/**
 * Percent-encode an entity id, but leave its namespace colon alone.
 *
 * `encodeURIComponent` escapes `:` to `%3A`. It is legal in a URI fragment and
 * it is the separator every entity id carries (`mock:PAY-231`), so escaping it
 * would turn every address in the app into line noise for no gain. Everything
 * else stays encoded — `#` truncates the fragment at the browser level and `/`
 * reads as another path segment.
 */
function encodeId(id: string): string {
  return encodeURIComponent(id).replace(/%3A/g, ":");
}

/**
 * Build an address.
 *
 * The encoding is the point. Gitea keys carry `#` (`acme/payout-service#142`)
 * and `/`; unencoded, the first truncates the fragment at the browser level
 * and the second reads as another path segment.
 */
export function hashFor(route: Route): string {
  switch (route.view) {
    case "inbox":
      return route.ctx === null ? "#/inbox" : `#/inbox/ctx/${encodeId(route.ctx)}`;
    case "time":
      return route.day === null ? "#/time" : `#/time/${route.day}`;
    case "standup":
      return "#/standup";
    case "assets":
      // The route's address wins where there is one: it is the more specific
      // of the two, and it is what a reader copies out of the pane.
      if (route.routeId !== undefined) return `#/route/${encodeId(route.routeId)}`;
      return route.assetId === null
        ? `#/assets/${route.tab}`
        : `#/asset/${encodeId(route.assetId)}`;
    case "sources":
      return "#/sources";
    case "settings":
      return "#/settings";
    case "first-run":
      return "#/first-run";
    case "start-work":
      return `#/start-work/${encodeId(route.key)}`;
    case "unknown":
      return route.hash;
  }
}

