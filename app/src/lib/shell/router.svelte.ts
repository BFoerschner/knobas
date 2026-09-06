/**
 * Hash addressing — spec §2 "Entity addressing", adopted as the navigation
 * contract.
 *
 * | Address                | Meaning                                          |
 * |------------------------|--------------------------------------------------|
 * | `#/ctx/<id>`           | a room: `all`, `src:<id>`, `proj:<id>:<key>`,    |
 * |                        | or a stored context's own id                     |
 * | `#/<kind>/<entity_id>` | the detail slide-over over the current room      |
 * | `#/entity/<entity_id>` | kind-agnostic alias, resolved via `get_entity`   |
 * | `#/assets/tree`        | the Assets view, on its Tree tab (#428)          |
 * | `#/asset/<entity_id>`  | the Tree, opened at one asset (#428)             |
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
 * `#/route/*` and `#/monitor/*` are M4.0's routes (#432) and M4.1's monitors.
 * They parse to `unknown` rather than being mistaken for kinds, so the shell
 * can say which milestone they arrive in. `#/start-work/*` was one of them
 * until #44 and is now a view of its own — it stays in `RESERVED` so that an
 * adapter declaring a `start-work` *kind* could never take the address.
 * `#/inbox` graduated the same way with #45, `#/time` with #279, `#/standup`
 * with #288, and `#/assets` and `#/asset` with #428.
 */

/**
 * The Assets view's tabs — `CONTEXT.md`, **Tree**: *"its sibling tab is
 * Monitors"*.
 *
 * One member today, and deliberately not a bare string: `"monitors"` arrives
 * with M4.1 and every place that branches on the tab has to fail
 * `svelte-check` when it does, rather than fall through to the Tree.
 *
 * Never "board" — ADR-0009, which is why the first tab is called *Tree* at
 * all.
 */
export type AssetsTab = "tree";

export type Route =
  | {
      view: "room";
      ctx: string;
      /** `kind: null` is the `#/entity/<id>` alias: the kind is not known yet. */
      detail: { kind: string | null; entityId: string } | null;
    }
  /**
   * The Assets view (#428): the estate, on one of its tabs.
   *
   * `assetId` is the asset the Tree opens at, and it is *not* a second view:
   * `#/asset/<id>` and `#/assets/tree` draw the same surface, one of them with
   * a selection. Story 35 — *opening an asset's address re-opens the Tree at
   * its path* — is that sentence, and a separate detail view would have made
   * it a second surface to keep in step with the first.
   */
  | { view: "assets"; tab: AssetsTab; assetId: string | null }
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

/** The room every session starts in. */
export const DEFAULT_CTX = "all";

/**
 * First segments that are *not* a kind.
 *
 * Two groups, and the distinction matters.
 * `ctx`/`sources`/`settings`/`first-run`/`entity` are views that exist. The
 * rest are addresses M2–M4 will claim; they are listed now so that today they
 * render "arrives in M<n>" instead of being taken for an entity kind and sent
 * to `get_entity`, which would 404 on a word.
 */
const RESERVED = new Set([
  "ctx",
  "sources",
  "settings",
  "first-run",
  "entity",
  // The inbox is a view now (#45): `#/inbox` reaches `InboxView` rather than
  // reading "arrives in a later milestone". It stays in this list because a
  // *kind* called `inbox` must still never claim the address — the same
  // reasoning the M2-M4 group below is here for, applied to a word that has
  // stopped waiting.
  "inbox",
  // `time` is a view now (#279): `#/time/2026-09-03` reaches `DayReview`
  // rather than reading "arrives in a later milestone". It stays in this list
  // for the reason `inbox` does -- a *kind* called `time` must still never
  // claim the address.
  "time",
  // `standup` is a view now (#288): `#/standup` reaches `StandupView` rather
  // than reading "arrives in a later milestone". It stays in this list for the
  // reason `time` does -- a *kind* called `standup` must still never claim the
  // address.
  "standup",
  // M4, reserved so an open kind never collides with a view.
  //
  // `note` was here until #46 and is not any more: notes are a kind now, so
  // `#/note/note:<uuid>` has to reach `NoteView` the way `#/ticket/<id>`
  // reaches `Detail`. It is the one word in this list that stopped being a
  // *view* and became a kind, which is exactly what this list is arranged
  // around -- so a later tidy-up that adds it back is a note nobody can open.
  "assets",
  "asset",
  "route",
  "monitor",
  "start-work",
]);

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
 * `ctx` is the room a detail address was opened over — the address itself does
 * not carry it, and `Esc` has to return to the room the reader came from.
 */
