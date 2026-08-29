/**
 * Hash addressing — spec §2 "Entity addressing", adopted as the navigation
 * contract.
 *
 * | Address                | Meaning                                          |
 * |------------------------|--------------------------------------------------|
 * | `#/ctx/<id>`           | a room. M1 ids: `all`, and `src:<source_id>`     |
 * | `#/<kind>/<entity_id>` | the detail slide-over over the current room      |
 * | `#/entity/<entity_id>` | kind-agnostic alias, resolved via `get_entity`   |
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
 * `#/inbox`, `#/time`, `#/standup`, `#/assets/*`, `#/route/*` and `#/monitor/*`
 * are M2–M4. They parse to `unknown` rather than being mistaken for kinds, so
 * the shell can say which milestone they arrive in. `#/start-work/*` was one of
 * them until #44 and is now a view of its own — it stays in `RESERVED` so that
 * an adapter declaring a `start-work` *kind* could never take the address.
 */

export type Route =
  | {
      view: "room";
      ctx: string;
      /** `kind: null` is the `#/entity/<id>` alias: the kind is not known yet. */
      detail: { kind: string | null; entityId: string } | null;
    }
  | { view: "sources" }
  | { view: "settings" }
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
  // M2-M4, reserved so an open kind never collides with a view.
  //
  // `note` was here until #46 and is not any more: notes are a kind now, so
  // `#/note/note:<uuid>` has to reach `NoteView` the way `#/ticket/<id>`
  // reaches `Detail`. It is the one word in this list that stopped being a
  // *view* and became a kind, which is exactly what this list is arranged
  // around -- so a later tidy-up that adds it back is a note nobody can open.
  "inbox",
  "time",
  "standup",
  "assets",
  "asset",
  "route",
  "monitor",
  "start-work",
]);

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
  /** Navigate. The only navigator: nothing else writes `location.hash`. */
  go(hash: string): void;
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