export function parseHash(hash: string, ctx: string = DEFAULT_CTX): Route {
  const path = hash.replace(/^#\/?/, "");
  const segments = path.split("/");
  const head = segments[0] ?? "";
  const tail = segments.slice(1).map(decode).join("/");

  if (head === "") return { view: "room", ctx, detail: null };
  if (head === "ctx") return { view: "room", ctx: tail || DEFAULT_CTX, detail: null };
  if (head === "inbox") {
    // `#/inbox/ctx/<id>` opens the view with the per-context filter already
    // set (#47) — what the room's "N here" chip advertises has to be what it
    // opens. The bare address stays the whole stream.
    const ctx = segments[1] === "ctx" ? segments.slice(2).map(decode).join("/") : "";
    return { view: "inbox", ctx: ctx === "" ? null : ctx };
  }
  // Before the open-kind branch, like `start-work` below and for the same
  // reason: `time` is in `RESERVED`, so this is the only thing that can reach
  // the view.
  //
  // A tail that is not a day is **today**, not `unknown`. The head owns the
  // whole address -- the rule `#/sources/x` and `#/settings/x` already follow
  // -- and "arrives in a later milestone" is the one thing `#/time/whenever`
  // must not say, now that it does not.
  if (head === "time") {
    const day = segments[1] ?? "";
    return { view: "time", day: DAY.test(day) ? day : null };
  }
  // Before the open-kind branch, for the reason `time` above it is: `standup`
  // is in `RESERVED`, so this is the only thing that can reach the view. The
  // head owns the whole address -- `#/standup/anything` is the digest, the
  // same rule `#/sources/x` and `#/time/whenever` already follow.
  if (head === "standup") return { view: "standup" };
  // Before the open-kind branch, for the reason `time` and `standup` are:
  // both words are in `RESERVED`, so these are the only things that can reach
  // the view. The head owns the whole address, so `#/assets/whatever` is the
  // Tree rather than "arrives in a later milestone" -- the rule `#/sources/x`
  // and `#/time/whenever` already follow. `#/asset` with no id is *not* the
  // view: an address that sets out to name an asset and does not is a typo,
  // not the estate.
  if (head === "assets") return { view: "assets", tab: "tree", assetId: null };
  if (head === "asset") {
    return tail === ""
      ? { view: "unknown", hash }
      : { view: "assets", tab: "tree", assetId: tail };
  }
  if (head === "sources") return { view: "sources" };
  if (head === "settings") return { view: "settings" };
  if (head === "first-run") return { view: "first-run" };
  // Before the open-kind branch and before `RESERVED` is consulted: the word is
  // in that set so no adapter's kind can claim the address, which would
  // otherwise make this unreachable.
  if (head === "start-work") {
    return tail === "" ? { view: "unknown", hash } : { view: "start-work", key: tail };
  }
  if (head === "entity") {
    return { view: "room", ctx, detail: { kind: null, entityId: tail } };
  }
  // Open kinds (§3a): anything not reserved is a kind. A closed list here
  // would make an adapter's new kind unaddressable until the shell shipped
  // again.
  if (!RESERVED.has(head) && tail !== "") {
    return { view: "room", ctx, detail: { kind: head, entityId: tail } };
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
    case "room": {
      if (!route.detail) return `#/ctx/${route.ctx}`;
      const kind = route.detail.kind ?? "entity";
      return `#/${kind}/${encodeId(route.detail.entityId)}`;
    }
  }
}

/** The live route, and the only way to change it. */
export interface Router {
  readonly route: Route;
  /** The room a detail is drawn over — what `Esc` returns to. */
  readonly ctx: string;
  /** Navigate. With {@link replace}, the only navigator: nothing else writes `location.hash`. */
  go(hash: string): void;
  /**
   * Navigate **in place**: the entry the reader is on is rewritten rather
   * than a new one pushed, so the address being left is not one step back in
   * history (#241).
   *
   * A `Route` rather than a hash, and the difference is the room: a detail
   * address does not carry the room it is drawn over, so the only way to hand
   * an open slide-over to another room is to say which. The address bar then
   * shows the same detail address it did, and `back()` lands in the room the
   * route names.
   */
  replace(route: Route): void;
  /** One rung of the unwind ladder: back to the room. */
  back(): void;
  /** Install the `hashchange` listener. Returns its teardown. */
  start(): () => void;
}

export function createRouter(): Router {
  const state = $state<{ route: Route; ctx: string }>({
    route: parseHash(location.hash),
    ctx: DEFAULT_CTX,
  });

  function read() {
    const next = parseHash(location.hash, state.ctx);
    // The room is remembered across a detail address, which does not carry
    // one: opening a ticket from `src:jira` and pressing Esc has to land back
    // in `src:jira`, not in `all`.
    if (next.view === "room") state.ctx = next.ctx;
    state.route = next;
  }

  read();

  return {
    get route() {
      return state.route;
    },
    get ctx() {
      return state.ctx;
    },
    go(hash: string) {
      if (location.hash !== hash) location.hash = hash;
      // Read eagerly rather than waiting for `hashchange`, which browsers fire
      // on a later task: without this every navigation would render a frame
      // late, and assigning an *unchanged* hash fires no event at all, so a
      // repeated navigation would never render. The listener below is for the
      // changes this method did not make — back, forward, a typed address —
      // and `read()` is idempotent, so the duplicate is free.
      read();
    },
    replace(route: Route) {
      const hash = hashFor(route);
      if (location.hash !== hash) location.replace(hash);
      // Set before `read()` rather than left to it: the hash may be the same
      // detail address as before, which `read()` parses against whatever room
      // is remembered.
      if (route.view === "room") state.ctx = route.ctx;
      read();
    },
    back() {
      this.go(`#/ctx/${state.ctx}`);
    },
    start() {
      const onhashchange = () => read();
      window.addEventListener("hashchange", onhashchange);
      read();
      return () => window.removeEventListener("hashchange", onhashchange);
    },
  };
}

/** The one the window uses. Tests build their own with `createRouter`. */
export const router = createRouter();
